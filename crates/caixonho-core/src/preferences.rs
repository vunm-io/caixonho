//! What the user has chosen to *see*, remembered per connection
//! (`XONHO-0027`).
//!
//! Kept apart from [`crate::connections`] deliberately, and the reason is not
//! tidiness. That file holds the non-secret half of a **credential**; this one
//! holds a display choice. Putting a choice into that file would give a
//! cosmetic act the blast radius of a credential edit — and it would not work
//! anyway, because half the connections have no entry there to add a field to:
//! a profile is *discovered* in `~/.aws` and this application writes nothing
//! about it.
//!
//! So preferences are keyed by connection **name**, which is what the user
//! picked in the list and what both kinds of connection have.
//!
//! Nothing here may ever fail a listing. A file that is missing, unreadable or
//! malformed means "no choice recorded", which is the behaviour that existed
//! before this module did.

use std::collections::BTreeMap;
use std::path::PathBuf;

use directories::ProjectDirs;

use crate::error::Result;

/// What the file is called, beside `connections.toml` rather than inside it.
const FILE_NAME: &str = "view-preferences.toml";

const HEADER: &str = "\
# caixonho view preferences.
#
# Which buckets each connection shows. Display only: deleting this file loses
# no credential and no data, and every connection goes back to showing
# everything — which is also what a line this file cannot read means.
#
# `buckets` is comma-separated. Bucket names cannot contain a comma, so
# nothing here needs escaping beyond the quoting the value already has.
";

/// Where the preferences live, as a port a test can stand in for.
///
/// The same shape as [`crate::connections::ConnectionFile`]: the port moves
/// text and knows nothing about what is in it, so the parsing is testable
/// without a filesystem and the file is testable without a parser.
///
/// Smaller than `ConnectionFile` by one method, and the missing one says what
/// this file is: that trait has a `location` because a connections failure has
/// to name the file it could not read. Nothing here ever surfaces a failure,
/// so no message needs the path, so the method would be API kept for a caller
/// that cannot exist.
pub(crate) trait PreferencesFile: std::fmt::Debug + Send + Sync {
    /// What the file holds. `Ok(None)` is an answer, not a failure: no file
    /// yet is what every first run looks like.
    fn read(&self) -> Result<Option<String>>;

    /// Replace the file's contents.
    fn write(&self, contents: &str) -> Result<()>;
}

/// The whole file: the buckets each connection that has chosen shows.
///
/// A `BTreeMap` so the file is written in a stable order — one that reshuffles
/// itself on every save is one nobody can diff.
///
/// A connection **absent** from the map has made no choice and shows
/// everything. A connection present with an **empty** list has chosen nothing,
/// which is a different statement, and keeping the two apart is the whole
/// reason this is a map of lists rather than a flat list of names.
type Preferences = BTreeMap<String, Vec<String>>;

/// Hand-written, like `connections.rs`'s own encoder, and for the same reason:
/// `caixonho-core` carries neither `serde` nor a TOML crate, and adding two
/// dependency trees so a display preference can be a derive would be a
/// supply-chain cost paid for a convenience (`XONHO-0017`).
fn encode(preferences: &Preferences) -> String {
    let mut out = String::from(HEADER);
    for (connection, buckets) in preferences {
        out.push_str("\n[[shows]]\n");
        out.push_str("connection = ");
        out.push_str(&quote(connection));
        out.push_str("\nbuckets = ");
        out.push_str(&quote(&buckets.join(",")));
        out.push('\n');
    }
    out
}

/// Read the file's text back.
///
/// Anything unrecognised is skipped rather than refused. That is the opposite
/// of `connections.rs`, which reports a file it cannot understand — and the
/// difference is deliberate: losing a credential's existence is worth telling
/// someone about, and losing a display preference is worth showing them their
/// buckets.
fn decode(contents: &str) -> Preferences {
    let mut preferences = Preferences::new();
    let mut connection: Option<String> = None;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[shows]]" {
            connection = None;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        let Some(value) = unquote(value) else {
            continue;
        };
        match key {
            "connection" => connection = Some(value),
            "buckets" => {
                if let Some(name) = connection.take() {
                    let buckets = value
                        .split(',')
                        .map(str::trim)
                        .filter(|bucket| !bucket.is_empty())
                        .map(ToOwned::to_owned)
                        .collect();
                    preferences.insert(name, buckets);
                }
            }
            _ => {}
        }
    }
    preferences
}

fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// `None` for anything that is not a quoted string, because a value this
/// cannot read is a line to skip rather than a file to reject.
fn unquote(value: &str) -> Option<String> {
    let inner = value.strip_prefix('"').and_then(|v| v.strip_suffix('"'))?;
    let mut out = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            out.push(characters.next()?);
        } else {
            out.push(character);
        }
    }
    Some(out)
}

/// Which buckets `connection` has chosen to show, if it has chosen.
///
/// `None` means no choice was recorded and every bucket should be listed.
/// That is also what a missing, unreadable or malformed file means: a display
/// preference must never be able to fail a listing, and the worst this data
/// can be is absent.
pub(crate) fn chosen_buckets(file: &dyn PreferencesFile, connection: &str) -> Option<Vec<String>> {
    let contents = file.read().ok()??;
    decode(&contents).get(connection).cloned()
}

/// Record which buckets `connection` shows.
///
/// Reads the file first and writes it back whole, so a choice recorded for one
/// connection does not erase another's — the same read-modify-write the
/// connections file does, and for the same reason.
pub(crate) fn choose_buckets(
    file: &dyn PreferencesFile,
    connection: &str,
    buckets: Vec<String>,
) -> Result<()> {
    let mut preferences = file.read()?.map(|c| decode(&c)).unwrap_or_default();
    preferences.insert(connection.to_owned(), buckets);
    file.write(&encode(&preferences))
}

/// Forget `connection`'s choice, so it shows everything again.
pub(crate) fn clear_choice(file: &dyn PreferencesFile, connection: &str) -> Result<()> {
    let Some(contents) = file.read()? else {
        return Ok(());
    };
    let mut preferences = decode(&contents);
    if preferences.remove(connection).is_none() {
        return Ok(());
    }
    file.write(&encode(&preferences))
}

/// The real file, in the platform's own configuration directory.
///
/// A unit struct for [`crate::connections::ConfigDirectory`]'s reason: the
/// path is resolved per call, so holding one costs no filesystem access.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PreferencesDirectory;

fn preferences_path() -> Option<PathBuf> {
    ProjectDirs::from("", "", "caixonho").map(|dirs| dirs.config_dir().join(FILE_NAME))
}

impl PreferencesFile for PreferencesDirectory {
    fn read(&self) -> Result<Option<String>> {
        let Some(path) = preferences_path() else {
            return Ok(None);
        };
        match std::fs::read_to_string(&path) {
            Ok(contents) => Ok(Some(contents)),
            // Every failure is "no choice recorded". Not laziness: this file
            // holds a display preference, and there is no failure mode of it
            // that is worth showing a user instead of their buckets.
            Err(_) => Ok(None),
        }
    }

    fn write(&self, contents: &str) -> Result<()> {
        let Some(path) = preferences_path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, contents);
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod double {
    //! A preferences file a test can hold in its hand.

    use std::sync::{Mutex, PoisonError};

    use super::PreferencesFile;
    use crate::error::Result;

    #[derive(Debug, Default)]
    pub(crate) struct PreferencesFileDouble {
        contents: Mutex<Option<String>>,
        /// Reads refuse, as an unreadable file on disk would.
        unreadable: bool,
    }

    impl PreferencesFileDouble {
        /// A machine that has never recorded a preference.
        pub(crate) fn empty() -> Self {
            Self::default()
        }

        /// A file already holding `contents` — malformed ones included, which
        /// is the point of taking a string rather than a structure.
        pub(crate) fn holding(contents: &str) -> Self {
            Self {
                contents: Mutex::new(Some(contents.to_owned())),
                unreadable: false,
            }
        }

        /// A file that cannot be read at all.
        pub(crate) fn unreadable() -> Self {
            Self {
                contents: Mutex::default(),
                unreadable: true,
            }
        }
    }

    impl PreferencesFile for PreferencesFileDouble {
        fn read(&self) -> Result<Option<String>> {
            if self.unreadable {
                return Err(crate::error::Error::Unexpected {
                    detail: "this double refuses reads".into(),
                });
            }
            Ok(self
                .contents
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone())
        }

        fn write(&self, contents: &str) -> Result<()> {
            *self.contents.lock().unwrap_or_else(PoisonError::into_inner) =
                Some(contents.to_owned());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    //! Every test here is about *absence*: no file, a broken file, a
    //! connection nobody chose for. Each one failing looks to a user like a
    //! bucket that has gone missing.

    use super::double::PreferencesFileDouble;
    use super::*;

    #[test]
    fn a_machine_that_has_chosen_nothing_shows_everything() {
        let file = PreferencesFileDouble::empty();

        assert_eq!(chosen_buckets(&file, "work"), None);
    }

    #[test]
    fn a_file_that_cannot_be_read_shows_everything() {
        // Never a failure the user sees. A display preference has no failure
        // mode worth showing someone instead of their buckets.
        let file = PreferencesFileDouble::unreadable();

        assert_eq!(chosen_buckets(&file, "work"), None);
    }

    #[test]
    fn a_file_that_makes_no_sense_shows_everything() {
        let file = PreferencesFileDouble::holding("this is not toml {{{");

        assert_eq!(chosen_buckets(&file, "work"), None);
    }

    #[test]
    fn a_choice_survives_being_written_and_read_again() {
        let file = PreferencesFileDouble::empty();

        choose_buckets(&file, "work", vec!["reports".into(), "logs".into()])
            .expect("the double accepts it");

        assert_eq!(
            chosen_buckets(&file, "work"),
            Some(vec!["reports".to_owned(), "logs".to_owned()])
        );
    }

    #[test]
    fn a_choice_made_for_one_connection_is_not_read_for_another() {
        let file = PreferencesFileDouble::empty();

        choose_buckets(&file, "work", vec!["reports".into()]).expect("accepted");

        assert_eq!(chosen_buckets(&file, "personal"), None);
    }

    #[test]
    fn choosing_for_one_connection_leaves_anothers_choice_alone() {
        // Read-modify-write, not write-over. Two connections chosen in one
        // run must both survive.
        let file = PreferencesFileDouble::empty();

        choose_buckets(&file, "work", vec!["reports".into()]).expect("accepted");
        choose_buckets(&file, "personal", vec!["photos".into()]).expect("accepted");

        assert_eq!(
            chosen_buckets(&file, "work"),
            Some(vec!["reports".to_owned()])
        );
        assert_eq!(
            chosen_buckets(&file, "personal"),
            Some(vec!["photos".to_owned()])
        );
    }

    #[test]
    fn an_empty_choice_is_a_choice_and_not_an_absent_one() {
        // The distinction the type exists for: "I chose nothing" and "I have
        // not chosen" are different, and collapsing them would make an empty
        // choice silently show every bucket.
        let file = PreferencesFileDouble::empty();

        choose_buckets(&file, "work", Vec::new()).expect("accepted");

        assert_eq!(chosen_buckets(&file, "work"), Some(Vec::new()));
    }

    #[test]
    fn a_cleared_choice_shows_everything_again() {
        let file = PreferencesFileDouble::empty();
        choose_buckets(&file, "work", vec!["reports".into()]).expect("accepted");

        clear_choice(&file, "work").expect("accepted");

        assert_eq!(chosen_buckets(&file, "work"), None);
    }

    #[test]
    fn clearing_a_choice_nobody_made_is_not_a_failure() {
        let file = PreferencesFileDouble::empty();

        assert!(clear_choice(&file, "work").is_ok());
    }
}
