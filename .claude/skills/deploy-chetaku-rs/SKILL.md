---
name: deploy-chetaku-rs
description: Build, commit, push and deploy chetaku-rs to Cloud Run with env vars
allowed-tools: Bash, Read
---

# Deploy chetaku-rs

Deploy the Rust API to Cloud Run and restore all env vars from `.env`.

⚠️ **`gcloud run deploy --source .` efface les env vars** — `deploy.sh` les restaure automatiquement depuis `.env`.

## Steps

1. **Pre-flight checks**
   - Run `git status` to check for uncommitted changes
   - Verify `.env` exists and contains all required vars (DATABASE_URL, API_KEY, RAWG_API_KEY, TMDB_API_KEY, STRAVA_*)

2. **Commit & Push**
   - Stage relevant files (never stage `.env`)
   - Commit with a descriptive message (no Co-Authored-By)
   - Push to `origin main`

3. **Deploy via deploy.sh**
   - Run: `powershell.exe -ExecutionPolicy Bypass -Command "Set-Location 'C:\Users\cheta\repositories\chetaku-rs'; bash deploy.sh"`
   - Le script fait : `gcloud run deploy --source .` puis `gcloud run services update --update-env-vars` depuis `.env`
   - Attendre la confirmation de révision (ex: `chetaku-rs-000XX-xxx`)

4. **Post-deploy verification**
   - `curl -s https://chetaku-rs-267131866578.europe-west1.run.app/health` → `{"status":"ok"}`
   - Vérifier les env vars restaurées : `gcloud run services describe chetaku-rs --region europe-west1 --format='value(spec.template.spec.containers[0].env)'`

5. **Report** results to the user

## Important
- Repo : `C:\Users\cheta\repositories\chetaku-rs`
- Service URL : `https://chetaku-rs-267131866578.europe-west1.run.app`
- Region : `europe-west1`
- gcloud cmd path : `C:\Users\cheta\AppData\Local\Google\Cloud SDK\google-cloud-sdk\bin\gcloud.cmd`
- Pour appeler gcloud directement (sans deploy.sh) :
  `powershell.exe -ExecutionPolicy Bypass -Command "& 'C:\Users\cheta\AppData\Local\Google\Cloud SDK\google-cloud-sdk\bin\gcloud.cmd' ..."`
