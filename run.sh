#!/usr/bin/env bash
# 바둑 게임방 실행: Rust 엔진 빌드 → C# 서버 실행 → 준비되면 화면 열기
#
#   ./run.sh             서버 실행 + 화면 열기
#   ./run.sh --browser   VS Code 터미널에서도 시스템 브라우저로 열기
#   ./run.sh --no-open   화면은 열지 않고 서버만 실행
#
# 종료: Ctrl+C
set -euo pipefail
cd "$(dirname "$0")"

PORT=5000
URL="http://localhost:$PORT"
MODE=auto
case "${1:-}" in
  --browser) MODE=browser ;;
  --no-open) MODE=none ;;
esac

server_up() { curl -sf "$URL/api/status" >/dev/null 2>&1; }

open_ui() {
  [ "$MODE" = none ] && return 0
  if [ "$MODE" = auto ] && [ "${TERM_PROGRAM:-}" = "vscode" ]; then
    # VS Code의 통합 브라우저는 명령줄에서 열 수 없어 방법을 안내한다
    cat <<EOF

  ┌──────────────────────────────────────────────────────────────┐
  │  바둑 게임방이 준비되었습니다:  $URL
  │
  │  VS Code 통합 브라우저로 열기:
  │    Ctrl+Shift+P → "Browser: Open Integrated Browser" → 위 주소 입력
  │  (또는 위 주소를 Ctrl+클릭)
  └──────────────────────────────────────────────────────────────┘

EOF
    return 0
  fi
  echo "브라우저에서 $URL 을 엽니다."
  if command -v xdg-open >/dev/null; then xdg-open "$URL" >/dev/null 2>&1 &
  elif command -v open >/dev/null; then open "$URL"
  else echo "브라우저를 자동으로 열 수 없습니다. $URL 을 직접 여세요."
  fi
}

if server_up; then
  echo "서버가 이미 실행 중입니다."
  open_ui
  exit 0
fi

echo "빌드 중... (Rust 엔진 + C# 서버)"
dotnet build server -nologo -v q

echo "서버 시작 중..."
# dotnet run 대신 빌드된 서버를 직접 실행해야 종료 시 서버 프로세스가 확실히 꺼진다
(cd server && exec dotnet bin/Debug/net10.0/GoArcade.dll --urls "$URL") &
SERVER_PID=$!
trap 'kill $SERVER_PID 2>/dev/null; wait $SERVER_PID 2>/dev/null; echo; echo "서버를 종료했습니다."' EXIT INT TERM

for _ in $(seq 1 120); do
  if server_up; then break; fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then echo "서버가 시작되지 못했습니다. 위 오류를 확인하세요."; exit 1; fi
  sleep 0.5
done
server_up || { echo "서버가 응답하지 않습니다."; exit 1; }

open_ui
echo "종료하려면 Ctrl+C를 누르세요."
wait "$SERVER_PID"
