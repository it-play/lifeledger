#!/usr/bin/env bash
# Read-only M5-F production observation snapshot.
set -euo pipefail
export PATH="/usr/local/bin:/opt/homebrew/bin:$PATH"

cd "$(dirname "$0")/.."

readonly HEALTH_URL="http://127.0.0.1:10105/api/health"

echo "[observe] API health"
curl -fsS --max-time 3 "$HEALTH_URL"
echo

echo "[observe] container health"
docker inspect --format 'server={{.State.Health.Status}}' lifeledger-server
docker inspect --format 'offlineWorker={{.State.Health.Status}}' lifeledger-offline-worker

echo "[observe] sanitized operations report"
docker compose run --rm --no-deps --entrypoint ops-report server
