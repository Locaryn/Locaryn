-- Noyaux alternatifs : une conversation peut être confiée à un noyau
-- installé (OpenClaw, Hermes Agent…) au lieu du noyau Locaryn.
-- NULL = noyau Locaryn natif (comportement historique inchangé).
ALTER TABLE sessions ADD COLUMN core_id TEXT;
