-- dossiers : détail d'une tâche et sous-cases (volet latéral de chet-admin). Idempotent, joué à la main (psql) comme 009.
ALTER TABLE dossiers.task ADD COLUMN IF NOT EXISTS detail TEXT NOT NULL DEFAULT '';

CREATE TABLE IF NOT EXISTS dossiers.step (
    key         TEXT PRIMARY KEY,           -- « <clé de tâche>.<n> », n = rang dans la tâche, jamais renuméroté
    task_key    TEXT NOT NULL REFERENCES dossiers.task(key) ON DELETE CASCADE,
    position    INT  NOT NULL,
    text        TEXT NOT NULL,
    done        BOOLEAN NOT NULL DEFAULT FALSE,
    done_at     TIMESTAMPTZ,
    done_by     TEXT,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_dossiers_step_task ON dossiers.step(task_key, position);
