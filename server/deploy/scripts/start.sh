#!/usr/bin/env bash
# ApplicationStart — 새 이미지로 컨테이너를 교체한다.
set -euo pipefail
export PATH="/usr/local/bin:/opt/homebrew/bin:$PATH"

cd "$(dirname "$0")/.."

echo "[start] 컨테이너 교체"
docker compose up -d --remove-orphans

# 교체로 떨어져 나온 이전 이미지만 정리한다 (다른 프로젝트 이미지는 건드리지 않는다)
docker image prune -f --filter "label=com.docker.compose.project=lifeledger" >/dev/null 2>&1 || true

docker compose ps
