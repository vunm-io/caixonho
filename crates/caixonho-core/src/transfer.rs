//! Moving object content between the service and this machine.
//!
//! This module owns the parts of a transfer that must not be improvised
//! per call site: what a key is called on disk, and (later in this change)
//! the write that cannot leave a partial file where the real one belongs.

/// What producing a local name did to the key's final segment.
///
/// Ordered by how much the caller must tell the user: `Unchanged` needs no
/// words, `Substituted` means characters were percent-encoded, `Suffixed`
/// means the name alone could not carry the key (empty, reserved, or
/// overlong) and a deterministic suffix derived from the full key was added.
/// `object-transfer` spec: every substitution or collision is reported —
/// this enum is the report's raw material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingOutcome {
    /// The key's final segment is usable as-is on every shipped platform.
    Unchanged,
    /// One or more bytes were percent-encoded.
    Substituted,
    /// A deterministic suffix was appended (implies the name also differs
    /// from the raw segment in shape, whether or not bytes were encoded).
    Suffixed,
}

/// A key's local name, together with what it took to produce it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapped {
    /// The filename to use on disk. Never empty, never a path.
    pub name: String,
    /// What was done. [`MappingOutcome::Unchanged`] means `name` equals the
    /// key's final segment byte for byte.
    pub how: MappingOutcome,
}

/// The name a key gets on disk — one scheme, every platform.
///
/// The scheme (ADR-0004, in brief):
///
/// - The name is the key's final `/`-separated segment.
/// - Bytes no shipped filesystem accepts are **percent-encoded**, never
///   replaced with a stand-in: encoding is injective, so two segments that
///   differ can never merge into one name. The set is Windows's refused
///   punctuation, control bytes, the separator pair, and `%` itself —
///   Unix would allow most of them, but one scheme on every platform means
///   a download made on a Mac has the name it would have had on Windows.
/// - A trailing `.` or space is encoded (Windows strips them on create, so
///   `name.` and `name` would silently be one file).
/// - A segment that cannot serve at all — empty, `.`/`..`, a reserved DOS
///   device name, or overlong — keeps what it can and takes a suffix
///   derived from the **full key** by FNV-1a, so distinctness the segment
///   lost is carried by where it came from. FNV is implemented inline
///   because it must be stable across platforms and releases, which the
///   standard library's hasher does not promise.
///
/// No `cfg` anywhere: the platform question was decided once, above.
pub fn local_name(key: &str) -> Mapped {
    let segment = key.rsplit('/').next().unwrap_or("");

    let mut name = String::with_capacity(segment.len());
    let mut substituted = false;
    let bytes = segment.as_bytes();
    for (i, ch) in segment.char_indices() {
        let last = i + ch.len_utf8() == bytes.len();
        let refused = matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '%')
            || ch.is_control()
            || (last && matches!(ch, '.' | ' '));
        if refused {
            substituted = true;
            let mut utf8 = [0u8; 4];
            for byte in ch.encode_utf8(&mut utf8).as_bytes() {
                name.push_str(&format!("%{byte:02X}"));
            }
        } else {
            name.push(ch);
        }
    }

    // `.` and `..` encode their *trailing* dot above and stop being special;
    // asserted rather than assumed, because the empty check below depends on
    // nothing else reaching it in that shape.
    debug_assert!(name != "." && name != "..");

    let empty = name.is_empty();
    let reserved = is_reserved_device(&name);
    let overlong = name.len() > MAX_NAME_BYTES;

    if !(empty || reserved || overlong) {
        return Mapped {
            name,
            how: if substituted {
                MappingOutcome::Substituted
            } else {
                MappingOutcome::Unchanged
            },
        };
    }

    // The suffix carries what the segment alone cannot. Derived from the
    // full key so `reports/daily/` and `logs/daily/` stay apart.
    let suffix = format!("-{:016x}", fnv1a(key.as_bytes()));
    let mut kept = name;
    if overlong {
        let budget = MAX_NAME_BYTES - suffix.len();
        let mut cut = budget;
        while !kept.is_char_boundary(cut) {
            cut -= 1;
        }
        kept.truncate(cut);
    }
    if kept.is_empty() {
        kept.push_str("object");
    }
    Mapped {
        name: format!("{kept}{suffix}"),
        how: MappingOutcome::Suffixed,
    }
}

/// The longest name the scheme will produce, in bytes.
///
/// Filesystem limits are 255 bytes on every shipped target; 200 leaves room
/// for the working-file suffix the writer appends, with margin rather than
/// arithmetic that has to be revisited when that suffix changes.
const MAX_NAME_BYTES: usize = 200;

/// Windows refuses these as file *stems* whatever the extension — `CON` and
/// `con.txt` alike name the console device.
fn is_reserved_device(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("");
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL"
            | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9"
            | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    )
}

/// FNV-1a, 64-bit — inline because the suffix must be identical on every
/// platform and every release, which `DefaultHasher` explicitly does not
/// promise. Not cryptographic and not trying to be: the suffix
/// disambiguates, the encoding is what guarantees injectivity.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    //! `object-transfer` spec, "Keys map to filenames deterministically and
    //! losslessly in effect" — the mapping half. The on-disk half (existing
    //! files, case-insensitive volumes) lives with the writer, which is the
    //! only place that can see the destination.

    use super::*;

    /// The §4.4 shapes, one per line, with what the scheme must do about
    /// them. These are the documented examples; the loops below chase the
    /// space around them.
    #[test]
    fn the_shapes_the_brief_names() {
        // An ordinary key passes through untouched.
        let plain = local_name("reports/2026/summary.csv");
        assert_eq!(plain.name, "summary.csv");
        assert_eq!(plain.how, MappingOutcome::Unchanged);

        // Characters Windows refuses are percent-encoded, not replaced with
        // a lossy stand-in.
        let colon = local_name("logs/12:30:00.log");
        assert_eq!(colon.name, "12%3A30%3A00.log");
        assert_eq!(colon.how, MappingOutcome::Substituted);

        // The escape byte itself is escaped, or the scheme is not injective.
        let percent = local_name("a/100%3A.txt");
        assert_eq!(percent.name, "100%253A.txt");
        assert_eq!(percent.how, MappingOutcome::Substituted);

        // A key ending in the separator has no final segment to use.
        let marker = local_name("reports/daily/");
        assert!(!marker.name.is_empty());
        assert_eq!(marker.how, MappingOutcome::Suffixed);

        // Reserved DOS names, bare or with an extension, in any case.
        for reserved in ["CON", "con.txt", "Nul.log", "com3", "LPT1.csv"] {
            let mapped = local_name(&format!("dir/{reserved}"));
            assert_eq!(
                mapped.how,
                MappingOutcome::Suffixed,
                "{reserved} is refused by Windows and must not be used bare"
            );
        }

        // A name that is not reserved but merely contains one is left alone.
        let contains = local_name("dir/console.txt");
        assert_eq!(contains.name, "console.txt");
        assert_eq!(contains.how, MappingOutcome::Unchanged);

        // Windows strips trailing dots and spaces; a name that ends in one
        // would collide with its stripped twin, so the last byte is encoded.
        let dot = local_name("a/name.");
        assert_eq!(dot.name, "name%2E");
        let space = local_name("a/name ");
        assert_eq!(space.name, "name%20");

        // Control bytes never reach the filesystem.
        let control = local_name("a/be\u{7}ll.txt");
        assert_eq!(control.name, "be%07ll.txt");
    }

    /// Overlong names are cut to a bound that leaves room for the working
    /// suffix, and the cut lands on a character boundary.
    #[test]
    fn an_overlong_name_is_bounded_and_still_distinct() {
        let long_a = format!("dir/{}-alpha.bin", "x".repeat(400));
        let long_b = format!("dir/{}-omega.bin", "x".repeat(400));
        let a = local_name(&long_a);
        let b = local_name(&long_b);
        assert!(a.name.len() <= 200, "{} bytes", a.name.len());
        assert!(b.name.len() <= 200);
        assert_eq!(a.how, MappingOutcome::Suffixed);
        // The difference the truncation destroyed is carried by the suffix.
        assert_ne!(a.name, b.name);

        // Multibyte input still cuts on a boundary — this would panic or
        // produce invalid UTF-8 otherwise.
        let viet = format!("dir/{}.txt", "cái xô nhỏ ".repeat(40));
        let mapped = local_name(&viet);
        assert!(mapped.name.len() <= 200);
        assert!(mapped.name.is_char_boundary(mapped.name.len()));
    }

    /// Determinism: the same key maps to the same name, run after run —
    /// the suffix is derived from the key, not from time or randomness.
    #[test]
    fn the_mapping_is_deterministic() {
        for key in ["reports/daily/", "a/CON", "x/12:30.log", "plain.txt"] {
            assert_eq!(local_name(key), local_name(key), "key: {key}");
        }
    }

    /// Injectivity where it is promised: two keys with different final
    /// segments never silently share a name. (Two keys with the *same*
    /// segment — `a/x.txt` and `b/x.txt` — do share one, and that is the
    /// writer's existing-file question, reported there.)
    #[test]
    fn distinct_segments_never_silently_merge() {
        // A corpus dense in the characters the scheme touches. Hand-rolled
        // rather than proptest: the dependency is not in this crate, and the
        // space that matters is small enough to walk deliberately.
        let mut corpus: Vec<String> = Vec::new();
        let interesting = [
            "a", "A", "a.", "a ", "a?", "a*", "a:", "a<", "a>", "a\"", "a|",
            "a\\", "a%", "a%3A", "a%253A", "CON", "con", "CON.txt", "NUL",
            "com1", "com10", "a\u{1}", "a\u{7f}", ".", "..", "...", " ",
            "xô", "nhỏ", "a?b*c", "%", "%%", "%2F",
        ];
        for stem in interesting {
            corpus.push(stem.to_owned());
            corpus.push(format!("{stem}.txt"));
            corpus.push(format!("pre/{stem}"));
        }

        let mut seen: std::collections::HashMap<String, (String, MappingOutcome)> =
            std::collections::HashMap::new();
        for key in &corpus {
            let segment = key.rsplit('/').next().unwrap_or("").to_owned();
            let mapped = local_name(key);
            if let Some((other_segment, other_how)) =
                seen.insert(mapped.name.clone(), (segment.clone(), mapped.how))
            {
                // Same name from two keys is only tolerable when the final
                // segments were identical, or at least one mapping said so.
                assert!(
                    other_segment == segment
                        || mapped.how != MappingOutcome::Unchanged
                        || other_how != MappingOutcome::Unchanged,
                    "`{other_segment}` and `{segment}` merged silently into `{}`",
                    mapped.name
                );
            }
        }
    }

    /// What the writer relies on without being able to say so in types:
    /// the name is a bare filename, never a path, never empty.
    #[test]
    fn the_name_is_always_a_bare_filename() {
        for key in [
            "a/b/c.txt", "trailing/", "//", "/", "", "..", "a/..", "nul",
            "weird/\\backslash", "per%cent/",
        ] {
            let mapped = local_name(key);
            assert!(!mapped.name.is_empty(), "key {key:?} produced empty");
            assert!(
                !mapped.name.contains('/') && !mapped.name.contains('\\'),
                "key {key:?} produced a path: {}",
                mapped.name
            );
            assert_ne!(mapped.name, ".", "key {key:?}");
            assert_ne!(mapped.name, "..", "key {key:?}");
        }
    }
}
