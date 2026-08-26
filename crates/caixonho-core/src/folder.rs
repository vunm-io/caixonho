//! Making a folder (`XONHO-0024`).
//!
//! S3 has no folders. A general purpose bucket fakes one with a zero-byte
//! object whose key ends in `/`, which is what the AWS console writes and what
//! `ListObjectsV2` with `delimiter=/` returns as a common prefix — so the
//! listing this application already draws renders it as a folder with no
//! further help.
//!
//! A directory bucket does not fake it. It has real directories, and it
//! removes them the moment they empty:
//!
//! > Directories are created during `PutObject` or `CreateMultiPartUpload`
//! > operations and automatically removed when they become empty after
//! > `DeleteObject` or `AbortMultiPartUpload` operations.
//!
//! So an empty folder cannot survive there, and this module refuses rather
//! than writing something that will be gone before the next listing. That is
//! not a corner case for whoever reads this next: the account this was written
//! for is entirely directory buckets.

use crate::types::Prefix;

/// Why a name cannot become a folder.
///
/// Each variant is a sentence a person can act on, which is the whole reason
/// they are separate: "that name will not work" tells someone to guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadFolderName {
    /// Nothing, or nothing but whitespace.
    Empty,
    /// Contains the one character that means "and then a folder inside that".
    /// One level at a time, so the listing can show what was made.
    HasSeparator,
}

impl std::fmt::Display for BadFolderName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("a folder needs a name"),
            Self::HasSeparator => {
                f.write_str("a folder's name cannot contain `/` — make one level at a time")
            }
        }
    }
}

/// The key a folder called `name` would have inside `prefix`.
///
/// Trimmed, because a name typed with a stray space either side is the name the
/// person meant and a key with a space either side is not — and the difference
/// would be invisible in every listing that showed it.
///
/// The trailing `/` is what makes it a folder rather than an empty file, and it
/// is added here rather than by the caller so there is exactly one place that
/// decides what a folder key looks like.
pub fn key_for(prefix: &Prefix, name: &str) -> Result<String, BadFolderName> {
    let name = name.trim();
    if name.is_empty() {
        return Err(BadFolderName::Empty);
    }
    if name.contains('/') {
        return Err(BadFolderName::HasSeparator);
    }
    Ok(format!("{}{name}/", prefix.as_str()))
}

#[cfg(test)]
mod tests {
    //! `bucket-listing` spec: "A folder can be made where the user is
    //! standing" and "An empty folder is not offered where it cannot exist".

    use super::*;

    #[test]
    fn a_folder_at_the_root_of_a_bucket_has_no_leading_separator() {
        assert_eq!(
            key_for(&Prefix::root(), "reports"),
            Ok("reports/".to_owned())
        );
    }

    #[test]
    fn a_folder_inside_a_prefix_is_made_inside_it() {
        let inside = Prefix::parse("2026/");

        assert_eq!(key_for(&inside, "august"), Ok("2026/august/".to_owned()));
    }

    #[test]
    fn a_name_that_is_nothing_is_refused() {
        assert_eq!(key_for(&Prefix::root(), ""), Err(BadFolderName::Empty));
    }

    #[test]
    fn a_name_that_is_only_whitespace_is_nothing() {
        // Not a separate rule — the trim is what makes it the same rule, and
        // it is what stops a folder whose name is a space.
        assert_eq!(key_for(&Prefix::root(), "   "), Err(BadFolderName::Empty));
    }

    #[test]
    fn a_name_carrying_a_separator_is_refused() {
        assert_eq!(
            key_for(&Prefix::root(), "reports/august"),
            Err(BadFolderName::HasSeparator)
        );
    }

    #[test]
    fn a_name_typed_with_stray_spaces_is_the_name_that_was_meant() {
        assert_eq!(
            key_for(&Prefix::root(), "  reports  "),
            Ok("reports/".to_owned()),
            "a key with a space either side is invisible in every listing that shows it"
        );
    }
}
