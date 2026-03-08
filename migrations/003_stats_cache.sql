CREATE TABLE IF NOT EXISTS stats_cache (
    key         TEXT PRIMARY KEY,
    value       JSONB NOT NULL,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
