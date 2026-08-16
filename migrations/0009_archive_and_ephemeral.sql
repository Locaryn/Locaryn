-- Archiver plutôt que supprimer, et pouvoir ne rien garder du tout.
--
-- Une conversation retirée d'une liste n'est presque jamais une conversation
-- dont on veut se débarrasser : c'est une conversation qui encombre. Elle part
-- donc aux archives, où elle reste consultable, et la suppression devient un
-- geste séparé et explicite.
--
-- À l'opposé, une conversation éphémère ne doit rien laisser : elle est
-- marquée dès sa création pour que rien, pas même un titre, ne survive à sa
-- fermeture.
ALTER TABLE sessions ADD COLUMN archived_at TEXT;
ALTER TABLE sessions ADD COLUMN ephemeral INTEGER NOT NULL DEFAULT 0;
