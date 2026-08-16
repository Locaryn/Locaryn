-- Vitesses mesurées, machine par machine.
--
-- Les chiffres annoncés par un catalogue valent pour le matériel de celui qui
-- les a publiés. Ce qui compte pour choisir un modèle, c'est ce qu'il donne
-- ici : combien de jetons par seconde sur ce processeur, combien de temps pour
-- une image de cette taille sur cette carte. Ces mesures viennent donc des
-- exécutions réelles, jamais d'une fiche.
--
-- On garde une moyenne courante plutôt que chaque exécution : la table reste
-- petite quel que soit l'usage, et une moyenne sur des dizaines de générations
-- dit plus qu'un relevé isolé. `samples` permet de savoir combien pèse la
-- moyenne — une seule mesure ne vaut pas dix.
CREATE TABLE IF NOT EXISTS model_metrics (
    -- Nom du fichier ou du modèle, tel que l'utilisateur le voit.
    model       TEXT NOT NULL,
    -- 'chat' | 'image' | 'audio'
    kind        TEXT NOT NULL,
    samples     INTEGER NOT NULL DEFAULT 0,
    -- Conversation : jetons produits par seconde.
    avg_tokens_per_second REAL,
    -- Image et audio : durée d'une génération, en millisecondes.
    avg_duration_ms REAL,
    -- Dernière mesure retenue, pour distinguer une moyenne fraîche d'une
    -- moyenne qui date d'un autre matériel.
    last_measured_at TEXT NOT NULL,
    PRIMARY KEY (model, kind)
);
