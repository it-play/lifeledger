#!/usr/bin/env bash
# ApplicationStop — 여기서 컨테이너를 내리지 않는다.
#
# 빌드(AfterInstall)가 몇 분 걸리므로, 그동안 기존 컨테이너가 계속 서비스하게 두고
# start.sh 의 `up -d` 로 한 번에 교체한다. 다운타임을 컨테이너 교체 순간으로 줄인다.
set -euo pipefail
export PATH="/usr/local/bin:/opt/homebrew/bin:$PATH"

cd "$(dirname "$0")/.."

echo "[stop] 현재 상태:"
docker compose ps 2>/dev/null || echo "[stop] 아직 배포된 적 없음 (첫 배포)"
