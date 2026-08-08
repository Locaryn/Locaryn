#!/usr/bin/env bash
# Example PreToolUse hook: refuse to write files under .env paths.
set -euo pipefail

# The hook receives the tool args on stdin as JSON in V1; this skeleton
# just guards a known-bad pattern.
if [[ "${LOCHOR_TOOL_ARGS:-}" == *".env"* ]]; then
  echo "refusing to write to a .env file" >&2
  exit 1
fi
exit 0
