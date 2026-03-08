CREATE TABLE IF NOT EXISTS voyages (
    id              SERIAL      PRIMARY KEY,
    title           TEXT        NOT NULL,            -- "Cambodge mars 2024"
    country_code    CHAR(2)     NOT NULL,            -- ISO 3166-1 alpha-2 "KH"
    country_name    TEXT        NOT NULL,            -- "Cambodge"
    continent       TEXT        NOT NULL,            -- "Asie" | "Europe" | "Amérique du Nord" | ...
    date_start      DATE        NOT NULL,
    date_end        DATE        NOT NULL,
    lat             FLOAT8      NOT NULL,            -- centroïde destination
    lng             FLOAT8      NOT NULL,
    distance_km     INTEGER     NOT NULL DEFAULT 0,  -- estimation aller-retour
    cover_gcs_path  TEXT,                            -- "voyages/cambodge-2024/cover.jpg"
    notes           TEXT,                            -- anecdote (markdown)
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_voyages_date    ON voyages(date_start DESC);
CREATE INDEX IF NOT EXISTS idx_voyages_country ON voyages(country_code);
