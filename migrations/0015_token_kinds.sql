-- Two distinct access circuits share the `auth_tokens` table, and the UI has
-- to tell them apart: a key the user minted by hand for VS Code is not a
-- phone session. `kind` carries that distinction without a second table —
-- same hashing, same revocation, same expiry mechanics on both.
--
-- 'session' covers every token issued through a login or a device pairing
-- (the default, so existing rows keep their meaning). 'api' marks the
-- developer keys created on purpose from the settings screen.
ALTER TABLE auth_tokens ADD COLUMN kind TEXT NOT NULL DEFAULT 'session';

-- Listing a user's keys and devices is the settings screen's whole job, and
-- it filters on the owner first: one index per circuit.
CREATE INDEX IF NOT EXISTS idx_auth_tokens_user_kind
    ON auth_tokens (user_id, kind, revoked_at);
