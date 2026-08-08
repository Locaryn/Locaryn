-- Turn the existing account skeleton into something that can actually
-- authenticate someone.
--
-- `users` and `auth_tokens` were created in 0001 but had no way to store a
-- credential: no password column at all. They are extended in place rather
-- than recreated, so a database already holding sessions and projects is not
-- disturbed.

-- Argon2id encoding, salt included. Empty for accounts created before this
-- migration, which therefore cannot log in until a password is set — the
-- correct default, since they never had one.
ALTER TABLE users ADD COLUMN password_hash TEXT NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';

-- First characters of a token, so a user can recognise one of their sessions
-- in a list without the secret itself being recoverable.
ALTER TABLE auth_tokens ADD COLUMN hint TEXT NOT NULL DEFAULT '';
ALTER TABLE auth_tokens ADD COLUMN last_used_at TEXT;

-- 0001 made `username` UNIQUE, which is case-sensitive in SQLite: "Marie" and
-- "marie" would be two accounts, and a lookalike name is an impersonation
-- route. This index makes the collision impossible.
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username_ci
    ON users (lower(username));

-- Verifying a bearer token means Argon2-checking the live candidates, so the
-- set has to stay small.
CREATE INDEX IF NOT EXISTS idx_auth_tokens_live
    ON auth_tokens (revoked_at, expires_at);
