CREATE TABLE IF NOT EXISTS media_entries (
    id              SERIAL PRIMARY KEY,
    media_type      TEXT        NOT NULL,           -- 'anime' | 'game'
    external_id     INTEGER     NOT NULL,           -- mal_id  | rawg_id
    title           TEXT        NOT NULL,
    title_original  TEXT,                           -- titre JP pour les animés
    status          TEXT        NOT NULL,           -- watching | playing | completed | plan_to_watch | plan_to_play | dropped
    score           SMALLINT    CHECK (score BETWEEN 1 AND 10),

    -- Anime
    episodes_watched INTEGER,
    episodes_total   INTEGER,

    -- Jeux
    playtime_hours   INTEGER,
    platform         TEXT,                          -- PC | PS5 | Switch | etc.

    -- Commun
    cover_url        TEXT,
    genres           TEXT[]      NOT NULL DEFAULT '{}',
    creator          TEXT,                          -- studio (anime) | developer (game)
    year             INTEGER,
    notes            TEXT,
    synced_at        TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (media_type, external_id)
);

CREATE INDEX IF NOT EXISTS idx_media_type   ON media_entries (media_type);
CREATE INDEX IF NOT EXISTS idx_media_status ON media_entries (status);
