# chetaku-rs — Instructions Claude

## Deploy

**Toujours utiliser `deploy.sh`**, jamais `gcloud run deploy` directement :

```bash
bash deploy.sh
```

Ce script passe `--set-env-vars` **dans le même appel** `gcloud run deploy --source .` → une seule révision créée, env vars garanties.

⚠️ **Ne JAMAIS faire séparément :**
- `gcloud run deploy --source .` seul → efface toutes les env vars
- `gcloud run services update --update-env-vars ...` via PowerShell → crée des doubles révisions (PowerShell exécute gcloud.cmd deux fois), ce qui peut écraser les vars avec des valeurs partielles

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
