-- Les documents qu'un projet donne à lire au modèle.
--
-- Un texte long ne se donne pas d'un bloc : on le découpe, on note ce que
-- chaque morceau « veut dire » sous forme de vecteur, et à chaque question on
-- ne remonte que les morceaux proches. Sans cela, ou bien on envoie tout — ce
-- que la fenêtre de contexte ne supporte pas — ou bien on n'envoie rien.
--
-- Le vecteur est rangé en binaire : une suite de `f32` en petit-boutien. SQLite
-- ne sait pas comparer des vecteurs, donc la comparaison se fait en Rust, sur
-- les morceaux du projet concerné. C'est linéaire, et c'est très bien à cette
-- échelle : quelques milliers de morceaux se parcourent en une poignée de
-- millisecondes, et un index vectoriel dédié serait une dépendance de plus
-- pour un gain que personne ne remarquerait.
CREATE TABLE IF NOT EXISTS rag_chunks (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL,
    -- D'où vient ce morceau : nom de fichier, titre, adresse. C'est ce qui
    -- s'affiche à côté d'une réponse, et ce qui permet d'effacer un document
    -- sans toucher aux autres.
    source      TEXT NOT NULL,
    -- Le rang du morceau dans son document, pour les relire dans l'ordre.
    ordinal     INTEGER NOT NULL DEFAULT 0,
    text        TEXT NOT NULL,
    embedding   BLOB NOT NULL,
    -- La taille du vecteur et le modèle qui l'a produit. Changer de modèle
    -- change la géométrie : comparer un vecteur d'un modèle à celui d'un autre
    -- ne donne pas un résultat faux, il donne un résultat qui n'a aucun sens.
    dim         INTEGER NOT NULL,
    embed_model TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_rag_chunks_project ON rag_chunks(project_id);
CREATE INDEX IF NOT EXISTS idx_rag_chunks_source ON rag_chunks(project_id, source);
