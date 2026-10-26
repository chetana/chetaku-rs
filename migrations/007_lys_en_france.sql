-- lys-en-france : schéma dédié, sans aucun accès aux tables existantes. Idempotent (rejouable).
-- Joué à la main (psql), comme 005 et 006 : ne pas passer par RUN_MIGRATIONS.
CREATE SCHEMA IF NOT EXISTS lys_fr;

CREATE TABLE IF NOT EXISTS lys_fr.category (
    slug       TEXT PRIMARY KEY,
    position   INT  NOT NULL,
    title_fr   TEXT NOT NULL,
    title_km   TEXT
);

CREATE TABLE IF NOT EXISTS lys_fr.task (
    slug            TEXT PRIMARY KEY,
    category_slug   TEXT NOT NULL REFERENCES lys_fr.category(slug) ON DELETE RESTRICT,
    position        INT  NOT NULL,
    title_fr        TEXT NOT NULL,
    title_km        TEXT,
    help_fr         TEXT,
    help_km         TEXT,
    due_date        DATE,                -- échéance souhaitée (seed)
    due_override    DATE,                -- échéance modifiée par l'utilisateur
    legal_deadline  DATE,                -- échéance réglementaire, saisie à la main
    status          TEXT NOT NULL DEFAULT 'todo' CHECK (status IN ('todo', 'doing', 'done', 'blocked')),
    note            TEXT NOT NULL DEFAULT '' CHECK (char_length(note) <= 2000),
    done_at         TIMESTAMPTZ,
    km_status       TEXT NOT NULL DEFAULT 'none' CHECK (km_status IN ('none', 'draft', 'reviewed')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_lys_task_category ON lys_fr.task(category_slug, position);

CREATE TABLE IF NOT EXISTS lys_fr.item (
    slug        TEXT PRIMARY KEY,
    task_slug   TEXT NOT NULL REFERENCES lys_fr.task(slug) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('step', 'document')),
    position    INT  NOT NULL,
    label_fr    TEXT NOT NULL,
    label_km    TEXT,
    done        BOOLEAN NOT NULL DEFAULT FALSE,
    done_at     TIMESTAMPTZ,
    km_status   TEXT NOT NULL DEFAULT 'none' CHECK (km_status IN ('none', 'draft', 'reviewed'))
);
CREATE INDEX IF NOT EXISTS idx_lys_item_task ON lys_fr.item(task_slug, position);

CREATE TABLE IF NOT EXISTS lys_fr.document (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_slug     TEXT NOT NULL REFERENCES lys_fr.task(slug) ON DELETE RESTRICT,
    item_slug     TEXT REFERENCES lys_fr.item(slug) ON DELETE SET NULL,
    s3_key        TEXT NOT NULL UNIQUE,
    filename      TEXT NOT NULL,
    content_type  TEXT NOT NULL,
    size_bytes    BIGINT NOT NULL CHECK (size_bytes > 0),
    uploaded_by   TEXT NOT NULL,         -- `sub` Logto : traçabilité, pas un contrôle d'accès
    uploaded_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at    TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_lys_document_task ON lys_fr.document(task_slug) WHERE deleted_at IS NULL;
