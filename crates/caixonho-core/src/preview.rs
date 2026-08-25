//! Answering *what is this?* without a download (`XONHO-0008`).
//!
//! Two pure functions: the extension chooses the path, the content gets the
//! last word. Both live here because they are decisions, not rendering —
//! and because their edges (a UTF-8 character split by a ranged cut, a BOM,
//! a `.log` full of NULs) are exactly where a preview turns into noise.

/// Which preview path an object's name selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    /// Text-like: previewed by its first page, over a ranged read.
    Text,
    /// A raster image: previewed whole, under the size gate.
    Image(RasterKind),
    /// Neither: the preview refuses honestly and offers Open.
    None,
}

/// The raster formats the preview draws.
///
/// Core's own enum, not `gpui::ImageFormat`: this crate names no UI type
/// (crate invariant), and the GUI maps this at its edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasterKind {
    Png,
    Jpeg,
    Gif,
    Webp,
    Bmp,
}

/// What the fetched bytes turned out to be.
#[derive(Debug, PartialEq, Eq)]
pub enum TextVerdict {
    /// Decodable text, ready to show.
    Text(String),
    /// Not text, whatever the name said. The preview says *binary* instead
    /// of rendering noise.
    Binary,
}

/// The path an object's key selects, by its final segment's extension.
///
/// The dot searched is the **last segment's** — the rule `local_name` and
/// `beside` already follow, so a prefix like `v1.png/` can never choose the
/// path for what sits inside it.
pub fn kind_of(key: &str) -> PreviewKind {
    let segment = key.rsplit('/').next().unwrap_or("");
    let Some((_, extension)) = segment.rsplit_once('.') else {
        return PreviewKind::None;
    };
    match extension.to_ascii_lowercase().as_str() {
        "txt" | "log" | "json" | "yaml" | "yml" | "csv" | "md" | "toml" | "xml" => {
            PreviewKind::Text
        }
        "png" => PreviewKind::Image(RasterKind::Png),
        "jpg" | "jpeg" => PreviewKind::Image(RasterKind::Jpeg),
        "gif" => PreviewKind::Image(RasterKind::Gif),
        "webp" => PreviewKind::Image(RasterKind::Webp),
        "bmp" => PreviewKind::Image(RasterKind::Bmp),
        _ => PreviewKind::None,
    }
}

/// The truth about fetched bytes. `truncated` says a ranged cut may have
/// split the final character.
///
/// Strict on purpose: a NUL anywhere is binary, an invalid sequence is
/// binary, and the single tolerance — an incomplete character at the very
/// end — exists only because a ranged cut lands on a byte, not a character
/// boundary, and that accident must not condemn the file. `from_utf8`
/// distinguishes the two shapes for us: `error_len() == None` is "the input
/// ended mid-character", anything else is "these bytes are wrong".
pub fn text_of(bytes: &[u8], truncated: bool) -> TextVerdict {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    if bytes.contains(&0) {
        return TextVerdict::Binary;
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => TextVerdict::Text(text.to_owned()),
        Err(error) if truncated && error.error_len().is_none() => {
            let valid = &bytes[..error.valid_up_to()];
            match std::str::from_utf8(valid) {
                Ok(text) => TextVerdict::Text(text.to_owned()),
                Err(_) => TextVerdict::Binary,
            }
        }
        Err(_) => TextVerdict::Binary,
    }
}

#[cfg(test)]
mod tests {
    //! `object-preview` spec, "A text-like object previews by its first
    //! page" — the name-said-text-bytes-said-otherwise half — and the kind
    //! selection both other requirements route through.

    use super::*;

    #[test]
    fn the_extension_chooses_the_path_case_insensitively() {
        for key in [
            "a/b/notes.txt",
            "X.LOG",
            "data.json",
            "y.yaml",
            "z.yml",
            "t.csv",
            "README.md",
            "conf.toml",
            "feed.xml",
        ] {
            assert_eq!(kind_of(key), PreviewKind::Text, "{key}");
        }
        assert_eq!(kind_of("photo.PNG"), PreviewKind::Image(RasterKind::Png));
        assert_eq!(kind_of("a/pic.jpeg"), PreviewKind::Image(RasterKind::Jpeg));
        assert_eq!(kind_of("pic.jpg"), PreviewKind::Image(RasterKind::Jpeg));
        assert_eq!(kind_of("anim.gif"), PreviewKind::Image(RasterKind::Gif));
        assert_eq!(kind_of("w.webp"), PreviewKind::Image(RasterKind::Webp));
        assert_eq!(kind_of("old.bmp"), PreviewKind::Image(RasterKind::Bmp));
        for key in ["archive.zip", "report.pdf", "binary", "video.mp4"] {
            assert_eq!(kind_of(key), PreviewKind::None, "{key}");
        }
    }

    /// The dot searched is the last segment's — the same rule `beside` and
    /// `local_name` already follow, asserted so the three never drift.
    #[test]
    fn a_dot_in_a_prefix_does_not_choose_the_path() {
        assert_eq!(kind_of("v1.2/binary"), PreviewKind::None);
        assert_eq!(kind_of("v1.png/binary"), PreviewKind::None);
        assert_eq!(kind_of("v1.2/notes.txt"), PreviewKind::Text);
    }

    #[test]
    fn ordinary_utf8_is_text_and_a_bom_is_stripped() {
        assert_eq!(
            text_of("cái xô nhỏ".as_bytes(), false),
            TextVerdict::Text("cái xô nhỏ".to_owned())
        );
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice(b"hello");
        assert_eq!(text_of(&bom, false), TextVerdict::Text("hello".to_owned()));
        assert_eq!(text_of(b"", false), TextVerdict::Text(String::new()));
    }

    #[test]
    fn a_nul_anywhere_is_binary_whatever_the_name_said() {
        assert_eq!(text_of(b"looks\x00fine", false), TextVerdict::Binary);
        assert_eq!(text_of(b"\x00", true), TextVerdict::Binary);
    }

    /// A ranged cut may split a multibyte character. Exactly one truncated
    /// tail character is tolerated — and only when the caller says the cut
    /// happened.
    #[test]
    fn a_ranged_cut_through_a_character_does_not_condemn_the_file() {
        let text = "xô".as_bytes(); // 'ô' is two bytes
        let cut = &text[..text.len() - 1]; // split inside 'ô'
        assert_eq!(
            text_of(cut, true),
            TextVerdict::Text("x".to_owned()),
            "the split character is dropped, the rest survives"
        );
        // The same bytes without the truncation excuse are just invalid.
        assert_eq!(text_of(cut, false), TextVerdict::Binary);
        // And an invalid sequence in the *middle* is binary even when
        // truncated: the excuse covers the tail alone.
        let mut broken = b"ab".to_vec();
        broken.push(0xC3); // opens a 2-byte char
        broken.extend_from_slice(b"cd"); // ...but continues with ASCII
        assert_eq!(text_of(&broken, true), TextVerdict::Binary);
    }
}
