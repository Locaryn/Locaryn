-- La mémoire change de forme : d'une liste de phrases isolées à des fiches
-- qui grossissent — un titre court, un résumé d'une ligne, des détails qui
-- s'accumulent au fil des conversations, rangés dans l'un de quatre groupes
-- fixes (vous, sujets, zones, personnes). C'est ce qui permet à l'écran des
-- réglages de montrer une liste compacte qui s'ouvre sur le détail, plutôt
-- qu'un document qui grossit sans repère.
--
-- L'ancienne mémoire n'est pas perdue : chaque phrase déjà retenue devient
-- une fiche à elle seule, avec un groupe deviné depuis son ancienne
-- catégorie. Personne n'a besoin de la réapprendre.

CREATE TABLE memory_entries_v2 (
    id          TEXT PRIMARY KEY NOT NULL,
    user_id     TEXT,
    -- 'vous' | 'sujets' | 'zones' | 'personnes' — les quatre groupes fixes de
    -- l'écran des réglages.
    group_name  TEXT NOT NULL DEFAULT 'sujets',
    -- Nom court de la fiche ('Bot Bastet', 'Préférences'). C'est la clé
    -- d'unicité par groupe : deux fiches du même groupe et du même titre
    -- fusionnent plutôt que de se doubler.
    title       TEXT NOT NULL,
    -- Une ligne, montrée dans la liste sans qu'on ouvre la fiche.
    summary     TEXT NOT NULL DEFAULT '',
    -- Tableau JSON de phrases, montré en entier une fois la fiche ouverte.
    -- S'accumule : une conversation qui apprend un nouveau détail sur une
    -- fiche existante l'ajoute, elle ne réécrit pas les précédents.
    details     TEXT NOT NULL DEFAULT '[]',
    -- Qui a écrit le dernier détail : 'utilisateur' ou 'assistant'.
    source      TEXT NOT NULL DEFAULT 'utilisateur',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Migration des fiches existantes : chaque ancienne phrase devient une fiche
-- à elle seule. Le titre est tronqué depuis le contenu — sans passage par un
-- modèle, il n'y a pas de nom plus court à en tirer honnêtement, et une
-- fiche qui garde toute la phrase en résumé reste lisible.
INSERT INTO memory_entries_v2 (id, user_id, group_name, title, summary, details, source, created_at, updated_at)
SELECT
    id,
    user_id,
    CASE category
        WHEN 'preference' THEN 'vous'
        WHEN 'projet'     THEN 'zones'
        ELSE                   'sujets'
    END,
    CASE WHEN length(content) > 48 THEN substr(content, 1, 47) || '…' ELSE content END,
    content,
    json_array(content),
    source,
    created_at,
    updated_at
FROM memory_entries;

DROP TABLE memory_entries;
ALTER TABLE memory_entries_v2 RENAME TO memory_entries;

-- La mémoire est lue à chaque message : la lecture par compte doit être
-- immédiate.
CREATE INDEX IF NOT EXISTS idx_memory_user ON memory_entries (user_id, group_name, updated_at DESC);

-- Une fiche par (compte, groupe, titre) : la retrouver pour lui ajouter un
-- détail, plutôt que d'en créer une seconde qui dirait la même chose.
CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_unique
    ON memory_entries (COALESCE(user_id, ''), group_name, lower(title));
