-- Sessions Claude Code actives, une ligne par machine : l'état est REMPLACÉ à chaque envoi (pas d'historique).
CREATE TABLE IF NOT EXISTS live_sessions (
    machine      TEXT        PRIMARY KEY,                -- "djustyin"
    sessions     JSONB       NOT NULL DEFAULT '[]',      -- [{session_id, name, kind, status, cwd, pid, started_at}]
    reported_at  TIMESTAMPTZ NOT NULL,                   -- horloge de la machine qui envoie
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()      -- horloge du serveur : sert à juger la fraîcheur
);
