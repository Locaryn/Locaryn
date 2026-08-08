---
name: refactor
description: Refactor code — extract module, rename, inline.
allowed_tools: [read_file, write_file, search]
arguments: ["operation", "target"]
---

Perform the `$1` refactor on `$2`.

- For `extract-module`: read the file, identify the construct to extract,
  create a new module file, move the construct, update imports.
- For `rename`: find all references with `search`, rename consistently,
  verify no stale references remain.
- For `inline`: inline the target symbol at all call sites and remove the
  original definition.

Always show a diff and ask for approval before writing.
