# Cutting a release

There is no release workflow. This is the process, written down because it has
been run three times from memory and gone wrong twice: `v0.1.0-beta.1` shipped
its two assets from two different commits, and both it and `v0.1.0-beta.2`
shipped a macOS asset that could not be opened at all.

## Before anything

1. **Bump the version.** `[workspace.package] version` in the root
   `Cargo.toml` is the single source (`XONHO-0033`): the window's status bar,
   the macOS `Info.plist`, and the names CI gives its artifacts all derive from
   it. Nothing else states a version, and nothing else should be edited to say
   one.
2. Run `cargo metadata --format-version 1 --no-deps` or build once, so
   `Cargo.lock` records the new version too. `cargo pkgid` reads the lock, and
   CI names the artifacts from `cargo pkgid` — a stale lock names the files
   after the old release.
3. Write `docs/releases/vX.Y.Z-tag.md`. Say what changed, and say what is *not*
   in it: the notes of both betas describe absences at length and that is
   deliberate.
4. Commit both together. The tag then names a commit whose declared version
   matches it.

**The title is English and says what a reader gains.** Look at the previous
two or three before writing one — `gh release view <tag> --json name` — because
the convention lives in them: *"directory buckets, Local Zones, and a queue"*,
*"acting on a row, and on more than one"*, *"the macOS build can be opened"*.
`v0.1.0-beta.4` shipped as *"Đất Nặn, and a window that says what it is"* and
was corrected within the hour: the design system's own name is a proper noun
and belongs in the body, where a sentence can explain it, not in the one string
a stranger reads first.

## Getting the assets

**Take what CI built. Do not rename anything.** The files already carry their
version — `caixonho-<version>-macos-arm64.zip` and
`caixonho-<version>-windows-x86_64.exe` — and the hand-rename that used to sit
here is where a mislabelled asset would come from.

```
gh run download <run-id> -D <dir>          # a green run on main
```

Then check what you are about to publish, on the files themselves:

```
file <dir>/caixonho-*-windows-x86_64.exe   # PE32+ executable (GUI) x86-64
unzip -q <dir>/caixonho-*-macos-arm64.zip -d x
file x/Caixonho.app/Contents/MacOS/caixonho-gui   # Mach-O 64-bit executable arm64
codesign --verify --deep --strict x/Caixonho.app  # valid on disk
shasum -a 256 <dir>/caixonho-*
```

`codesign --verify` is not optional and not a formality. An unsealed bundle
verifies as *"code has no resources but signature indicates they must be
present"*, which is what Gatekeeper calls **damaged** — and what two published
releases shipped.

## Publishing

```
git tag -a vX.Y.Z-tag -m "..."
git push origin main vX.Y.Z-tag
gh release create vX.Y.Z-tag <dir>/caixonho-* \
  --title "..." --prerelease --notes-file docs/releases/vX.Y.Z-tag.md
```

Then **download what you published and check it again**:

```
gh release download vX.Y.Z-tag -D verify && shasum -a 256 verify/*
```

The checksums in the notes must match the files that are actually being served.

## Afterwards, on a Mac

Download the macOS asset **with a browser** — so it carries the quarantine
mark a real user's copy carries — and open it. Read `syspolicyd` in the unified
log if it is refused:

```
log show --predicate 'subsystem == "com.apple.syspolicy"' --last 10m --style compact | grep -i caixonho
```

Write in the notes the dialog you actually saw. Three sets of release notes
described a dialog nobody had opened the asset to check, and all three were
wrong about it.
