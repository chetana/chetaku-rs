-- lys-en-france T3 : rendez-vous et relecture khmère. Idempotent (rejouable), ne touche qu'au schéma lys_fr.
-- Joué à la main (psql), comme 007 : ne pas passer par RUN_MIGRATIONS.
CREATE TABLE IF NOT EXISTS lys_fr.appointment (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_slug     TEXT NOT NULL REFERENCES lys_fr.task(slug) ON DELETE RESTRICT,
    starts_at     TIMESTAMPTZ NOT NULL,
    place         TEXT,
    note          TEXT CHECK (char_length(note) <= 500),
    created_by    TEXT NOT NULL,         -- `sub` Logto : traçabilité, pas un contrôle d'accès
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    cancelled_at  TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_lys_appointment_task ON lys_fr.appointment(task_slug, starts_at) WHERE cancelled_at IS NULL;

ALTER TABLE lys_fr.task ADD COLUMN IF NOT EXISTS km_reviewed_by TEXT;
ALTER TABLE lys_fr.task ADD COLUMN IF NOT EXISTS km_reviewed_at TIMESTAMPTZ;
