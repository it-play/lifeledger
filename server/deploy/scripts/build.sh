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
build_pid=""

cleanup_build() {
  if [[ -n "$build_pid" ]] && kill -0 "$build_pid" 2>/dev/null; then
    kill "$build_pid" 2>/dev/null || true
    wait "$build_pid" 2>/dev/null || true
  fi
  build_pid=""
}

terminate_build() {
  cleanup_build
  exit 130
}

# A release rebuild can be silent for several minutes. Periodic output keeps the
# remote execution channel alive until Docker finishes the runtime image stages.
trap cleanup_build EXIT
trap terminate_build HUP INT TERM
docker compose build &
build_pid=$!

while kill -0 "$build_pid" 2>/dev/null; do
  sleep 20
  if kill -0 "$build_pid" 2>/dev/null; then
    echo "[build] still running (${SECONDS}s elapsed)"
  fi
done

if wait "$build_pid"; then
  build_pid=""
else
  build_status=$?
  build_pid=""
  echo "[build] failed with exit code $build_status" >&2
  exit "$build_status"
fi

trap - EXIT HUP INT TERM
echo "[build] 완료"
