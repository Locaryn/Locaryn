---
name: security
priority: 50
---

# Security rules for this project

- Never commit secrets. Scan staged files with `gitleaks` before any commit.
- All new HTTP endpoints must require authentication.
- Use parameterized queries only — never string-concatenate SQL.
- Validate and sanitize all inputs at the boundary.
- Log security-relevant events to the audit log.
