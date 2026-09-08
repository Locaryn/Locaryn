-- Le contexte d'un projet : ce qu'il faut savoir pour y travailler, et qui a
-- le droit de le savoir.
--
-- Volontairement sans domaine. Une fiche peut dire « le rendu final est en A2
-- sur papier grain torchon », « les mesures se font à 20 °C, sinon la
-- dilatation faussse tout », « le client refuse le violet », ou « les tests
-- passent par cargo test ». Un projet d'art, un dossier de physique, un mémoire
-- ou du code posent les mêmes questions à qui arrive dessus : qu'est-ce qui a
-- déjà été décidé, et pourquoi. Rien ici ne suppose du code.
--
-- Trois portées, et c'est la seule chose qui distingue une fiche d'une autre :
--
--   'machine' — ne quitte jamais cet ordinateur. Ce qui n'a de sens qu'ici :
--               les outils installés, un chemin local, une version de
--               logiciel. Inutile à un collègue, et faux sur un autre poste.
--
--   'compte'  — suit la personne entre ses appareils. Ses exigences à elle :
--               « je veux qu'on me relise avant d'envoyer », « je travaille en
--               anglais ». Deux ordinateurs, le même compte, la même fiche.
--
--   'partage' — visible de tous ceux qui travaillent sur ce projet, sur ce
--               serveur. Ce que le projet a décidé, pas ce qu'une personne
--               préfère.
--
-- Les deux dernières exigent le mode serveur : sans lui, il n'y a ni compte à
-- suivre ni personne avec qui partager. L'application le dit plutôt que
-- d'écrire une fiche « partagée » que personne ne verra jamais.

CREATE TABLE IF NOT EXISTS project_context (
    id          TEXT PRIMARY KEY NOT NULL,
    project_id  TEXT NOT NULL,
    -- 'machine' | 'compte' | 'partage'
    scope       TEXT NOT NULL DEFAULT 'machine',
    -- Qui a posé la fiche. Renseigné en mode serveur, où plusieurs personnes
    -- écrivent : une décision partagée sans auteur ne se discute pas.
    author      TEXT,
    -- Nom court de la fiche. Clé d'unicité avec la portée : une deuxième fiche
    -- du même titre s'y ajoute plutôt que de la doubler.
    title       TEXT NOT NULL,
    -- Une ligne, montrée dans la liste sans ouvrir la fiche.
    summary     TEXT NOT NULL DEFAULT '',
    -- Tableau JSON de phrases. S'accumule : ce qu'on apprend s'ajoute, il ne
    -- remplace pas ce qui était su.
    details     TEXT NOT NULL DEFAULT '[]',
    -- 'utilisateur' ou 'assistant' : qui a écrit le dernier détail.
    source      TEXT NOT NULL DEFAULT 'utilisateur',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- Le contexte est lu à l'ouverture d'un projet et avant les tours qui en ont
-- besoin : la lecture par projet et par portée doit être immédiate.
CREATE INDEX IF NOT EXISTS idx_project_context_lookup
    ON project_context (project_id, scope, updated_at DESC);

-- Une fiche par (projet, portée, titre). La même décision peut donc exister en
-- 'partage' et en 'compte' sans se marcher dessus : le projet dit une chose, et
-- quelqu'un peut en vouloir une autre pour lui.
CREATE UNIQUE INDEX IF NOT EXISTS idx_project_context_unique
    ON project_context (project_id, scope, lower(title));
