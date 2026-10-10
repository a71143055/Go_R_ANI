#!/usr/bin/env bash
# 바둑 게임방 실행: 빌드 → 서버 실행 → 준비되면 화면 열기
#
#   ./run.sh             이 컴퓨터에서만 (http://localhost:5000)
#   ./run.sh --public    인터넷에도 열기: Cloudflare 무료 터널로 https 주소를 받는다
#                        (계정 불필요, 단 다시 켤 때마다 주소가 바뀜)
#   ./run.sh --browser   VS Code 터미널에서도 시스템 브라우저로 열기
#   ./run.sh --no-open   화면은 열지 않음
#
# 종료: Ctrl+C (서버와 터널이 함께 꺼진다)
set -euo pipefail
cd "$(dirname "$0")"

PORT="${PORT:-5000}"
URL="http://localhost:$PORT"
MODE=auto
PUBLIC=0
for arg in "$@"; do
  case "$arg" in
    --public) PUBLIC=1 ;;
    --browser) MODE=browser ;;
    --no-open) MODE=none ;;
  esac
done

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

PIDS=()
cleanup() {
  for p in "${PIDS[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done
  wait 2>/dev/null || true
  echo; echo "종료했습니다."
}

if server_up; then
  echo "서버가 이미 실행 중입니다."
  if [ "$PUBLIC" = 1 ] && ! curl -sf "$URL/api/auth/config" | grep -q canShutdown; then
    echo "실행 중인 서버는 보안 조치 이전 버전이라 인터넷에 열지 않습니다. 끄고 다시 실행하세요."
    exit 1
  fi
else
  echo "빌드 중... (Rust 엔진 + C# 서버)"
  dotnet build server -nologo -v q

  echo "서버 시작 중..."
  # dotnet run 대신 빌드된 서버를 직접 실행해야 종료 시 서버 프로세스가 확실히 꺼진다
  (cd server && exec dotnet bin/Debug/net10.0/GoArcade.dll --urls "$URL") &
  PIDS+=($!)
  trap cleanup EXIT INT TERM
  for _ in $(seq 1 120); do
    if server_up; then break; fi
    if ! kill -0 "${PIDS[0]}" 2>/dev/null; then echo "서버가 시작되지 못했습니다. 위 오류를 확인하세요."; exit 1; fi
    sleep 0.5
  done
  server_up || { echo "서버가 응답하지 않습니다."; exit 1; }
fi

if [ "$PUBLIC" = 1 ]; then
  command -v cloudflared >/dev/null || { echo "cloudflared가 없습니다. README의 '인터넷에 열기'를 보세요."; exit 1; }
  LOG="$(mktemp)"
  cloudflared tunnel --no-autoupdate --url "$URL" >"$LOG" 2>&1 &
  PIDS+=($!)
  trap cleanup EXIT INT TERM
  PUB=""
  for _ in $(seq 1 60); do
    PUB="$(grep -oE 'https://[a-z0-9-]+\.trycloudflare\.com' "$LOG" | head -1 || true)"
    [ -n "$PUB" ] && break
    sleep 1
  done
  if [ -z "$PUB" ]; then echo "터널 주소를 받지 못했습니다:"; tail -5 "$LOG"; exit 1; fi
  cat <<EOF

  ┌──────────────────────────────────────────────────────────────┐
  │  인터넷 주소:  $PUB
  │
  │  이 주소로 누구나 접속할 수 있습니다 (이 컴퓨터가 켜져 있는 동안).
  │  다시 실행하면 주소가 바뀝니다.
  └──────────────────────────────────────────────────────────────┘

EOF
fi

open_ui
if [ "${#PIDS[@]}" -gt 0 ]; then
  echo "종료하려면 Ctrl+C를 누르세요."
  wait
fi
