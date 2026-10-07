# chetaku-rs — Instructions Claude

## Deploy

Hébergé sur **chetbox** (docker compose + Caddy, Cloudflare devant) depuis 09/2026 — plus Cloud Run ni Scaleway Serverless.

**Toujours `bash deploy.sh`** (arbre git propre ; tag image = nb de commits) : build → push registre Scaleway → bump du tag dans
`/opt/chet/compose.yml` sur la box → `up -d --force-recreate` → vérifie `/health` et le tag réellement en cours d'exécution.
Rollback : `ROLLBACK=<tag> bash deploy.sh` (le script affiche le tag précédent).

Piège (07/10/2026) : ne jamais enchaîner `docker compose up` dans un script passé à `ssh … 'bash -s' <<EOF` — il avale le reste du
script via stdin et rien ne se déploie, sans erreur. `deploy.sh` utilise `ssh -n` et `< /dev/null`.

## Env vars

En prod : `/opt/chet/env/chetaku-rs.env` sur la box (jamais dans le dépôt) ; après édition, `docker compose up -d --force-recreate chetaku-rs`.
En local : `.env` (gitignorée).

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

## Route happy-lys (site cadeau d'anniversaire de Lys, 10/2026)
`GET|PUT /happy-lys/{token}/state` (`src/routes/happy_lys.rs`) : état des « bons à valoir » + souhait libre, dans UN objet S3 fixe
`happy-lys/state.json`. Secret = `HAPPY_LYS_TOKEN` (env box, sinon 404). Schéma strict (`b0..b99` bool, `txt` ≤ 300 car., `t` ms), corps ≤ 2 Ko,
un `t` plus ancien n'écrase jamais un plus récent. Appelée via Caddy (`happy-lys-2026.chetana.fr/api/*` → `chetaku-rs:8080/*`). Aucune base touchée.
