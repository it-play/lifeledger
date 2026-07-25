#!/usr/bin/env bash
# AfterInstall — 전송된 소스로 이미지를 만든다 (arm64 네이티브 빌드).
set -euo pipefail
export PATH="/usr/local/bin:/opt/homebrew/bin:$PATH"

cd "$(dirname "$0")/.."

if [[ ! -f app.env ]]; then
  echo "[build] app.env 가 없습니다 — production 환경의 SERVER_ENV 시크릿을 확인하세요" >&2
  exit 1
fi

echo "[build] 이미지 빌드 시작"
docker compose build
echo "[build] 완료"
