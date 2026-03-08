CREATE TABLE strava_activities (
  id                 BIGINT PRIMARY KEY,
  name               TEXT NOT NULL,
  sport_type         TEXT NOT NULL,
  start_date         TIMESTAMPTZ NOT NULL,
  distance_m         FLOAT8 NOT NULL DEFAULT 0,
  moving_time_s      INTEGER NOT NULL DEFAULT 0,
  elapsed_time_s     INTEGER NOT NULL DEFAULT 0,
  elevation_gain_m   FLOAT8 NOT NULL DEFAULT 0,
  average_speed_ms   FLOAT8,
  max_speed_ms       FLOAT8,
  average_watts      FLOAT8,
  max_watts          INTEGER,
  average_heartrate  FLOAT8,
  max_heartrate      FLOAT8,
  average_cadence    FLOAT8,
  calories           FLOAT8,
  kudos_count        INTEGER NOT NULL DEFAULT 0,
  pr_count           INTEGER NOT NULL DEFAULT 0,
  trainer            BOOLEAN NOT NULL DEFAULT FALSE,
  commute            BOOLEAN NOT NULL DEFAULT FALSE,
  map_polyline       TEXT,
  synced_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_strava_start_date ON strava_activities(start_date DESC);
CREATE INDEX idx_strava_sport_type ON strava_activities(sport_type);
