#!/usr/bin/env bash
# Rust 엔진(규칙 + 자체 AI) 빌드 → C# 서버 실행. 브라우저에서 http://localhost:5000 을 여세요.
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --manifest-path engine/Cargo.toml
exec dotnet run --project server --launch-profile http
