//! What revision this build came from (`XONHO-0033`).
//!
//! The declared version alone cannot identify a build. `main` moves between
//! releases, so every build made after `v0.1.0-beta.3` would claim to be it —
//! which is the confusion this change exists to end rather than to reproduce
//! one level down.
//!
//! **Read here rather than passed in by CI.** A CI variable would leave every
//! build made on a developer's own machine unable to say what it is, and that
//! is precisely the case that has already cost this project a session: a
//! window on screen that was not the code just changed, with nothing to say
//! so. A build script treats the runner and the laptop the same.

use std::path::Path;
use std::process::Command;

fn main() {
    let commit = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=CAIXONHO_COMMIT={commit}");

    // Rerun when the checkout moves, and **only** then: without this the
    // script runs on every build, and with a path that does not exist it also
    // runs on every build, because Cargo treats a missing watched file as
    // changed. So each path is checked before it is watched.
    println!("cargo:rerun-if-changed=build.rs");
    if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        watch(Path::new(&git_dir).join("HEAD"));
        // The ref `HEAD` points at, when it points at one — a detached HEAD
        // has none, and a packed ref has no file of its own. Both are why this
        // is conditional rather than assumed.
        if let Some(head_ref) = git(&["symbolic-ref", "-q", "HEAD"]) {
            watch(Path::new(&git_dir).join(head_ref));
        }
    }
}

/// Ask `git`, and treat every way it can fail as the same answer: nothing.
///
/// No `git` on the machine, not a repository, a repository it refuses to read
/// — none of these is an error for this build. A source tree without history
/// is a thing people build, and refusing to compile in one would be this
/// script deciding something it has no business deciding.
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    // Empty is not a revision. It would render as `caixonho 0.1.0-beta.3 ()`,
    // which reads as a missing field rather than an unknown one.
    (!value.is_empty()).then_some(value)
}

fn watch(path: std::path::PathBuf) {
    if path.exists() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
