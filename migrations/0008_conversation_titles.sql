-- Le titre d'une conversation, et qui a le droit d'y toucher.
--
-- Un modèle de micro-tâche nomme les conversations pour qu'une liste se lise.
-- Mais un titre écrit à la main est une décision : le modèle ne doit jamais le
-- remplacer, sinon la personne cherche dans sa propre liste un nom qu'elle
-- avait choisi et qui a disparu.
ALTER TABLE sessions ADD COLUMN title_locked INTEGER NOT NULL DEFAULT 0;
