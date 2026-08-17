-- Les figures : un rôle, ses consignes, ses conversations.
--
-- Elles vivent sur le serveur et pas dans un fichier de l'extension : une
-- figure écrite sur l'ordinateur doit s'ouvrir sur le téléphone, et survivre
-- au retrait puis à la réinstallation de l'extension qui l'affiche.
--
-- `source` distingue celles qui viennent d'un dépôt d'extension de celles
-- écrites à la main : réinstaller une extension met à jour les premières sans
-- toucher aux secondes.
CREATE TABLE IF NOT EXISTS figures (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    description  TEXT NOT NULL DEFAULT '',
    instructions TEXT NOT NULL,
    model        TEXT,
    opening      TEXT,
    -- Vrai quand la figure lit la mémoire de l'utilisateur.
    uses_memory  INTEGER NOT NULL DEFAULT 0,
    source       TEXT NOT NULL DEFAULT 'user',
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

-- Deux figures du même nom seraient impossibles à distinguer dans une liste.
CREATE UNIQUE INDEX IF NOT EXISTS idx_figures_name ON figures (lower(name));

-- La conversation sait quelle figure la tient : c'est ce qui permet de
-- regrouper l'historique par figure, et de réinjecter ses consignes à chaque
-- tour sans les recopier dans chaque message.
ALTER TABLE sessions ADD COLUMN figure_id TEXT;
