#!/usr/bin/env bash
# Déploiement chetaku-rs → Scaleway Serverless Container.
# build → push → update image → deploy → vérifie. Tag image = nombre de commits git.
# Usage : bash deploy.sh
set -euo pipefail
cd "$(dirname "$0")"

CID=bb76402f-4ec9-48e6-8c29-79d9bb2aa142
REG=rg.fr-par.scw.cloud/chetana-apps/chetaku-rs
URL=https://chetaku.chetana.fr
TAG=$(git rev-list --count HEAD)

echo "→ build $REG:$TAG"
docker --context default build -t "$REG:$TAG" .
echo "→ push"
docker --context default push "$REG:$TAG"
echo "→ deploy Scaleway (préserve env/secrets : on ne passe QUE image=)"
scw container container update "$CID" image="$REG:$TAG" >/dev/null
scw container container deploy "$CID" >/dev/null

st=""
for i in $(seq 1 40); do
  st=$(scw container container get "$CID" -o json | node -e 'process.stdout.write(JSON.parse(require("fs").readFileSync(0)).status)')
  [ "$st" = "ready" ] && break
  sleep 6
done
echo "→ container : $st"
code=$(curl -s -o /dev/null -w "%{http_code}" "$URL/" --max-time 20)
echo "→ $URL : $code"
echo "✅ chetaku-rs déployé ($REG:$TAG)"
