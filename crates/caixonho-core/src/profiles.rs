//! Profile discovery: which connections exist on this machine.
//!
//! Reads the AWS shared configuration files directly rather than asking the
//! SDK, because discovery must not require a credential to be valid — a
//! profile that exists but cannot authenticate still belongs in the picker,
//! and its failure surfaces only when a connection is opened (`connections`
//! spec, "Profile discovery").
//!
//! Only the section headers are parsed here. Everything inside a section is
//! the SDK's business; duplicating its precedence rules would be a second
//! source of truth that drifts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::sso::SignInLocation;
use crate::types::Profile;

/// Where the shared configuration lives.
///
/// Environment overrides win over the default paths, which is what lets
/// tests point at fixtures without touching the developer's real `~/.aws`.
#[derive(Debug, Clone)]
pub struct ConfigPaths {
    /// `~/.aws/config` or `AWS_CONFIG_FILE`.
    pub config: Option<PathBuf>,
    /// `~/.aws/credentials` or `AWS_SHARED_CREDENTIALS_FILE`.
    pub credentials: Option<PathBuf>,
}

impl ConfigPaths {
    /// Resolve from the process environment, honouring `AWS_CONFIG_FILE` and
    /// `AWS_SHARED_CREDENTIALS_FILE`, falling back to `~/.aws/*`.
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let pick = |var: &str, default: &str| -> Option<PathBuf> {
            match std::env::var_os(var) {
                Some(explicit) if !explicit.is_empty() => Some(PathBuf::from(explicit)),
                _ => home.as_ref().map(|h| h.join(".aws").join(default)),
            }
        };
        Self {
            config: pick("AWS_CONFIG_FILE", "config"),
            credentials: pick("AWS_SHARED_CREDENTIALS_FILE", "credentials"),
        }
    }
}

/// List the profiles declared on this machine.
///
/// A missing file is not an error: no configuration at all yields an empty
/// list, which the UI reports as "no profiles found" rather than as an
/// authentication failure. Order is stable — `default` first, then named
/// profiles alphabetically — so the picker does not reshuffle between runs.
pub fn discover(paths: &ConfigPaths) -> Result<Vec<Profile>> {
    let mut names: Vec<String> = Vec::new();

    for (path, style) in [
        (paths.config.as_deref(), SectionStyle::Config),
        (paths.credentials.as_deref(), SectionStyle::Credentials),
    ] {
        let Some(path) = path else { continue };
        for name in read_section_names(path, style) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }

    names.sort_by(|a, b| match (a.as_str(), b.as_str()) {
        ("default", "default") => std::cmp::Ordering::Equal,
        ("default", _) => std::cmp::Ordering::Less,
        (_, "default") => std::cmp::Ordering::Greater,
        (a, b) => a.cmp(b),
    });

    Ok(names
        .into_iter()
        .map(|name| Profile {
            is_default: name == "default",
            name,
        })
        .collect())
}

/// The `sso_session` a profile belongs to, when it declares one.
///
/// Only the config file carries it, and only as a plain `key = value` inside
/// the profile's own section. Read on demand rather than during discovery:
/// it exists to name the session in an expired-session message
/// (`connections` spec), and discovery must stay cheap and total.
pub fn sso_session(paths: &ConfigPaths, profile: &str) -> Option<String> {
    let contents = std::fs::read_to_string(paths.config.as_deref()?).ok()?;
    let wanted = [format!("[profile {profile}]"), format!("[{profile}]")];

    let mut inside = false;
    for line in contents.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = wanted.iter().any(|header| line == header);
            continue;
        }
        if !inside {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
            && key.trim() == "sso_session"
        {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}

/// Where a profile signs in, read from the `[sso-session <name>]` section it
/// points at.
///
/// Three fields are needed and none can be invented: the portal URL, the
/// region the Identity Center instance lives in, and the session name itself,
/// which is also what the token cache is keyed on. A profile missing any of
/// them is reported as not saying where to sign in — a configuration cause, so
/// the window can state it instead of offering a button that cannot work
/// (`sso-sign-in` spec).
///
/// Deliberately reads only the profile's own section and the session it names.
/// A legacy inline profile, or one reached through a `source_profile` chain,
/// is out of scope for `XONHO-0011` and lands here as a missing declaration —
/// visible, rather than silently wrong.
pub fn sign_in_location(paths: &ConfigPaths, profile: &str) -> Result<SignInLocation> {
    let missing = |detail: String| Error::MissingConfiguration {
        profile: Some(profile.to_owned()),
        detail,
    };

    let session_name = sso_session(paths, profile).ok_or_else(|| {
        missing("it declares no `sso_session`, so there is nowhere to sign in".to_owned())
    })?;

    let path = paths
        .config
        .as_deref()
        .ok_or_else(|| missing("there is no shared config file to read".to_owned()))?;
    let contents = std::fs::read_to_string(path).map_err(|source| {
        missing(format!(
            "the shared config at `{}` could not be read: {source}",
            path.display()
        ))
    })?;

    let section = section_entries(&contents, &[format!("[sso-session {session_name}]")]);
    let required = |key: &str| {
        section.get(key).cloned().ok_or_else(|| {
            missing(format!(
                "the `[sso-session {session_name}]` section declares no `{key}`"
            ))
        })
    };

    Ok(SignInLocation {
        start_url: required("sso_start_url")?,
        region: required("sso_region")?,
        // Optional, and the common configuration leaves it out. An empty list
        // means none are sent, which is what the service treats as "the
        // defaults for this instance".
        scopes: section
            .get("sso_registration_scopes")
            .map(|declared| {
                declared
                    .split(',')
                    .map(str::trim)
                    .filter(|scope| !scope.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        session_name,
    })
}

/// Every `key = value` inside the first matching section.
fn section_entries(contents: &str, headers: &[String]) -> HashMap<String, String> {
    let mut entries = HashMap::new();
    let mut inside = false;
    for line in contents.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = headers.iter().any(|header| line == header);
            continue;
        }
        if !inside || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim();
            if !value.is_empty() {
                entries.insert(key.trim().to_owned(), value.to_owned());
            }
        }
    }
    entries
}

/// `config` writes `[profile foo]`; `credentials` writes `[foo]`.
#[derive(Clone, Copy)]
enum SectionStyle {
    Config,
    Credentials,
}

/// Extract profile names from a shared-config file, ignoring anything that
/// is not a well-formed section header. A malformed line is skipped rather
/// than failing discovery: one bad entry must not hide every other profile.
fn read_section_names(path: &Path, style: SectionStyle) -> Vec<String> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    contents
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix('[')?.strip_suffix(']'))
        .filter_map(|section| {
            let section = section.trim();
            match style {
                // `[profile foo]`, plus bare `[default]` which needs no prefix.
                SectionStyle::Config => match section.strip_prefix("profile ") {
                    Some(name) => Some(name.trim()),
                    None if section == "default" => Some(section),
                    None => None,
                },
                // Every section is a profile, but `[profile foo]` here is a
                // common mistake and names the profile `foo`, not `profile foo`.
                SectionStyle::Credentials => Some(
                    section
                        .strip_prefix("profile ")
                        .map_or(section, str::trim_start),
                ),
            }
        })
        .filter(|name| !name.is_empty() && !name.contains(char::is_whitespace))
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    //! `connections` spec, "Profile discovery" — all three scenarios, over
    //! fixture files so the developer's real `~/.aws` is never read.

    use super::*;

    pub(super) struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        pub(super) fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("caixonho-profiles-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("fixture dir");
            Self { dir }
        }

        pub(super) fn write(&self, file: &str, contents: &str) -> PathBuf {
            let path = self.dir.join(file);
            std::fs::write(&path, contents).expect("write fixture");
            path
        }

        pub(super) fn missing(&self, file: &str) -> PathBuf {
            self.dir.join(file)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn named_and_default_profiles_are_listed_with_the_default_identified() {
        let fixture = Fixture::new("named-and-default");
        let config = fixture.write(
            "config",
            "[default]\nregion = ap-southeast-1\n\n\
             [profile work]\nregion = us-east-1\n\n\
             [profile personal]\nregion = eu-west-1\n",
        );

        let profiles = discover(&ConfigPaths {
            config: Some(config),
            credentials: Some(fixture.missing("credentials")),
        })
        .expect("discovery must succeed");

        let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["default", "personal", "work"]);
        assert!(profiles[0].is_default);
        assert!(profiles.iter().skip(1).all(|p| !p.is_default));
    }

    #[test]
    fn no_configuration_files_yields_an_empty_list_not_an_error() {
        let fixture = Fixture::new("no-files");

        let profiles = discover(&ConfigPaths {
            config: Some(fixture.missing("config")),
            credentials: Some(fixture.missing("credentials")),
        })
        .expect("absence of files is not a failure");

        assert!(profiles.is_empty());
    }

    #[test]
    fn a_malformed_entry_does_not_hide_the_profiles_around_it() {
        let fixture = Fixture::new("malformed");
        let config = fixture.write(
            "config",
            "[profile good]\nregion = us-east-1\n\n\
             [profile \nregion = broken\n\n\
             []\n\n\
             [profile also-good]\nregion = us-west-2\n",
        );

        let profiles = discover(&ConfigPaths {
            config: Some(config),
            credentials: Some(fixture.missing("credentials")),
        })
        .expect("a malformed entry must not fail discovery");

        let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["also-good", "good"]);
    }

    #[test]
    fn credentials_file_profiles_are_included_and_never_duplicated() {
        let fixture = Fixture::new("both-files");
        let config = fixture.write("config", "[default]\nregion = ap-southeast-1\n");
        let credentials = fixture.write(
            "credentials",
            "[default]\naws_access_key_id = AKIAEXAMPLE\n\n\
             [legacy]\naws_access_key_id = AKIAEXAMPLE2\n",
        );

        let profiles = discover(&ConfigPaths {
            config: Some(config),
            credentials: Some(credentials),
        })
        .expect("discovery must succeed");

        let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["default", "legacy"]);
    }

    #[test]
    fn discovery_reads_the_paths_named_by_the_environment() {
        // Guards the wiring the GUI depends on: without it, a machine using
        // AWS_CONFIG_FILE would silently show no profiles.
        let fixture = Fixture::new("env-paths");
        let config = fixture.write("elsewhere.conf", "[profile from-env]\nregion = us-east-1\n");

        let paths = ConfigPaths {
            config: Some(config),
            credentials: None,
        };
        let profiles = discover(&paths).expect("discovery must succeed");

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "from-env");
        assert!(!profiles[0].is_default);
    }
}

#[cfg(test)]
mod sign_in_location_tests {
    //! `sso-sign-in` spec — "A session can be obtained from within the
    //! application", and the half of it that matters most: what happens when
    //! the profile does not say where.

    use super::tests::Fixture;
    use super::*;

    /// A config with one fully declared session and one profile using it.
    fn declared(fixture: &Fixture) -> ConfigPaths {
        let config = fixture.write(
            "config",
            "[profile work]\nregion = us-east-1\nsso_session = corp\n\n\
             [sso-session corp]\n\
             sso_start_url = https://corp.awsapps.com/start\n\
             sso_region = ap-southeast-1\n\
             sso_registration_scopes = sso:account:access, sso:account:read\n",
        );
        ConfigPaths {
            config: Some(config),
            credentials: Some(fixture.missing("credentials")),
        }
    }

    #[test]
    fn a_declared_session_gives_every_field_the_flow_needs() {
        let fixture = Fixture::new("sign-in-declared");

        let at = sign_in_location(&declared(&fixture), "work").expect("the session is declared");

        assert_eq!(at.session_name, "corp");
        assert_eq!(at.start_url, "https://corp.awsapps.com/start");
        // The session's region, not the profile's `us-east-1`: an Identity
        // Center instance lives where it lives.
        assert_eq!(at.region, "ap-southeast-1");
        assert_eq!(at.scopes, ["sso:account:access", "sso:account:read"]);
    }

    #[test]
    fn scopes_are_optional_and_their_absence_is_not_a_failure() {
        let fixture = Fixture::new("sign-in-no-scopes");
        let config = fixture.write(
            "config",
            "[profile work]\nsso_session = corp\n\n\
             [sso-session corp]\n\
             sso_start_url = https://corp.awsapps.com/start\n\
             sso_region = ap-southeast-1\n",
        );

        let at = sign_in_location(
            &ConfigPaths {
                config: Some(config),
                credentials: None,
            },
            "work",
        )
        .expect("scopes are not required");

        assert!(at.scopes.is_empty());
    }

    #[test]
    fn a_profile_with_no_sso_session_says_so_rather_than_failing_obscurely() {
        // The case the window has to render as a cause instead of a button:
        // there is nowhere to sign in, and no attempt could change that.
        let fixture = Fixture::new("sign-in-none");
        let config = fixture.write("config", "[profile work]\nregion = us-east-1\n");

        let error = sign_in_location(
            &ConfigPaths {
                config: Some(config),
                credentials: None,
            },
            "work",
        )
        .expect_err("nothing declares where to sign in");

        match error {
            Error::MissingConfiguration { profile, detail } => {
                assert_eq!(profile.as_deref(), Some("work"));
                assert!(detail.contains("sso_session"), "{detail}");
            }
            other => panic!("expected a configuration cause, got {other:?}"),
        }
    }

    #[test]
    fn a_session_missing_its_start_url_names_the_field_that_is_missing() {
        // Half-declared is its own trap: the profile points at a session, so
        // the name resolves, and the flow would then have nowhere to send the
        // user. The message names the field so the fix is one line away.
        let fixture = Fixture::new("sign-in-half");
        let config = fixture.write(
            "config",
            "[profile work]\nsso_session = corp\n\n\
             [sso-session corp]\nsso_region = ap-southeast-1\n",
        );

        let error = sign_in_location(
            &ConfigPaths {
                config: Some(config),
                credentials: None,
            },
            "work",
        )
        .expect_err("the session is incomplete");

        match error {
            Error::MissingConfiguration { detail, .. } => {
                assert!(detail.contains("sso_start_url"), "{detail}");
                assert!(detail.contains("corp"), "{detail}");
            }
            other => panic!("expected a configuration cause, got {other:?}"),
        }
    }

    #[test]
    fn another_sessions_fields_are_not_borrowed() {
        // Sections are read by name. A second session in the same file must
        // not lend its start URL to the one being asked about.
        let fixture = Fixture::new("sign-in-two-sessions");
        let config = fixture.write(
            "config",
            "[profile work]\nsso_session = corp\n\n\
             [sso-session other]\n\
             sso_start_url = https://other.awsapps.com/start\n\
             sso_region = eu-west-1\n\n\
             [sso-session corp]\n\
             sso_start_url = https://corp.awsapps.com/start\n\
             sso_region = ap-southeast-1\n",
        );

        let at = sign_in_location(
            &ConfigPaths {
                config: Some(config),
                credentials: None,
            },
            "work",
        )
        .expect("the named session is found");

        assert_eq!(at.start_url, "https://corp.awsapps.com/start");
        assert_eq!(at.region, "ap-southeast-1");
    }
}
