#!/usr/bin/env bash
# 컴퓨터를 켜면(로그인하면) 바둑 게임방 서버와 고정 주소(Tailscale Funnel)가 저절로 켜지게 한다.
# 관리자 권한 없이 이 계정의 systemd 사용자 서비스로 등록한다.
#
#   ./deploy/install-autostart.sh            설치 + 지금 바로 켜기
#   ./deploy/install-autostart.sh --remove   자동 실행 해제
set -euo pipefail
cd "$(dirname "$0")/.."
REPO="$(pwd)"
UNIT_DIR="$HOME/.config/systemd/user"
TS="$HOME/.local/bin/tailscale --socket=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/goarcade-tailscaled.sock"

if [ "${1:-}" = "--remove" ]; then
  systemctl --user disable --now goarcade.service goarcade-tailscale.service 2>/dev/null || true
  rm -f "$UNIT_DIR/goarcade.service" "$UNIT_DIR/goarcade-tailscale.service"
  systemctl --user daemon-reload
  echo "자동 실행을 해제했습니다."
  exit 0
fi

DOTNET_DIR="$(dirname "$(readlink -f "$(command -v dotnet)")")"
CARGO_DIR="$(dirname "$(command -v cargo)")"
BIN="$HOME/.local/bin"
[ -x "$BIN/tailscaled" ] || { echo "tailscaled가 없습니다 ($BIN). README의 '고정 주소'를 보세요."; exit 1; }

mkdir -p "$UNIT_DIR"
for u in goarcade.service goarcade-tailscale.service; do
  sed -e "s|@REPO@|$REPO|g" -e "s|@DOTNET_DIR@|$DOTNET_DIR|g" -e "s|@CARGO_DIR@|$CARGO_DIR|g" -e "s|@BIN@|$BIN|g" \
    "deploy/systemd/$u" > "$UNIT_DIR/$u"
done
systemctl --user daemon-reload
systemctl --user enable --now goarcade-tailscale.service goarcade.service
echo "자동 실행을 등록하고 켰습니다."

# 로그인하지 않아도 부팅하면 켜지게 (허용되지 않는 환경이면 로그인할 때 켜짐)
loginctl enable-linger "$USER" 2>/dev/null && echo "부팅만 해도 켜지도록 설정했습니다." || echo "(로그인하면 켜집니다)"

echo
echo "서버 상태:  systemctl --user status goarcade"
echo "주소 상태:  $TS funnel status"
