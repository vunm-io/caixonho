#!/usr/bin/env bash
# Count the requirement rows in docs/requirements-status.md, per section.
#
# That file's summary line has drifted twice in one day when typed by hand,
# each time keeping the total right and getting the split wrong — which is
# exactly the kind of error review does not catch. Run this instead, and paste
# what it says.
#
# Usage: scripts/count-requirements.sh [path-to-requirements-status.md]
set -euo pipefail

file="${1:-$(dirname "$0")/../docs/requirements-status.md}"

awk -F'|' '
  /^## / { section = substr($0, 4); order[++n] = section; next }
  # A requirement row: | text | state | notes |
  section && /^\|/ && $2 !~ /^-+$/ && $2 !~ /^ *Requirement *$/ {
    state = $3
    gsub(/^ +| +$/, "", state)
    if (state == "done" || state == "partial" || state == "none") {
      count[section, state]++
      if (section ~ /^§4\.[123]/) m1[state]++
    }
  }
  END {
    for (i = 1; i <= n; i++) {
      s = order[i]
      d = count[s, "done"] + 0; p = count[s, "partial"] + 0; z = count[s, "none"] + 0
      if (d + p + z == 0) continue
      printf "%s: %d done, %d partial, %d not started (= %d)\n", s, d, p, z, d + p + z
    }
    d = m1["done"] + 0; p = m1["partial"] + 0; z = m1["none"] + 0
    printf "\nM1 (§4.1-§4.3): %d done, %d partial, %d not started (= %d)\n", d, p, z, d + p + z
  }
' "$file"
