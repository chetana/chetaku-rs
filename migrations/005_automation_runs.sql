CREATE TABLE IF NOT EXISTS automation_runs (
    id           BIGSERIAL   PRIMARY KEY,
    run_id       TEXT        NOT NULL UNIQUE,        -- idempotence : "<automatisation>-<début ISO>"
    automation   TEXT        NOT NULL,               -- "mr-review-watch"
    started_at   TIMESTAMPTZ NOT NULL,
    ended_at     TIMESTAMPTZ,
    status       TEXT        NOT NULL CHECK (status IN ('ok', 'noop', 'alert', 'error')),
    headline     TEXT        NOT NULL DEFAULT '',    -- une ligne
    metrics      JSONB       NOT NULL DEFAULT '{}',  -- compteurs
    items        JSONB       NOT NULL DEFAULT '[]',  -- [{kind, ref, url, title, action}]
    impact       JSONB       NOT NULL DEFAULT '{}',  -- résumé d'impact djust-sales
    report_key   TEXT,                               -- clé S3 du rapport markdown
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_automation_runs_started    ON automation_runs(started_at DESC);
CREATE INDEX IF NOT EXISTS idx_automation_runs_automation ON automation_runs(automation, started_at DESC);
