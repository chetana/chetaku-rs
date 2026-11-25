-- dossiers : suivi administratif personnel (chet-admin). Schéma dédié, aucun accès aux tables existantes.
-- Idempotent (rejouable). Joué à la main (psql), comme 005 et 006 : ne pas passer par RUN_MIGRATIONS.
CREATE SCHEMA IF NOT EXISTS dossiers;

CREATE TABLE IF NOT EXISTS dossiers.subject (
    id        TEXT PRIMARY KEY,
    position  INT  NOT NULL,
    title     TEXT NOT NULL,
    status    TEXT NOT NULL DEFAULT '',
    locked    BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE TABLE IF NOT EXISTS dossiers.task (
    key         TEXT PRIMARY KEY,           -- « <sujet>-<n> », n = rang dans le SUIVI.md, jamais renuméroté
    subject_id  TEXT NOT NULL REFERENCES dossiers.subject(id) ON DELETE CASCADE,
    position    INT  NOT NULL,
    text        TEXT NOT NULL,
    done        BOOLEAN NOT NULL DEFAULT FALSE,
    done_at     TIMESTAMPTZ,
    done_by     TEXT,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_dossiers_task_subject ON dossiers.task(subject_id, position);

CREATE TABLE IF NOT EXISTS dossiers.deadline (
    id      SERIAL PRIMARY KEY,
    due     DATE NOT NULL,
    subject TEXT NOT NULL,
    action  TEXT NOT NULL
);
