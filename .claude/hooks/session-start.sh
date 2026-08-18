#!/bin/bash
# SessionStart hook — Claude Code on the web only.
# Installs the OpenSpec CLI, which the repo's /opsx:* commands and
# openspec-* skills shell out to (see openspec/ and .claude/commands/opsx/).
set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

if ! command -v openspec >/dev/null 2>&1; then
  npm install -g @fission-ai/openspec
fi

openspec --version
