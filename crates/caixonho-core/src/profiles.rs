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

use std::path::{Path, PathBuf};

use crate::error::Result;
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

    struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("caixonho-profiles-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("fixture dir");
            Self { dir }
        }

        fn write(&self, file: &str, contents: &str) -> PathBuf {
            let path = self.dir.join(file);
            std::fs::write(&path, contents).expect("write fixture");
            path
        }

        fn missing(&self, file: &str) -> PathBuf {
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
