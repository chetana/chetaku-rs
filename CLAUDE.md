# chetaku-rs — Instructions Claude

## Deploy

**Toujours utiliser `deploy.sh` pour déployer**, jamais `gcloud run deploy` directement :

```bash
bash deploy.sh
```

Ce script :
1. Déploie sur Cloud Run (`gcloud run deploy --source .`)
2. Restaure immédiatement toutes les env vars depuis `.env`

⚠️ `gcloud run deploy --source .` efface les env vars à chaque deploy — `deploy.sh` les remet automatiquement.

## Env vars

Toutes les vars sont dans `.env` (gitignorée). Tenir ce fichier à jour à chaque ajout de variable Cloud Run.

Variables requises :
- `DATABASE_URL` — Neon PostgreSQL
- `API_KEY` — clé pour les endpoints protégés
- `RAWG_API_KEY` — jeux (rawg.io)
- `TMDB_API_KEY` — films/séries (themoviedb.org)
- `STRAVA_CLIENT_ID` / `STRAVA_CLIENT_SECRET` / `STRAVA_REFRESH_TOKEN` — activités sportives
- `PORT` — 8080 (local uniquement)

## Commits

- Messages concis, en français ou anglais
- Pas de `Co-Authored-By`
