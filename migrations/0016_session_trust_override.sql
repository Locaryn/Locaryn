-- Les permissions d'une conversation venaient du projet qui la porte :
-- chaque projet porte un `trust_level`, les conversations en héritaient sans
-- recours. Or la personne parle à un modèle, pas à un projet — et c'est dans
-- la conversation qu'elle décide de ce qu'elle lui accorde : demander avant
-- d'écrire, tout autoriser, ou la tenir en simple aperçu.
--
-- `trust_override` porte ce choix propre à la conversation. NULL — le cas de
-- toutes les lignes existantes — veut dire « hériter du projet », de sorte
-- que rien ne change pour personne tant que la personne n'a rien changé.
ALTER TABLE sessions ADD COLUMN trust_override TEXT NULL;
