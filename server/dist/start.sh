#!/usr/bin/env bash
# 바둑 게임방 실행 (리눅스·맥). 터미널에서: ./start.sh   종료: Ctrl+C 또는 화면의 "앱 종료"
set -euo pipefail
cd "$(dirname "$0")"
URL="http://localhost:5000"

if curl -sf "$URL/api/status" >/dev/null 2>&1; then
  echo "이미 실행 중입니다: $URL"
else
  ./GoArcade --urls "$URL" &
  PID=$!
  trap 'kill $PID 2>/dev/null; wait $PID 2>/dev/null; echo "종료했습니다."' EXIT INT TERM
  for _ in $(seq 1 60); do
    curl -sf "$URL/api/status" >/dev/null 2>&1 && break
    kill -0 "$PID" 2>/dev/null || { echo "시작하지 못했습니다."; exit 1; }
    sleep 0.5
  done
fi

echo "바둑 게임방: $URL"
if command -v xdg-open >/dev/null; then xdg-open "$URL" >/dev/null 2>&1 &
elif command -v open >/dev/null; then open "$URL"
fi
[ -n "${PID:-}" ] && wait "$PID"
