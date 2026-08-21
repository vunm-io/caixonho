#!/usr/bin/env bash
# Fail when an accepted advisory in deny.toml has outlived its acceptance.
#
# cargo-deny 0.20.2 has no `expires` key — it takes `id` and `reason` only and
# rejects anything else — so the date is written into the reason as
# `expires YYYY-MM-DD` and enforced here. Without this, an ignore list is the
# one file nobody re-reads, and every entry in it becomes permanent by accident.
#
# An entry with no expiry fails too. "Accepted forever" is not a decision this
# policy offers, and an acceptance that cannot end is indistinguishable from
# having stopped looking.
#
# POSIX awk only: this runs on the maintainer's macOS as well as on the Linux
# CI runner, and macOS awk has neither three-argument `match` nor `{n}`
# interval expressions.
#
# Usage: scripts/check-advisory-expiry.sh [path-to-deny.toml]
set -euo pipefail

file="${1:-$(dirname "$0")/../deny.toml}"
today="$(date -u +%Y-%m-%d)"

awk -v today="$today" -v file="$file" '
  /id[ \t]*=[ \t]*"RUSTSEC-/ {
    if (!match($0, /RUSTSEC-[0-9][0-9][0-9][0-9]-[0-9][0-9][0-9][0-9]/)) next
    id = substr($0, RSTART, RLENGTH)
    seen++
    if (match($0, /expires[ \t]+[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]/)) {
      when = substr($0, RSTART, RLENGTH)
      sub(/^expires[ \t]+/, "", when)
      # ISO dates compare correctly as strings, which is the whole reason to
      # insist on that one format.
      if (when < today) {
        printf "EXPIRED   %s  (acceptance ran out %s)\n", id, when
        bad++
      } else {
        printf "ok        %s  (until %s)\n", id, when
      }
    } else {
      printf "NO EXPIRY %s  (an acceptance with no end is not one)\n", id
      bad++
    }
  }
  END {
    if (seen == 0) { print "no accepted advisories in " file; exit 0 }
    if (bad > 0) {
      printf "\n%d of %d acceptances need deciding again.\n", bad, seen
      exit 1
    }
    printf "\nall %d acceptances are still in date (today %s).\n", seen, today
  }
' "$file"
