-- Remember which ecosystem an extension was installed from.
--
-- A foreign bundle (a Claude Code plugin, a Gemini CLI extension, an OpenCode
-- config) is converted to Lochor's layout at install time, so nothing on disk
-- says where it came from afterwards. The UI needs that provenance to group the
-- installed list and to explain why part of a bundle was skipped.
--
-- Rows written before this migration are Lochor-native by definition.
ALTER TABLE extensions ADD COLUMN ecosystem TEXT NOT NULL DEFAULT 'lochor';

CREATE INDEX IF NOT EXISTS idx_extensions_ecosystem ON extensions(ecosystem);
