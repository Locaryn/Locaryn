---
name: refactor
description: Refactor code — extract module, rename, inline.
allowed_tools: [read_file, write_file, search]
arguments: ["operation", "target"]
---

Perform the `$1` refactor on `$2`.

- `extract-module`: read the file, identify the construct, create a new module, update imports.
- `rename`: find references with `search`, rename consistently, verify no stale refs.
- `inline`: inline the target at all call sites and remove the original.

Always show a diff and ask for approval before writing.
