---
name: security
priority: 50
---

# Security rules (from my-plugin)

- Never commit secrets. Scan staged files with `gitleaks` before any commit.
- All new HTTP endpoints must require authentication.
- Use parameterized queries only.
