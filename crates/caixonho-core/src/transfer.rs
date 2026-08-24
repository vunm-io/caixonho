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
        let refused = matches!(
            ch,
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '%'
        ) || ch.is_control()
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
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
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

/// The extension a download-in-progress wears.
///
/// Unmistakably this application's, so a crash's leavings can be recognised
/// — and `local_name` keeps final names short enough that appending this
/// never breaks the filesystem's limit (its bound accounts for it).
const WORKING_EXTENSION: &str = "caixonho-partial";

/// Writes one object to disk without ever letting a partial file sit at the
/// final path (`object-transfer` spec, "A partial download is never
/// mistakable for the file").
///
/// Content goes to `<final>.caixonho-partial` in the same directory — same
/// directory, so the promotion is a same-volume rename and never a copy —
/// and reaches the final name only through [`Writer::promote`]. Every other
/// way out of this type, `Drop` included, removes the working file: cancel,
/// failure and panic share one cleanup, and the happy path disarms it.
///
/// Writes are ordinary blocking `std::fs` writes. The chunks arrive from an
/// async pull, but each write is a small append; putting an async file
/// runtime between the two would buy nothing this application can observe.
#[derive(Debug)]
pub struct Writer {
    file: Option<std::fs::File>,
    working: std::path::PathBuf,
    final_path: std::path::PathBuf,
}

impl Writer {
    /// Open a writer for `final_path`.
    ///
    /// A stale working file from a crashed run is truncated, never resumed:
    /// nothing certifies its bytes, and the spec's promise is about what the
    /// *final* path holds, which resuming unknown content would break the
    /// slow way.
    pub fn begin(final_path: &std::path::Path) -> crate::error::Result<Self> {
        let mut name = final_path
            .file_name()
            .map(std::ffi::OsString::from)
            .unwrap_or_default();
        name.push(".");
        name.push(WORKING_EXTENSION);
        let working = final_path.with_file_name(name);

        let file = std::fs::File::create(&working).map_err(destination)?;
        Ok(Self {
            file: Some(file),
            working,
            final_path: final_path.to_owned(),
        })
    }

    /// Append one chunk to the working file.
    pub fn write(&mut self, chunk: &[u8]) -> crate::error::Result<()> {
        use std::io::Write as _;
        self.file
            .as_mut()
            .expect("a writer holds its file until promoted")
            .write_all(chunk)
            .map_err(destination)
    }

    /// The content is complete: put it at the final path.
    ///
    /// Flushed to the platform's satisfaction before the rename, so the file
    /// that appears under the real name is the file, not a page cache's
    /// intention of one.
    pub fn promote(mut self) -> crate::error::Result<()> {
        let file = self.file.take().expect("promote consumes the writer");
        file.sync_all().map_err(destination)?;
        drop(file);
        std::fs::rename(&self.working, &self.final_path).map_err(destination)
        // `self` drops here with `file: None`, which is what disarms the
        // cleanup below — the working file has just become the real one.
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        // Armed exactly while the working file is ours: `promote` takes the
        // file out first, and after a rename there is nothing to remove.
        if self.file.take().is_some() {
            let _ = std::fs::remove_file(&self.working);
        }
    }
}

/// An `io::Error` as the destination's refusal — without the path, which the
/// diagnostics spec keeps out of anything that can reach the log.
fn destination(error: std::io::Error) -> crate::error::Error {
    crate::error::Error::Destination {
        detail: error.to_string(),
    }
}

/// What to do when the final name is already taken at the destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collision {
    /// Return the question to whoever asked; transfer nothing.
    Ask,
    /// Write over the existing file — only ever on the user's say-so.
    Replace,
    /// Keep both: derive a free name beside the existing one.
    KeepBoth,
}

/// How one download ended.
#[derive(Debug)]
pub enum DownloadOutcome {
    /// The file is at the destination under `name`.
    Finished {
        /// The final filename — differs from the key's segment when the
        /// mapping or keep-both had to step in, and the UI says so.
        name: String,
        /// What the mapping did to produce it.
        mapped: MappingOutcome,
        /// Bytes written.
        bytes: u64,
    },
    /// The final name is taken and the caller asked to be asked.
    NameTaken {
        /// The name that is taken.
        name: String,
    },
    /// Cancelled by the user; nothing is at the destination.
    Cancelled,
    /// Failed; nothing is at the destination. Carries the classified cause.
    Failed(crate::error::Error),
}

/// Cancels a download in flight.
///
/// Cooperative, checked between chunks — so the task itself sees the
/// cancellation, cleans up through the writer's one path, logs the outcome
/// and delivers it. Aborting the task instead would leave nobody to say so.
#[derive(Debug, Clone, Default)]
pub struct Cancel(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Cancel {
    /// Ask the download to stop.
    pub fn cancel(&self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Where a download will write, or the question that stops it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Resolved {
    /// Write here.
    Write {
        name: String,
        mapped: MappingOutcome,
    },
    /// The name is taken and the caller wanted to be asked.
    Taken { name: String },
}

/// Decide the filename for `key` in `directory` under `collision`.
///
/// The policy only matters once the mapped name is taken. Keep-both takes
/// the first free ` (n)` name, counting from 2 — the numbering people already
/// know from every file manager — inserted before the final extension so
/// `report (2).csv` still opens as a CSV.
pub(crate) fn resolve_destination(
    directory: &std::path::Path,
    key: &str,
    collision: Collision,
) -> Resolved {
    let mapped = local_name(key);
    if !directory.join(&mapped.name).exists() {
        return Resolved::Write {
            name: mapped.name,
            mapped: mapped.how,
        };
    }
    match collision {
        Collision::Ask => Resolved::Taken { name: mapped.name },
        Collision::Replace => Resolved::Write {
            name: mapped.name,
            mapped: mapped.how,
        },
        Collision::KeepBoth => {
            let (stem, extension) = match mapped.name.rsplit_once('.') {
                // A leading dot is a hidden file's, not an extension's.
                Some((stem, extension)) if !stem.is_empty() => {
                    (stem.to_owned(), format!(".{extension}"))
                }
                _ => (mapped.name.clone(), String::new()),
            };
            let mut n: u32 = 2;
            let name = loop {
                let candidate = format!("{stem} ({n}){extension}");
                if !directory.join(&candidate).exists() {
                    break candidate;
                }
                n += 1;
            };
            Resolved::Write {
                name,
                mapped: mapped.how,
            }
        }
    }
}

/// Pump `content` into a [`Writer`] at `final_path`, counting as it goes.
///
/// Cancellation is checked between chunks — after each write and before the
/// next pull — so a cancel lands within one chunk's worth of bytes, and the
/// task that was cancelled is still alive to clean up, log and deliver.
/// Every early way out drops the writer unpromoted, which is the cleanup.
pub(crate) async fn pump(
    mut content: crate::store::ObjectContent,
    final_path: &std::path::Path,
    cancel: &Cancel,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> std::result::Result<u64, PumpEnd> {
    let mut writer = Writer::begin(final_path).map_err(PumpEnd::Failed)?;
    let total = content.size;
    let mut bytes: u64 = 0;

    loop {
        if cancel.is_cancelled() {
            return Err(PumpEnd::Cancelled);
        }
        let chunk = match content.body.next_chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(cause) => return Err(PumpEnd::Failed(cause)),
        };
        writer.write(&chunk).map_err(PumpEnd::Failed)?;
        bytes += chunk.len() as u64;
        on_progress(bytes, total);
    }

    writer.promote().map_err(PumpEnd::Failed)?;
    Ok(bytes)
}

/// How a pump ends when it does not finish.
#[derive(Debug)]
pub(crate) enum PumpEnd {
    Cancelled,
    Failed(crate::error::Error),
}

/// How long an opened file may sit in the cache before the sweep takes it.
///
/// A week: long enough that "open it again" over a few days costs nothing,
/// short enough that the cache stays a cache. Not a setting — the spec's
/// promise is "bounded, not the user's job", and a knob would make it the
/// user's job with extra steps.
const OPEN_CACHE_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

/// Where download-to-open files live on this machine.
///
/// The cache location, not the state or data one: everything in it can be
/// re-downloaded, which is what a cache directory means to the platform's
/// own cleanup tooling. `directories` resolves the base for the same reason
/// the log directory lets it (`diagnostics::log_directory`): that crate is
/// tested on all three platforms and this repository is not.
///
/// - macOS: `~/Library/Caches/caixonho/open`
/// - Windows: `%LOCALAPPDATA%\caixonho\cache\open`
/// - Linux: `$XDG_CACHE_HOME/caixonho/open`
pub fn open_cache_dir() -> Option<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "caixonho")?;
    Some(dirs.cache_dir().join("open"))
}

/// Remove cache entries older than [`OPEN_CACHE_MAX_AGE`], judged against
/// `now`.
///
/// Called once at startup — the same sweep-on-use pattern the log's own
/// rotation follows; no daemon, no timer. `now` is a parameter so a test can
/// hold the clock instead of waiting a week. A directory that does not exist
/// is a cache with nothing in it; errors on individual entries are skipped
/// rather than fatal, because a file the sweep cannot remove today is one it
/// will remove tomorrow, and startup must not fail over housekeeping.
pub fn sweep_open_cache(directory: &std::path::Path, now: std::time::SystemTime) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        let expired = now
            .duration_since(modified)
            .map(|age| age > OPEN_CACHE_MAX_AGE)
            .unwrap_or(false);
        if expired {
            let _ = std::fs::remove_file(entry.path());
        }
    }
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
            "a", "A", "a.", "a ", "a?", "a*", "a:", "a<", "a>", "a\"", "a|", "a\\", "a%", "a%3A",
            "a%253A", "CON", "con", "CON.txt", "NUL", "com1", "com10", "a\u{1}", "a\u{7f}", ".",
            "..", "...", " ", "xô", "nhỏ", "a?b*c", "%", "%%", "%2F",
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
            "a/b/c.txt",
            "trailing/",
            "//",
            "/",
            "",
            "..",
            "a/..",
            "nul",
            "weird/\\backslash",
            "per%cent/",
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

    // ---- The writer (task 3.1) ----

    /// Each test gets its own directory, cleaned up going in rather than out,
    /// so a failed run leaves evidence — the same arrangement the session and
    /// diagnostics fixtures use.
    fn a_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("caixonho-transfer-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the fixture directory is creatable");
        dir
    }

    #[test]
    fn completion_promotes_and_leaves_no_working_file() {
        let dir = a_dir("promotes");
        let final_path = dir.join("report.csv");

        let mut writer = Writer::begin(&final_path).expect("the directory is writable");
        writer.write(b"a,b\n").expect("writes");
        writer.write(b"1,2\n").expect("writes");
        assert!(
            !final_path.exists(),
            "nothing may sit at the final path before promotion"
        );
        writer.promote().expect("promotes");

        assert_eq!(
            std::fs::read(&final_path).expect("the file exists"),
            b"a,b\n1,2\n"
        );
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            1,
            "the working file is gone; only the real one remains"
        );
    }

    #[test]
    fn a_dropped_writer_leaves_neither_working_nor_final_file() {
        let dir = a_dir("dropped");
        let final_path = dir.join("big.bin");

        let mut writer = Writer::begin(&final_path).expect("begins");
        writer
            .write(b"some bytes that will not survive")
            .expect("writes");
        drop(writer);

        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "cancel and failure share the drop path, and it cleans up"
        );
    }

    #[test]
    fn a_failed_transfer_leaves_an_existing_final_file_untouched() {
        let dir = a_dir("existing");
        let final_path = dir.join("kept.txt");
        std::fs::write(&final_path, b"the original").expect("fixture");

        let mut writer = Writer::begin(&final_path).expect("begins");
        writer.write(b"half of a replace").expect("writes");
        drop(writer);

        assert_eq!(
            std::fs::read(&final_path).expect("still there"),
            b"the original",
            "a transfer that did not finish may not have touched the real file"
        );
    }

    #[test]
    fn a_stale_working_file_is_replaced_not_resumed() {
        let dir = a_dir("stale");
        let final_path = dir.join("report.csv");
        std::fs::write(
            dir.join("report.csv.caixonho-partial"),
            b"left behind by a crash",
        )
        .expect("fixture");

        let mut writer = Writer::begin(&final_path).expect("begins over the stale file");
        writer.write(b"fresh").expect("writes");
        writer.promote().expect("promotes");

        assert_eq!(
            std::fs::read(&final_path).expect("exists"),
            b"fresh",
            "yesterday's crash may not prepend itself to today's download"
        );
    }

    #[test]
    fn an_unwritable_destination_is_a_destination_error_without_the_path() {
        let dir = a_dir("unwritable");
        let missing = dir.join("no-such-dir").join("file.txt");

        let outcome = Writer::begin(&missing);
        match outcome {
            Err(crate::error::Error::Destination { detail }) => {
                assert!(
                    !detail.contains("no-such-dir"),
                    "the cause reaches the log; the path may not: {detail}"
                );
            }
            other => panic!("expected Destination, got {other:?}"),
        }
    }

    // ---- The transfer itself (task 3.2) ----

    fn content_of(double: crate::store::double::StoreDouble) -> crate::store::ObjectContent {
        futures_of(async move {
            use crate::store::ObjectStore as _;
            double.get_object("bucket", "key").await.expect("scripted")
        })
    }

    /// One current-thread runtime per test, the arrangement every async test
    /// in this crate already uses.
    fn futures_of<T>(fut: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(fut)
    }

    #[test]
    fn a_download_finishes_counting_every_byte() {
        let dir = a_dir("finishes");
        let final_path = dir.join("summary.csv");
        let content = content_of(crate::store::double::StoreDouble::serving_chunks(vec![
            b"one ".to_vec(),
            b"two ".to_vec(),
            b"three".to_vec(),
        ]));

        let mut seen: Vec<(u64, Option<u64>)> = Vec::new();
        let bytes = futures_of(pump(content, &final_path, &Cancel::default(), |b, t| {
            seen.push((b, t));
        }))
        .expect("finishes");

        assert_eq!(bytes, 13);
        assert_eq!(std::fs::read(&final_path).unwrap(), b"one two three");
        assert_eq!(
            seen,
            vec![(4, Some(13)), (8, Some(13)), (13, Some(13))],
            "progress is cumulative, against the stated size"
        );
    }

    #[test]
    fn a_cancelled_download_leaves_nothing_at_the_destination() {
        let dir = a_dir("cancelled");
        let final_path = dir.join("big.bin");
        let content = content_of(crate::store::double::StoreDouble::serving_chunks(vec![
            b"first".to_vec(),
            b"second".to_vec(),
            b"third".to_vec(),
        ]));

        let cancel = Cancel::default();
        let seen = std::cell::Cell::new(0u32);
        let outcome = futures_of(pump(content, &final_path, &cancel, |_, _| {
            seen.set(seen.get() + 1);
            cancel.cancel(); // the user clicks after the first chunk
        }));

        assert!(matches!(outcome, Err(PumpEnd::Cancelled)), "{outcome:?}");
        assert_eq!(seen.get(), 1, "cancellation lands between chunks");
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "neither the file nor its working twin survives a cancel"
        );
    }

    #[test]
    fn a_mid_stream_failure_cleans_up_and_keeps_its_cause() {
        let dir = a_dir("break");
        let final_path = dir.join("cut.bin");
        let content = content_of(crate::store::double::StoreDouble::content_breaking_after(
            vec![b"the only chunk".to_vec()],
        ));

        let outcome = futures_of(pump(content, &final_path, &Cancel::default(), |_, _| {}));

        match outcome {
            Err(PumpEnd::Failed(crate::error::Error::Network { .. })) => {}
            other => panic!("expected the network cause to survive, got {other:?}"),
        }
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    }

    #[test]
    fn a_taken_name_is_a_question_when_asking_was_asked_for() {
        let dir = a_dir("taken");
        std::fs::write(dir.join("report.csv"), b"already here").unwrap();

        let resolved = resolve_destination(&dir, "monthly/report.csv", Collision::Ask);
        assert_eq!(
            resolved,
            Resolved::Taken {
                name: "report.csv".to_owned()
            }
        );
    }

    #[test]
    fn replace_writes_and_keep_both_steps_aside() {
        let dir = a_dir("policies");
        std::fs::write(dir.join("report.csv"), b"v1").unwrap();
        std::fs::write(dir.join("report (2).csv"), b"v2").unwrap();

        assert_eq!(
            resolve_destination(&dir, "monthly/report.csv", Collision::Replace),
            Resolved::Write {
                name: "report.csv".to_owned(),
                mapped: MappingOutcome::Unchanged
            },
            "replace answers with the taken name itself"
        );
        assert_eq!(
            resolve_destination(&dir, "monthly/report.csv", Collision::KeepBoth),
            Resolved::Write {
                name: "report (3).csv".to_owned(),
                mapped: MappingOutcome::Unchanged
            },
            "keep-both takes the first free numbered name"
        );
    }

    #[test]
    fn a_free_name_is_written_whatever_the_policy_says() {
        let dir = a_dir("free");
        for collision in [Collision::Ask, Collision::Replace, Collision::KeepBoth] {
            assert_eq!(
                resolve_destination(&dir, "logs/12:30.log", collision),
                Resolved::Write {
                    name: "12%3A30.log".to_owned(),
                    mapped: MappingOutcome::Substituted
                },
                "the policy only matters once a name is taken"
            );
        }
    }

    // ---- The open-cache sweep (task 3.3) ----

    #[test]
    fn the_sweep_takes_the_old_and_leaves_the_young() {
        let dir = a_dir("sweep");
        std::fs::write(dir.join("opened-today.pdf"), b"x").unwrap();
        std::fs::write(dir.join("opened-last-month.pdf"), b"y").unwrap();
        let now = std::time::SystemTime::now();

        // Judged now, both files are fresh: nothing goes.
        sweep_open_cache(&dir, now);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);

        // Judged from eight days in the future, both are past the age — the
        // clock is injected precisely so the test does not have to set
        // mtimes, which std cannot do without another dependency.
        sweep_open_cache(&dir, now + std::time::Duration::from_secs(8 * 24 * 60 * 60));
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "a cache entry past its age is reclaimed"
        );
    }

    #[test]
    fn sweeping_a_missing_directory_is_a_cache_with_nothing_in_it() {
        let ghost = std::env::temp_dir().join("caixonho-transfer-no-such-cache");
        let _ = std::fs::remove_dir_all(&ghost);
        // The assertion is that this returns instead of panicking or
        // creating anything.
        sweep_open_cache(&ghost, std::time::SystemTime::now());
        assert!(!ghost.exists());
    }
}
