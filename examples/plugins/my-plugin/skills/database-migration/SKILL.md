---
name: database-migration
description: When the user wants to run or create SQL migrations.
version: 1.0.0
auto_trigger: true
allowed_tools: [read_file, write_file, run_command, search]
---

# Instructions

Analyze the schema changes, check the `migrations/` directory, and verify
SQL syntax before executing against the target database.

## Steps

1. Read the existing `migrations/` directory to find the latest applied migration.
2. Compare with the requested change.
3. Generate a new migration file named `NNNN_description.sql`.
4. Validate SQL syntax.
5. Ask the user before executing against a non-dev database.
