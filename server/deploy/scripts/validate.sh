#!/usr/bin/env bash
# ValidateService — 헬스 엔드포인트가 살아날 때까지 기다린다.
set -euo pipefail
export PATH="/usr/local/bin:/opt/homebrew/bin:$PATH"

cd "$(dirname "$0")/.."

readonly HEALTH_URL="http://127.0.0.1:10105/api/health"
readonly ATTEMPTS=30

for attempt in $(seq 1 "$ATTEMPTS"); do
  worker_health="$(docker inspect --format '{{.State.Health.Status}}' lifeledger-offline-worker 2>/dev/null || true)"
  if curl -fsS --max-time 3 "$HEALTH_URL" >/dev/null 2>&1 \
    && [[ "$worker_health" == "healthy" ]]; then
    echo "[validate] 정상 ($attempt 번째 시도)"
    curl -fsS "$HEALTH_URL"
    echo
    exit 0
  fi
  sleep 2
done

echo "[validate] $((ATTEMPTS * 2))초 안에 healthy 가 되지 않았습니다. 최근 로그:" >&2
docker compose logs --tail 50 server offline-worker >&2
exit 1
