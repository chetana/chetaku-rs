#!/usr/bin/env bash
# deploy.sh — déploie chetaku-rs sur Cloud Run et restaure les env vars depuis .env
# Usage: bash deploy.sh

set -e

REGION="europe-west1"
SERVICE="chetaku-rs"
ENV_FILE=".env"

if [ ! -f "$ENV_FILE" ]; then
  echo "❌ Fichier .env introuvable — annulé"
  exit 1
fi

echo "🚀 Deploy Cloud Run..."
gcloud run deploy "$SERVICE" --source . --region "$REGION" --allow-unauthenticated

echo ""
echo "🔑 Restauration des env vars depuis $ENV_FILE..."

# Lire le .env, ignorer les lignes vides et les commentaires, construire KEY=VALUE,...
ENV_VARS=$(grep -v '^#' "$ENV_FILE" | grep -v '^[[:space:]]*$' | tr '\n' ',' | sed 's/,$//')

if [ -z "$ENV_VARS" ]; then
  echo "⚠️  Aucune var trouvée dans $ENV_FILE"
  exit 1
fi

gcloud run services update "$SERVICE" --region "$REGION" --update-env-vars "$ENV_VARS"

echo ""
echo "✅ Deploy terminé avec toutes les env vars"
