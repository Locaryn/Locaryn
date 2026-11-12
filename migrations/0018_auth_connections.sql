-- Chaque connexion d'un compte, pour l'écran « Appareils et sécurité » : quand,
-- d'où, avec quel client. Une ligne à l'ouverture d'une session (mot de passe,
-- code temporaire, appairage), puis une à chaque fois qu'un appareil déjà
-- connecté revient depuis une autre adresse — c'est ce qui trahit un jeton
-- volé, et ce que les pages « appareils » des grands services montrent.
CREATE TABLE IF NOT EXISTS auth_connections (
    id          TEXT PRIMARY KEY NOT NULL,
    user_id     TEXT NOT NULL,
    token_id    TEXT,
    at          TEXT NOT NULL,
    ip          TEXT,
    client      TEXT NOT NULL DEFAULT 'app',
    user_agent  TEXT,
    method      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_auth_connections_user_at
    ON auth_connections (user_id, at);
CREATE INDEX IF NOT EXISTS idx_auth_connections_token_at
    ON auth_connections (token_id, at);
