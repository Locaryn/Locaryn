-- Mémoire de l'utilisateur : ce que Locaryn retient d'une conversation à
-- l'autre — préférences, habitudes, projets en cours, faits utiles.
--
-- Elle vit dans la base du service, et non dans un fichier de l'application,
-- pour trois raisons : le téléphone et le bureau parlent au même service et
-- doivent voir la même mémoire ; sur un serveur partagé, chaque compte a la
-- sienne ; et une mémoire consultable est une mémoire qu'on peut corriger.
--
-- `user_id` est nul sur une installation personnelle, où il n'y a pas de
-- comptes : la mémoire appartient alors à la machine.
CREATE TABLE IF NOT EXISTS memory_entries (
    id          TEXT PRIMARY KEY NOT NULL,
    user_id     TEXT,
    -- 'preference' | 'habitude' | 'projet' | 'fait'
    category    TEXT NOT NULL DEFAULT 'fait',
    content     TEXT NOT NULL,
    -- Qui l'a écrite : 'utilisateur' quand la personne l'a saisie,
    -- 'assistant' quand le modèle l'a retenue. Ce qui vient du modèle se
    -- relit d'un autre œil.
    source      TEXT NOT NULL DEFAULT 'utilisateur',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- La mémoire est lue à chaque message : la lecture par compte doit être
-- immédiate.
CREATE INDEX IF NOT EXISTS idx_memory_user ON memory_entries (user_id, updated_at DESC);

-- Deux fois la même phrase n'apporte rien et dilue le contexte envoyé au
-- modèle. L'index couvre le cas sans compte (`user_id` nul) via COALESCE.
CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_unique
    ON memory_entries (COALESCE(user_id, ''), lower(content));
