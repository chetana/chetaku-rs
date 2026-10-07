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

## Déploiement actuel (depuis 09/2026) : chetbox, PAS Cloud Run / Scaleway Serverless
`deploy.sh` et la section ci-dessus sont **obsolètes**. Aujourd'hui : `docker build -t rg.fr-par.scw.cloud/chetana-apps/chetaku-rs:<N> .` (N = `git rev-list --count HEAD`),
`docker push`, puis sur la box (`ssh -i ~/.ssh/chetbox root@163.172.7.239`, `/opt/chet`) : éditer le tag dans `compose.yml`,
`docker compose up -d --force-recreate chetaku-rs < /dev/null` (le `--force-recreate` relit `env/chetaku-rs.env`). Rollback = remettre le tag précédent.
Piège : dans un `ssh … 'bash -s' <<EOF`, `docker compose up` avale le reste du script via stdin → toujours `< /dev/null`.

## Route happy-lys (site cadeau d'anniversaire de Lys, 10/2026)
`GET|PUT /happy-lys/{token}/state` (`src/routes/happy_lys.rs`) : état des « bons à valoir » + souhait libre, dans UN objet S3 fixe
`happy-lys/state.json`. Secret = `HAPPY_LYS_TOKEN` (env box, sinon 404). Schéma strict (`b0..b99` bool, `txt` ≤ 300 car., `t` ms), corps ≤ 2 Ko,
un `t` plus ancien n'écrase jamais un plus récent. Appelée via Caddy (`happy-lys-2026.chetana.fr/api/*` → `chetaku-rs:8080/*`). Aucune base touchée.
