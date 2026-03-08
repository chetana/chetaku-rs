#!/usr/bin/env bash
# deploy.sh — déploie chetaku-rs sur Cloud Run avec les env vars depuis .env
# Usage: bash deploy.sh
#
# ⚠️  Un seul appel gcloud (--set-env-vars inclus dans le deploy) pour éviter
#     les problèmes de double-révision sous Windows/PowerShell.

set -e

REGION="europe-west1"
SERVICE="chetaku-rs"
ENV_FILE=".env"

if [ ! -f "$ENV_FILE" ]; then
  echo "Fichier .env introuvable — annulé"
  exit 1
fi

# Lire le .env, ignorer lignes vides et commentaires, construire KEY=VALUE,...
ENV_VARS=$(grep -v '^#' "$ENV_FILE" | grep -v '^[[:space:]]*$' | grep -v '^PORT=' | tr '\n' ',' | sed 's/,$//')

if [ -z "$ENV_VARS" ]; then
  echo "Aucune var trouvée dans $ENV_FILE"
  exit 1
fi

echo "Deploy $SERVICE ($REGION) avec $(echo "$ENV_VARS" | tr ',' '\n' | wc -l | tr -d ' ') env vars..."

# Utiliser gcloud.cmd sur Windows si disponible
GCLOUD="gcloud"
if [ -f "/c/Users/cheta/AppData/Local/Google/Cloud SDK/google-cloud-sdk/bin/gcloud.cmd" ]; then
  GCLOUD="cmd.exe /c gcloud.cmd"
fi

$GCLOUD run deploy "$SERVICE" \
  --source . \
  --region "$REGION" \
  --allow-unauthenticated \
  --set-env-vars "$ENV_VARS"

echo ""
echo "Deploy terminé avec toutes les env vars"
