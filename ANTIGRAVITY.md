# ANTIGRAVITY — chetaku-rs

## Deploy
- **Toujours utiliser `bash deploy.sh`**.
- La DB (Neon) doit être up car `sqlx` (si utilisé avec `!`) vérifie le schéma.
- En cas de build error : `cargo check` pour isolation.

## Database (Neon)
- Table `stats_cache` pour les agrégations coûteuses.
- TTL 30s géré par `computed_at`.
- **Jamais de cache memory** (Cloud Run scale to zero).

## Strava
- Credentials dans `.env` (`STRAVA_CLIENT_ID`, etc.).
- Ne pas oublier `scope=activity:read_all`.

## Commits
- **PAS de `Co-Authored-By`**.
- Message concis.
