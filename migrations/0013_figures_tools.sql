-- Les outils qu'une figure a le droit d'appeler.
--
-- Une liste JSON de noms d'outils (`generate_image`, `generate_speech`,
-- `read_file`, `mcp__serveur__outil`…). Vide ou NULL : tout ce que
-- l'application propose — c'est le comportement de toutes les figures
-- écrites avant ce champ.
ALTER TABLE figures ADD COLUMN tools TEXT;
