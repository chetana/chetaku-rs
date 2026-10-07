#!/usr/bin/env bash
# Déploiement chetaku-rs → chetbox (docker compose + Caddy, Cloudflare devant).
# build → push registre Scaleway → bump du tag dans /opt/chet/compose.yml → recreate → vérifie /health.
# Tag image = nombre de commits git (le tag en prod ne diverge jamais du code qui a été buildé).
#
# Usage : bash deploy.sh                 # build + push + déploie le HEAD (arbre git propre exigé)
#         ROLLBACK=32 bash deploy.sh     # remet un tag déjà dans le registre, sans build
#         ALLOW_DIRTY=1 bash deploy.sh   # build malgré des modifs non commitées (déconseillé)
#
# Les variables d'env (secrets) vivent dans /opt/chet/env/chetaku-rs.env SUR LA BOX, pas ici :
# ce script n'y touche pas. Après l'avoir édité à la main : `docker compose up -d --force-recreate chetaku-rs`
# (un simple `restart` ne relit PAS le fichier d'env).
set -euo pipefail
cd "$(dirname "$0")"

BOX=root@163.172.7.239
KEY="${CHETBOX_KEY:-$HOME/.ssh/chetbox}"
DIR=/opt/chet
SERVICE=chetaku-rs
REG=rg.fr-par.scw.cloud/chetana-apps/chetaku-rs
URL=https://chetaku.chetana.fr

# `-n` : ssh ne lit JAMAIS notre stdin. Et chaque commande distante est une ligne unique avec
# `< /dev/null` : `docker compose up` avale sinon le reste d'un script passé en stdin (bug vécu le 07/10/2026,
# la box restait sur l'ancienne image sans aucune erreur).
rsh() { ssh -n -i "$KEY" -o BatchMode=yes -o ConnectTimeout=10 "$BOX" "$@"; }

TAG="${ROLLBACK:-}"
if [ -z "$TAG" ]; then
  if [ -z "${ALLOW_DIRTY:-}" ] && [ -n "$(git status --porcelain)" ]; then
    echo "✗ arbre git non propre : commite d'abord (le tag = nb de commits, il doit correspondre au code buildé)" >&2
    echo "  (ou ALLOW_DIRTY=1 bash deploy.sh)" >&2
    exit 1
  fi
  TAG=$(git rev-list --count HEAD)
  echo "→ build $REG:$TAG"
  docker --context default build -t "$REG:$TAG" .
  echo "→ push"
  docker --context default push "$REG:$TAG" >/dev/null || { echo "✗ push échoué — registre : scw registry login" >&2; exit 1; }
else
  echo "→ ROLLBACK vers $REG:$TAG (pas de build)"
fi

PREV=$(rsh "grep -oE '$REG:[0-9A-Za-z._-]+' $DIR/compose.yml | head -1 | cut -d: -f2")
echo "→ box : tag actuel $PREV → $TAG"
rsh "cd $DIR && cp compose.yml compose.yml.bak-deploy && sed -i -E 's#($REG):[0-9A-Za-z._-]+#\\1:$TAG#' compose.yml"
if ! rsh "grep -qF '$REG:$TAG' $DIR/compose.yml"; then
  rsh "cd $DIR && cp compose.yml.bak-deploy compose.yml"
  echo "✗ compose.yml non modifié (restauré) — vérifie le nom du service/image" >&2; exit 1
fi
if ! rsh "cd $DIR && docker compose pull $SERVICE < /dev/null" >/dev/null 2>&1; then
  rsh "cd $DIR && cp compose.yml.bak-deploy compose.yml"
  echo "✗ image $REG:$TAG introuvable dans le registre — compose.yml restauré, service intact" >&2; exit 1
fi
echo "→ recreate (relit aussi env/$SERVICE.env)"
rsh "cd $DIR && docker compose up -d --force-recreate $SERVICE < /dev/null" 2>&1 | tail -2

ok=""
for i in $(seq 1 20); do
  code=$(curl -s -o /dev/null -w "%{http_code}" "$URL/health" --max-time 8 || true)
  [ "$code" = "200" ] && ok=1 && break
  sleep 3
done
RUN=$(rsh "cd $DIR && docker compose ps $SERVICE --format '{{.Image}} {{.Status}}' < /dev/null")
echo "→ box : $RUN"
case "$RUN" in *":$TAG "*) ;; *) echo "✗ la box ne tourne PAS sur le tag $TAG" >&2; exit 1;; esac
if [ -z "$ok" ]; then echo "✗ $URL/health ne répond pas 200 — rollback : ROLLBACK=$PREV bash deploy.sh" >&2; exit 1; fi
echo "→ $URL/health : 200"
echo "✅ chetaku-rs déployé ($REG:$TAG) — rollback si besoin : ROLLBACK=$PREV bash deploy.sh"
