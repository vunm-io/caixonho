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

/// Why a typed destination cannot name an object (`XONHO-0026`).
///
/// Beside the folder rules rather than in a module of its own: these are the
/// same question about a different shape, and two homes would drift on the
/// rules they share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadObjectKey {
    /// Nothing, or nothing but whitespace.
    Empty,
    /// Ends in `/`, so it names a folder. This operation writes an object.
    NamesAFolder,
    /// Begins with `/`. S3 permits it, and it makes an object inside a folder
    /// whose name is the empty string — legal, never intended, and rendered
    /// strangely by every tool that meets it.
    LeadingSeparator,
}

impl std::fmt::Display for BadObjectKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("a destination is needed"),
            Self::NamesAFolder => {
                f.write_str("a destination ending in `/` names a folder — give the file a name too")
            }
            Self::LeadingSeparator => {
                f.write_str("a destination cannot start with `/` — it is relative to the bucket")
            }
        }
    }
}

/// Whether `key` may name the object an upload writes.
///
/// Returns the key itself on success, trimmed, so callers cannot accidentally
/// send the untrimmed one — the same shape as [`key_for`], for the same
/// reason.
///
/// A `/` in the *middle* is fine and is the whole point: it is how a folder
/// comes into being on a directory bucket, where the service creates the
/// directories as part of the write.
pub fn object_key(key: &str) -> Result<String, BadObjectKey> {
    let key = key.trim();
    if key.is_empty() {
        return Err(BadObjectKey::Empty);
    }
    if key.ends_with('/') {
        return Err(BadObjectKey::NamesAFolder);
    }
    if key.starts_with('/') {
        return Err(BadObjectKey::LeadingSeparator);
    }
    Ok(key.to_owned())
}

/// Whether `folder` may be the prefix several files share (`XONHO-0029`).
///
/// Returns it with exactly one trailing `/`, so a caller cannot send the
/// un-normalised one — the same shape [`object_key`] uses and for the same
/// reason.
///
/// The empty string is **allowed** here and refused there, and that is the
/// difference between the two questions: an object must be named, and "no
/// folder" is a real answer meaning the bucket's root.
pub fn folder_prefix(folder: &str) -> Result<String, BadObjectKey> {
    let folder = folder.trim();
    if folder.is_empty() {
        return Ok(String::new());
    }
    if folder.starts_with('/') {
        return Err(BadObjectKey::LeadingSeparator);
    }
    Ok(format!("{}/", folder.trim_end_matches('/')))
}

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
    fn a_folder_for_several_files_gets_exactly_one_separator() {
        assert_eq!(folder_prefix("uploads"), Ok("uploads/".to_owned()));
        assert_eq!(folder_prefix("uploads/"), Ok("uploads/".to_owned()));
        assert_eq!(folder_prefix("uploads///"), Ok("uploads/".to_owned()));
    }

    #[test]
    fn no_folder_means_the_root_and_is_a_real_answer() {
        // Where `object_key` refuses the empty string, this accepts it: an
        // object must be named, a folder need not be.
        assert_eq!(folder_prefix("   "), Ok(String::new()));
    }

    #[test]
    fn a_folder_starting_at_the_root_is_refused_like_a_key() {
        assert_eq!(
            folder_prefix("/uploads"),
            Err(BadObjectKey::LeadingSeparator)
        );
    }

    #[test]
    fn a_plain_name_is_a_destination() {
        assert_eq!(object_key("report.csv"), Ok("report.csv".to_owned()));
    }

    #[test]
    fn a_destination_may_carry_a_path_and_that_is_the_point() {
        // On a directory bucket this is the *only* way to make a folder: the
        // service creates the directories as part of the write.
        assert_eq!(
            object_key("uploads/2026/report.csv"),
            Ok("uploads/2026/report.csv".to_owned())
        );
    }

    #[test]
    fn a_destination_that_is_nothing_is_refused() {
        assert_eq!(object_key("   "), Err(BadObjectKey::Empty));
    }

    #[test]
    fn a_destination_ending_in_a_separator_names_a_folder_not_an_object() {
        assert_eq!(object_key("uploads/"), Err(BadObjectKey::NamesAFolder));
    }

    #[test]
    fn a_destination_starting_at_the_root_is_refused_rather_than_trimmed() {
        // Trimming would send a key the user did not type, and the spec says
        // what is shown is what is sent.
        assert_eq!(
            object_key("/uploads/report.csv"),
            Err(BadObjectKey::LeadingSeparator)
        );
    }

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
