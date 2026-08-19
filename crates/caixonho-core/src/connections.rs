//! The connections this application remembers between runs.
//!
//! `connection.rs` opens one connection; this module is the list of the ones
//! the user has entered here, and the file that outlives the process. Only the
//! half that is not secret is written: a name, a region and an access key id
//! (`stored-credentials` spec, "Everything except the secret is ordinary
//! configuration"). The secret access key and the session token stay in the
//! operating system's credential store and are never in this file, in any
//! spelling.
//!
//! **Why this exists at all.** The secret was going to the keychain and
//! nothing was keeping the rest, so a connection entered in the app vanished
//! on restart while its secret stayed behind — an orphan the application could
//! no longer see, name or delete, leaving the user to open Keychain Access and
//! clean up after a client whose whole promise is that they do not have to
//! touch files by hand (design.md, "A stored connection is remembered, or it
//! should not be offered at all").
//!
//! That gives the one rule the two write paths here are built around:
//!
//! > **The residue of a partial failure is always something the application
//! > can name and remove, never a secret it cannot see.**
//!
//! Which fixes both orders. Remembering writes the configuration entry first
//! and the secret second, and takes the configuration entry back out if the
//! secret will not go in. Forgetting deletes the secret first and the
//! configuration entry second, and does not touch the configuration at all if
//! the secret could not be deleted. Either way the worst that survives a
//! failure is a connection with no secret — visible, and forgettable again.
//!
//! **A file this cannot read is reported, never replaced.** It is the user's
//! file. Overwriting one we failed to parse would discard every connection in
//! it to save the one being written, so a failed read stops the write
//! ([`ConnectionsProblem`]).

use std::path::PathBuf;

use directories::ProjectDirs;

use crate::credentials::{self, CredentialSecret, SecretStore, StoredCredential};
use crate::error::{ConnectionsProblem, Error, Result};

/// What the file is called, under the platform's own config directory.
const FILE_NAME: &str = "connections.toml";

/// Written at the top of every file this module produces.
///
/// It is addressed to whoever opens the file expecting to find a key in it.
const HEADER: &str = "\
# caixonho connections.
#
# The half of each connection that is not secret: what it is called, the
# region it uses, and its access key id. No secret is ever written here.
# A secret access key and a session token go to this operating system's own
# credential store, and nowhere else.
#
# Deleting this file loses no secret. It loses caixonho's knowledge that one
# exists, which is why caixonho reports a file it cannot read and leaves it
# exactly as it is, rather than replacing it.
";

/// Where the remembered connections are kept.
///
/// A port, for the same reason the credential store beside it is one: every
/// test in this module runs against [`double::ConnectionFileDouble`] and none
/// of them touches a real config directory. The one implementation that does
/// ([`ConfigDirectory`]) stays thin enough to read in a sitting — it resolves
/// a path, reads text, writes text.
///
/// Text rather than parsed records on purpose: what is written is exactly
/// what a test can look at, which is how
/// `the_written_file_holds_the_configuration_and_never_the_secret` can assert
/// on the file itself rather than on a description of it.
pub(crate) trait ConnectionFile: std::fmt::Debug + Send + Sync {
    /// Where this file is, for a message that has to name it. `None` when
    /// this platform gave us nowhere to put one.
    fn location(&self) -> Option<PathBuf>;

    /// What the file holds.
    ///
    /// `Ok(None)` is an answer: there is no file yet, which is what a first
    /// run looks like and is not a failure.
    fn read(&self) -> Result<Option<String>>;

    /// Replace the file's contents with `contents`.
    fn write(&self, contents: &str) -> Result<()>;
}

/// Every connection this application remembers, by name.
///
/// No file yet is an empty list, not a failure — that is a first run. A file
/// that cannot be read or understood is neither: it is reported as its own
/// cause, so the user is told their connections could not be read instead of
/// being shown a machine that appears to have none.
pub(crate) fn list(file: &dyn ConnectionFile) -> Result<Vec<StoredCredential>> {
    match file.read()? {
        None => Ok(Vec::new()),
        Some(contents) => decode(&contents).map_err(|problem| Error::Connections {
            problem,
            path: file.location(),
        }),
    }
}

/// Remember `credential`, with `secret` in the credential store.
///
/// The configuration entry goes in first and the secret second. If the store
/// will not take the secret, the configuration entry comes back out — and if
/// even that fails, what is left is a connection with no secret, which the
/// user can see and forget. The other order cannot say that: it leaves a
/// secret filed under a name the application no longer knows.
pub(crate) fn remember(
    file: &dyn ConnectionFile,
    secrets: &dyn SecretStore,
    credential: &StoredCredential,
    secret: &CredentialSecret,
) -> Result<()> {
    let previously = list(file)?;

    let mut remembered = previously.clone();
    // Entering a credential under a name already in use replaces it rather
    // than listing it twice: the name is also the key its secret is filed
    // under, so two entries would be two views of one keychain entry, and
    // forgetting either would strand the other.
    remembered.retain(|known| known.name() != credential.name());
    remembered.push(credential.clone());
    write_all(file, &remembered)?;

    match credentials::save(secrets, credential, secret) {
        Ok(()) => Ok(()),
        Err(refusal) => {
            // Put the list back as it was, rather than taking the new entry
            // out. The two differ exactly when this credential replaced one
            // of the same name — a key being rotated — and there the store
            // still holds the *previous* secret, so removing the entry would
            // strand it under a name this application no longer lists.
            //
            // Best effort, and its failure is not what gets reported: the
            // refusal above is the real cause, and the residue of this one is
            // a connection with no secret rather than a secret with no name.
            let _ = write_all(file, &previously);
            Err(refusal)
        }
    }
}

/// Forget the connection called `name`, secret first.
///
/// The order is the point. A credential store that will not delete stops this
/// before the configuration entry goes, so the user sees a connection they can
/// try to forget again; deleting the configuration entry first would leave a
/// secret in the keychain that the application can no longer see, name or
/// remove.
///
/// If the second step fails the first still happened — the secret is gone —
/// and that is reported rather than hidden.
pub(crate) fn forget(
    file: &dyn ConnectionFile,
    secrets: &dyn SecretStore,
    name: &str,
) -> Result<()> {
    credentials::forget(secrets, name)?;
    remove(file, name)
}

/// Drop `name` from the file, leaving every other connection alone.
fn remove(file: &dyn ConnectionFile, name: &str) -> Result<()> {
    let mut remembered = list(file)?;
    let before = remembered.len();
    remembered.retain(|known| known.name() != name);
    if remembered.len() == before {
        // Nothing to do, and rewriting a file to change nothing is a chance
        // to lose it for no reason.
        return Ok(());
    }
    write_all(file, &remembered)
}

/// Write the whole list, in a stable order.
///
/// Sorted by name so the file does not reshuffle between runs and a diff of it
/// says something.
fn write_all(file: &dyn ConnectionFile, remembered: &[StoredCredential]) -> Result<()> {
    let mut remembered = remembered.to_vec();
    remembered.sort_by(|a, b| a.name().cmp(b.name()));
    file.write(&encode(&remembered))
}

/// The file's text for `remembered`.
///
/// Three fields, named explicitly. Nothing iterates a credential's contents,
/// so a field added to [`StoredCredential`] later cannot arrive here by
/// accident — which is the property that keeps a secret out if one is ever
/// added to that type.
fn encode(remembered: &[StoredCredential]) -> String {
    let mut out = String::from(HEADER);
    for credential in remembered {
        out.push_str("\n[[connection]]\n");
        for (key, value) in [
            ("name", credential.name()),
            ("region", credential.region()),
            ("access_key_id", credential.access_key_id()),
        ] {
            out.push_str(key);
            out.push_str(" = ");
            out.push_str(&quote(value));
            out.push('\n');
        }
    }
    out
}

/// Read the file's text back.
///
/// Strict on purpose, and its own cause when it refuses: anything not written
/// by [`encode`] is a file we did not write, and guessing at it would mean
/// dropping the parts we did not understand — connections the user would
/// silently stop being offered. The pure half of the parse, so the location
/// is attached above it and the rules below are testable without one.
fn decode(contents: &str) -> std::result::Result<Vec<StoredCredential>, ConnectionsProblem> {
    let mut remembered: Vec<StoredCredential> = Vec::new();
    let mut current: Option<Record> = None;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[connection]]" {
            if let Some(record) = current.replace(Record::default()) {
                remembered.push(record.finish()?);
            }
            continue;
        }
        // Some other table. This file has one shape; a second one means a
        // file we did not write, and skipping it would be a guess.
        if line.starts_with('[') {
            return Err(ConnectionsProblem::Malformed);
        }

        let record = current.as_mut().ok_or(ConnectionsProblem::Malformed)?;
        let (key, value) = line.split_once('=').ok_or(ConnectionsProblem::Malformed)?;
        record.set(key.trim(), unquote(value.trim())?)?;
    }
    if let Some(record) = current {
        remembered.push(record.finish()?);
    }

    // A name addresses one entry in the credential store, so two connections
    // under one name are two views of one secret: forgetting either would
    // strand the other.
    let mut names: Vec<&str> = remembered.iter().map(StoredCredential::name).collect();
    names.sort_unstable();
    let unique = names.len();
    names.dedup();
    if names.len() != unique {
        return Err(ConnectionsProblem::Malformed);
    }

    Ok(remembered)
}

/// One `[[connection]]` table, while it is being read.
#[derive(Debug, Default)]
struct Record {
    name: Option<String>,
    region: Option<String>,
    access_key_id: Option<String>,
}

impl Record {
    /// Take one `key = value` line.
    ///
    /// A key this does not know, or one given twice, is malformed rather than
    /// ignored: a setting that is silently dropped is a setting that silently
    /// does nothing.
    fn set(&mut self, key: &str, value: String) -> std::result::Result<(), ConnectionsProblem> {
        let field = match key {
            "name" => &mut self.name,
            "region" => &mut self.region,
            "access_key_id" => &mut self.access_key_id,
            _ => return Err(ConnectionsProblem::Malformed),
        };
        if field.replace(value).is_some() {
            return Err(ConnectionsProblem::Malformed);
        }
        Ok(())
    }

    /// The credential this table describes.
    ///
    /// All three fields are required. The name must also be non-empty,
    /// because it is the key the secret is filed under and an empty one
    /// addresses nothing — but an empty region or access key id is left
    /// alone: that is a connection the user has yet to finish, and it fails
    /// with its own message when opened rather than making the whole file
    /// unreadable.
    fn finish(self) -> std::result::Result<StoredCredential, ConnectionsProblem> {
        let (Some(name), Some(region), Some(access_key_id)) =
            (self.name, self.region, self.access_key_id)
        else {
            return Err(ConnectionsProblem::Malformed);
        };
        if name.is_empty() {
            return Err(ConnectionsProblem::Malformed);
        }
        Ok(StoredCredential::new(name, region, access_key_id))
    }
}

/// `value` as a quoted string.
///
/// Quoted rather than bare so that a connection called `  work  ` survives a
/// round trip: a name is whatever the user typed, and it is also the key its
/// secret is filed under, so a name that comes back different from how it went
/// in points at the wrong keychain entry.
fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// The string a quoted value stands for.
fn unquote(value: &str) -> std::result::Result<String, ConnectionsProblem> {
    let inner = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or(ConnectionsProblem::Malformed)?;

    let mut out = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        match character {
            // The value ended before the line did: `"a" "b"` is two values
            // where one belongs, and taking the first would drop the rest.
            '"' => return Err(ConnectionsProblem::Malformed),
            '\\' => match characters.next() {
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                _ => return Err(ConnectionsProblem::Malformed),
            },
            other => out.push(other),
        }
    }
    Ok(out)
}

/// The real file, in the platform's own configuration directory.
///
/// A unit struct because it holds nothing: the path is resolved per call, so
/// building a [`crate::Session`] costs no filesystem access and a machine with
/// nowhere to put the file is reported when something is actually asked of it
/// rather than at startup.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ConfigDirectory;

/// Where the file lives on this machine.
///
/// `directories` resolves it, rather than this repository assembling paths out
/// of `HOME` and `APPDATA`: that crate is tested on three platforms, which
/// this repository is not yet in a position to be, and Windows is both a
/// first-class target here and the one least likely to be exercised by hand
/// (design.md).
///
/// With no qualifier and no organization the three targets come out as:
///
/// - macOS: `~/Library/Application Support/caixonho/connections.toml`
/// - Windows: `%APPDATA%\caixonho\config\connections.toml`
/// - Linux: `$XDG_CONFIG_HOME/caixonho/connections.toml`
///
/// Read out of `directories` 6.0.0's own source rather than recalled.
fn connections_path() -> Option<PathBuf> {
    ProjectDirs::from("", "", "caixonho").map(|dirs| dirs.config_dir().join(FILE_NAME))
}

/// A platform with no configuration directory at all.
fn nowhere() -> Error {
    Error::Connections {
        problem: ConnectionsProblem::NoLocation,
        path: None,
    }
}

/// What this crate makes of a filesystem failure while reading.
///
/// Its own function so the one judgement here is testable without arranging a
/// real unreadable file. `NotFound` never reaches it — the caller reads that
/// as "no file yet", which is an answer.
fn read_problem(kind: std::io::ErrorKind) -> ConnectionsProblem {
    match kind {
        // The bytes were there and are not text. The file is intact and is
        // not ours to replace, which is what `Malformed` means here.
        std::io::ErrorKind::InvalidData => ConnectionsProblem::Malformed,
        _ => ConnectionsProblem::Unreadable,
    }
}

impl ConnectionFile for ConfigDirectory {
    fn location(&self) -> Option<PathBuf> {
        connections_path()
    }

    fn read(&self) -> Result<Option<String>> {
        let path = self.location().ok_or_else(nowhere)?;
        match std::fs::read_to_string(&path) {
            Ok(contents) => Ok(Some(contents)),
            // A first run, or a file the user removed. Neither is a failure.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Error::Connections {
                problem: read_problem(error.kind()),
                path: Some(path),
            }),
        }
    }

    fn write(&self, contents: &str) -> Result<()> {
        let path = self.location().ok_or_else(nowhere)?;
        let failed = |_| Error::Connections {
            problem: ConnectionsProblem::NotWritable,
            path: Some(path.clone()),
        };

        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory).map_err(failed)?;
        }
        // Written beside the file and moved onto it, so a write that dies
        // half way leaves the previous list intact. The whole file is
        // rewritten on every change, and a torn one would lose every
        // connection in it to save the one being added.
        let staged = path.with_extension("toml.new");
        std::fs::write(&staged, contents).map_err(failed)?;
        std::fs::rename(&staged, &path).map_err(failed)
    }
}

#[cfg(test)]
pub(crate) mod double {
    //! A connections file a test can hold in its hand: what it holds, what it
    //! will refuse, and nothing that touches the machine.

    use std::sync::{Mutex, PoisonError};

    use super::ConnectionFile;
    use crate::error::{ConnectionsProblem, Error, Result};

    /// A [`ConnectionFile`] that keeps its contents in memory.
    #[derive(Debug, Default)]
    pub(crate) struct ConnectionFileDouble {
        contents: Mutex<Option<String>>,
        /// The cause every read refuses with, if reads are refused.
        unreadable: Option<ConnectionsProblem>,
        /// The cause every write refuses with, if writes are refused.
        unwritable: Option<ConnectionsProblem>,
    }

    impl ConnectionFileDouble {
        /// A machine that has never saved a connection.
        pub(crate) fn empty() -> Self {
            Self::default()
        }

        /// A file already holding `contents`.
        pub(crate) fn holding(contents: &str) -> Self {
            Self {
                contents: Mutex::new(Some(contents.to_owned())),
                ..Self::default()
            }
        }

        /// The same file, refusing every read with `problem`.
        pub(crate) fn unreadable(mut self, problem: ConnectionsProblem) -> Self {
            self.unreadable = Some(problem);
            self
        }

        /// The same file, refusing every write with `problem`.
        pub(crate) fn unwritable(mut self, problem: ConnectionsProblem) -> Self {
            self.unwritable = Some(problem);
            self
        }

        /// Exactly what the file holds now.
        pub(crate) fn contents(&self) -> Option<String> {
            self.contents
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }

        /// The double's own path, so a message can name something.
        fn path(&self) -> std::path::PathBuf {
            std::path::PathBuf::from("/nowhere/connections.toml")
        }

        fn refusal(&self, problem: Option<ConnectionsProblem>) -> Option<Error> {
            problem.map(|problem| Error::Connections {
                problem,
                path: Some(self.path()),
            })
        }
    }

    impl ConnectionFile for ConnectionFileDouble {
        fn location(&self) -> Option<std::path::PathBuf> {
            Some(self.path())
        }

        fn read(&self) -> Result<Option<String>> {
            match self.refusal(self.unreadable) {
                Some(refusal) => Err(refusal),
                None => Ok(self.contents()),
            }
        }

        fn write(&self, contents: &str) -> Result<()> {
            if let Some(refusal) = self.refusal(self.unwritable) {
                return Err(refusal);
            }
            *self.contents.lock().unwrap_or_else(PoisonError::into_inner) =
                Some(contents.to_owned());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    //! `stored-credentials` spec, and tasks.md 4.0 — what is written, what is
    //! read back, and what a partial failure leaves behind. Every test runs
    //! against doubles of both stores, so none of them touches a real config
    //! directory or a real keychain.

    use super::double::ConnectionFileDouble;
    use super::*;
    use crate::credentials::double::SecretStoreDouble;
    use crate::error::CredentialStoreProblem;

    /// The secret the tests put through the store. Not a real key — but it is
    /// treated as one everywhere below, which is the point.
    const SECRET: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    const TOKEN: &str = "FQoGZXIvYXdzEExampleSessionToken";

    /// The same, made of the characters this file format has to escape.
    ///
    /// Its readable spelling and its spelling *in the file* differ, which is
    /// what makes the third check in
    /// `the_written_file_holds_the_configuration_and_never_the_secret` bite:
    /// a secret written out through [`quote`] would slip past a search for
    /// the readable one.
    const AWKWARD_SECRET: &str = "wJalr\\XUtn\"FEMI\tEXAMPLEKEY";
    const AWKWARD_TOKEN: &str = "FQoGZXIv\\YXdz\"ExampleSessionToken";

    fn credential() -> StoredCredential {
        StoredCredential::new("work", "ap-southeast-1", "AKIAIOSFODNN7EXAMPLE")
    }

    fn long_lived() -> CredentialSecret {
        CredentialSecret::new(SECRET, None)
    }

    fn temporary() -> CredentialSecret {
        CredentialSecret::new(SECRET, Some(TOKEN.to_owned()))
    }

    /// A file already remembering `remembered`, written the way this module
    /// writes one.
    fn file_remembering(remembered: &[StoredCredential]) -> ConnectionFileDouble {
        ConnectionFileDouble::holding(&encode(remembered))
    }

    #[test]
    fn the_written_file_holds_the_configuration_and_never_the_secret() {
        // Invariant 5, at the one place this change could break it: the file
        // is new, and it is the only thing this application writes that a
        // secret could reach.
        for (secret, token) in [(SECRET, TOKEN), (AWKWARD_SECRET, AWKWARD_TOKEN)] {
            let file = ConnectionFileDouble::empty();
            let secrets = SecretStoreDouble::open();

            remember(
                &file,
                &secrets,
                &credential(),
                &CredentialSecret::new(secret, Some(token.to_owned())),
            )
            .expect("an open store and a writable file accept it");

            let written = file.contents().expect("something was written");
            for configuration in ["work", "ap-southeast-1", "AKIAIOSFODNN7EXAMPLE"] {
                assert!(
                    written.contains(configuration),
                    "`{configuration}` is what this file is for: {written}"
                );
            }
            for material in [secret, token] {
                // Three spellings, because a secret can reach a file looking
                // like something other than itself. The readable one is the
                // obvious route; the byte-array one is the trap
                // `no_credential_store_failure_ever_discloses_the_secret`
                // documents, where `{:?}` on a payload prints
                // `[119, 74, 97, ...]`; and the quoted one is this file's
                // own, where a secret written as a value would arrive
                // escaped and sail straight past a search for the readable
                // string.
                for disclosure in [
                    material.to_owned(),
                    format!("{:?}", material.as_bytes()),
                    quote(material),
                ] {
                    assert!(
                        !written.contains(&disclosure),
                        "a secret reached the configuration file as `{disclosure}`: {written}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_connection_saved_and_loaded_again_is_the_same_connection() {
        // Which is the whole point of 4.0: without it the connection vanishes
        // on restart and its secret does not.
        let file = ConnectionFileDouble::empty();
        let secrets = SecretStoreDouble::open();
        // Names are whatever the user typed. A name that does not survive the
        // round trip points at the wrong entry in the credential store.
        let awkward = StoredCredential::new(
            "  prod = \"eu\" \\ two\nlines  #  ",
            "eu-west-1",
            "AKIAOTHEREXAMPLE",
        );
        let unicode = StoredCredential::new("kho ảnh", "ap-southeast-1", "AKIAUNICODEEXAMPLE");

        remember(&file, &secrets, &credential(), &temporary()).expect("accepted");
        remember(&file, &secrets, &awkward, &long_lived()).expect("accepted");
        remember(&file, &secrets, &unicode, &long_lived()).expect("accepted");

        let remembered = list(&file).expect("what was written can be read");
        assert_eq!(remembered.len(), 3);
        for original in [credential(), awkward, unicode] {
            assert!(
                remembered.contains(&original),
                "`{}` did not survive the round trip: {remembered:?}",
                original.name()
            );
        }
    }

    #[test]
    fn nothing_remembered_yet_is_an_empty_list_not_a_failure() {
        assert!(
            list(&ConnectionFileDouble::empty())
                .expect("a first run is not a failure")
                .is_empty()
        );
    }

    #[test]
    fn a_credential_store_that_will_not_delete_stops_before_the_configuration_entry() {
        // The order this task exists for. Deleting the configuration entry
        // first would leave a secret in the keychain filed under a name the
        // application no longer knows — an orphan only Keychain Access can
        // clear. Reverse the two steps in `forget` and this goes red.
        let file = file_remembering(&[credential()]);
        let secrets = SecretStoreDouble::refusing(CredentialStoreProblem::Locked);

        let forgotten = forget(&file, &secrets, "work");

        assert!(
            matches!(
                forgotten,
                Err(Error::CredentialStore {
                    problem: CredentialStoreProblem::Locked,
                    ..
                })
            ),
            "got {forgotten:?}"
        );
        assert_eq!(
            list(&file).expect("the file is untouched"),
            vec![credential()],
            "the connection stays listed, so the user can try to forget it again"
        );
    }

    #[test]
    fn a_configuration_that_cannot_be_written_does_not_leave_the_secret_undeleted() {
        // The other half of the order: the second step failing must not undo
        // the first. What is left is a connection with no secret — which the
        // user can see, and forget again.
        let file = file_remembering(&[credential()]).unwritable(ConnectionsProblem::NotWritable);
        let secrets = SecretStoreDouble::open();
        credentials::save(&secrets, &credential(), &temporary()).expect("accepted");

        let forgotten = forget(&file, &secrets, "work");

        assert!(
            matches!(
                forgotten,
                Err(Error::Connections {
                    problem: ConnectionsProblem::NotWritable,
                    ..
                })
            ),
            "got {forgotten:?}"
        );
        assert!(
            secrets.holds().is_empty(),
            "the secret was deleted first and stays deleted"
        );
    }

    #[test]
    fn forgetting_a_connection_removes_both_halves_and_leaves_the_others_alone() {
        let file = ConnectionFileDouble::empty();
        let secrets = SecretStoreDouble::open();
        let other = StoredCredential::new("personal", "eu-west-1", "AKIAOTHEREXAMPLE");
        remember(&file, &secrets, &credential(), &temporary()).expect("accepted");
        remember(&file, &secrets, &other, &long_lived()).expect("accepted");

        forget(&file, &secrets, "work").expect("an open store and a writable file forget it");

        assert_eq!(
            list(&file).expect("readable"),
            vec![other],
            "the forgotten connection is no longer offered, and the other one still is"
        );
        assert!(
            !secrets
                .holds()
                .keys()
                .any(|(connection, _)| connection == "work"),
            "and nothing of it is left to be signed with"
        );
    }

    #[test]
    fn a_configuration_that_cannot_be_written_leaves_no_orphaned_secret() {
        // The failure 4.0 was written for, from the saving side.
        let file = ConnectionFileDouble::empty().unwritable(ConnectionsProblem::NotWritable);
        let secrets = SecretStoreDouble::open();

        let saved = remember(&file, &secrets, &credential(), &temporary());

        assert!(
            matches!(
                saved,
                Err(Error::Connections {
                    problem: ConnectionsProblem::NotWritable,
                    ..
                })
            ),
            "got {saved:?}"
        );
        assert!(
            secrets.holds().is_empty(),
            "a connection that could not be recorded must not leave a secret behind"
        );
    }

    #[test]
    fn a_credential_the_store_refuses_is_not_left_in_the_configuration() {
        let file = ConnectionFileDouble::empty();
        let secrets = SecretStoreDouble::refusing(CredentialStoreProblem::Refused);

        let saved = remember(&file, &secrets, &credential(), &temporary());

        assert!(
            matches!(saved, Err(Error::CredentialStore { .. })),
            "got {saved:?}"
        );
        assert!(
            list(&file).expect("readable").is_empty(),
            "a connection whose secret was refused must not be offered afterwards"
        );
    }

    #[test]
    fn a_replacement_the_store_refuses_leaves_the_connection_it_would_have_replaced() {
        // Re-entering a credential under a name already in use is how a key
        // is rotated, editing in place not being offered. If the store then
        // refuses the new secret, the *old* secret is still in the keychain —
        // so dropping the entry entirely would strand it under a name this
        // application no longer lists, which is the orphan 4.0 exists to
        // prevent. What comes back is the connection that was there before,
        // which is the one the keychain still matches.
        let file = file_remembering(&[credential()]);
        let secrets = SecretStoreDouble::refusing(CredentialStoreProblem::Refused);
        let rotated = StoredCredential::new("work", "us-east-1", "AKIANEWEREXAMPLE");

        let saved = remember(&file, &secrets, &rotated, &long_lived());

        assert!(
            matches!(saved, Err(Error::CredentialStore { .. })),
            "got {saved:?}"
        );
        assert_eq!(
            list(&file).expect("readable"),
            vec![credential()],
            "the secret still in the keychain is the one that was there before, \
             so that is the connection the file has to go on naming"
        );
    }

    #[test]
    fn remembering_a_name_already_in_use_replaces_it_rather_than_listing_it_twice() {
        // Editing in place is not offered — a credential is forgotten and
        // entered again (design.md, Non-Goals) — so the same name arriving
        // twice is normal. Two entries would be two views of one keychain
        // entry.
        let file = ConnectionFileDouble::empty();
        let secrets = SecretStoreDouble::open();
        remember(&file, &secrets, &credential(), &temporary()).expect("accepted");

        let moved = StoredCredential::new("work", "us-east-1", "AKIANEWEREXAMPLE");
        remember(&file, &secrets, &moved, &long_lived()).expect("accepted");

        assert_eq!(list(&file).expect("readable"), vec![moved]);
    }

    #[test]
    fn a_configuration_this_cannot_understand_is_its_own_cause_not_an_empty_list() {
        // Reporting it as an empty list would tell the user this machine has
        // no connections, which is a different and false statement.
        for broken in [
            "[[connection]]\nname = \"work\"\n",
            "[[connection]]\nname = \"work\"\nregion = ap-southeast-1\naccess_key_id = \"A\"\n",
            "name = \"work\"\n",
            "[[connections]]\nname = \"work\"\nregion = \"x\"\naccess_key_id = \"A\"\n",
            "[[connection]]\nname = \"work\"\nregion = \"x\"\naccess_key_id = \"A\"\nnickname = \"w\"\n",
            "[[connection]]\nname = \"work\"\nname = \"work\"\nregion = \"x\"\naccess_key_id = \"A\"\n",
            "[[connection]]\nname = \"\"\nregion = \"x\"\naccess_key_id = \"A\"\n",
            "[[connection]]\nname = \"work\nregion = \"x\"\naccess_key_id = \"A\"\n",
            "[[connection]]\nname = \"wo\\qrk\"\nregion = \"x\"\naccess_key_id = \"A\"\n",
            "[[connection]]\nname = \"a\" \"b\"\nregion = \"x\"\naccess_key_id = \"A\"\n",
            "[[connection]]\nname = \"work\"\nregion = \"x\"\naccess_key_id = \"A\"\n\
             [[connection]]\nname = \"work\"\nregion = \"y\"\naccess_key_id = \"B\"\n",
        ] {
            let file = ConnectionFileDouble::holding(broken);

            match list(&file) {
                Err(Error::Connections {
                    problem: ConnectionsProblem::Malformed,
                    path,
                }) => assert!(path.is_some(), "the message has to name the file"),
                other => panic!("expected Malformed for `{broken}`, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_configuration_this_cannot_understand_is_never_replaced_by_one_it_can() {
        // The file is the user's. Overwriting one we failed to parse would
        // discard every connection in it to save the one being written —
        // silently, and permanently.
        let broken = "[[connection]]\nname = \"work\"\n";
        let file = ConnectionFileDouble::holding(broken);
        let secrets = SecretStoreDouble::open();

        let saved = remember(&file, &secrets, &credential(), &temporary());
        let forgotten = forget(&file, &secrets, "work");

        assert!(
            matches!(saved, Err(Error::Connections { .. })),
            "got {saved:?}"
        );
        assert!(
            matches!(forgotten, Err(Error::Connections { .. })),
            "got {forgotten:?}"
        );
        assert_eq!(
            file.contents().as_deref(),
            Some(broken),
            "a file we could not read is not ours to replace"
        );
        assert!(
            secrets.holds().is_empty(),
            "and no secret goes in against a connection that could not be recorded"
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_at_all_is_its_own_cause() {
        let file = ConnectionFileDouble::empty().unreadable(ConnectionsProblem::Unreadable);

        match list(&file) {
            Err(Error::Connections {
                problem: ConnectionsProblem::Unreadable,
                ..
            }) => {}
            other => panic!("expected Unreadable, got {other:?}"),
        }
    }

    #[test]
    fn a_broken_configuration_is_never_reported_as_an_access_denial_or_a_store_failure() {
        // The invariant the error enum exists for. A file the user can repair
        // is not an IAM policy and not a locked keychain, and sending someone
        // to either is the failure mode this crate is built to prevent.
        for file in [
            ConnectionFileDouble::holding("nonsense"),
            ConnectionFileDouble::empty().unreadable(ConnectionsProblem::Unreadable),
        ] {
            let reported = list(&file).expect_err("this file cannot be used");
            assert!(
                matches!(reported, Error::Connections { .. }),
                "a cause of its own, not a generic failure: {reported:?}"
            );
        }
    }

    #[test]
    fn a_filesystem_failure_keeps_its_own_cause() {
        // The one judgement the real file makes, tested without a real file.
        assert_eq!(
            read_problem(std::io::ErrorKind::PermissionDenied),
            ConnectionsProblem::Unreadable
        );
        // Bytes that are not text: the file is intact and is not ours to
        // replace, which is what `Malformed` means here.
        assert_eq!(
            read_problem(std::io::ErrorKind::InvalidData),
            ConnectionsProblem::Malformed
        );
    }

    #[test]
    fn every_way_this_file_can_fail_says_where_it_is_and_never_what_is_in_the_keychain() {
        for problem in [
            ConnectionsProblem::Unreadable,
            ConnectionsProblem::Malformed,
            ConnectionsProblem::NotWritable,
            ConnectionsProblem::NoLocation,
        ] {
            let reported = Error::Connections {
                problem,
                path: Some(PathBuf::from("/somewhere/connections.toml")),
            };
            let rendered = format!("{reported} {reported:?}");

            assert!(
                rendered.contains("/somewhere/connections.toml"),
                "{problem:?} must name the file: {rendered}"
            );
            for material in [SECRET, TOKEN] {
                assert!(!rendered.contains(material), "{rendered}");
            }
        }
    }

    #[test]
    fn the_file_lands_in_the_platform_s_own_configuration_directory() {
        // Resolved, not assembled: `directories` knows the Known Folder API on
        // Windows and Application Support on macOS, and this repository does
        // not yet test on three platforms.
        let path = connections_path().expect("this machine has a config directory");

        assert_eq!(
            path.file_name().and_then(std::ffi::OsStr::to_str),
            Some(FILE_NAME)
        );
        assert!(
            path.ancestors().any(
                |ancestor| ancestor.file_name().and_then(std::ffi::OsStr::to_str)
                    == Some("caixonho")
            ),
            "the application's own directory, not somebody else's: {}",
            path.display()
        );
    }

    #[test]
    fn the_file_says_out_loud_that_no_secret_is_in_it() {
        // Whoever opens this file looking for a key should be told where the
        // key actually is before they go looking for it elsewhere.
        let written = encode(&[credential()]);

        assert!(written.starts_with('#'), "{written}");
        assert!(written.contains("credential store"), "{written}");
    }

    #[test]
    fn the_order_connections_are_written_in_does_not_depend_on_the_order_they_arrived() {
        // A file that reshuffles between runs is a file whose diff says
        // nothing.
        let first = ConnectionFileDouble::empty();
        let second = ConnectionFileDouble::empty();
        let secrets = SecretStoreDouble::open();
        let other = StoredCredential::new("personal", "eu-west-1", "AKIAOTHEREXAMPLE");

        remember(&first, &secrets, &credential(), &long_lived()).expect("accepted");
        remember(&first, &secrets, &other, &long_lived()).expect("accepted");
        remember(&second, &secrets, &other, &long_lived()).expect("accepted");
        remember(&second, &secrets, &credential(), &long_lived()).expect("accepted");

        assert_eq!(first.contents(), second.contents());
    }
}
