#!/usr/bin/env bash
# Rust 엔진 빌드 → C# 서버 실행. 브라우저에서 http://localhost:5000 을 여세요.
set -euo pipefail
cd "$(dirname "$0")"

if [ ! -f .env ] && [ -z "${ANTHROPIC_API_KEY:-}" ]; then
  echo "경고: .env 파일이 없습니다. cp .env.example .env 후 ANTHROPIC_API_KEY를 넣으세요."
  echo "      (키 없이도 실행은 되지만 Claude 대신 엔진 대체 수로 둡니다)"
fi

cargo build --release --manifest-path engine/Cargo.toml
exec dotnet run --project server --launch-profile http
