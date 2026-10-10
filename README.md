# Go_R_ANI — Claude 바둑 게임방

바둑과 관련된 게임 세 가지를 **Claude AI**와 둘 수 있는 웹 앱입니다.

| 게임 | 내용 |
|---|---|
| **바둑** | 9·13·19줄. 패·자충수 규칙, 중국식 계가, Claude 사석 판정, 무르기·기권 |
| **맹기 바둑** | 판에 돌이 보이지 않고 수순만 보며 두는 바둑. "나만" 또는 "Claude도" 맹기로 둘 수 있음 |
| **오목** | 15줄, 5목 이상이면 승리 (자유 룰) |
| **알까기** | 돌을 튕겨 상대 돌을 판 밖으로. Claude가 샷과 힘을 고름 |

## 구조

```
브라우저 (server/wwwroot: HTML/JS — 그리기와 입력만)
   │  HTTP JSON
   ▼
C# ASP.NET Core 서버 (server/)
   ├─ 게임 세션 관리, API 엔드포인트          Games.cs, Sessions.cs
   ├─ Claude 호출 (공식 Anthropic C# SDK)     ClaudeAi.cs, Prompts.cs
   └─ Rust 엔진 호출 (P/Invoke)               Engine.cs
         │  C ABI: engine_call(json) / engine_free(ptr)
         ▼
Rust 엔진 (engine/, cdylib)
   ├─ go.rs       바둑 규칙: 착수·따내기·패·자충수·계가·대체 수
   ├─ omok.rs     오목 승리 판정·긴급 지점·대체 수
   └─ alkkagi.rs  알까기 물리 시뮬레이션 (프레임 단위로 결과 반환)
```

- **AI는 Claude 앱 로그인이 아니라 Anthropic API 키로 직접 호출**합니다. 키는 서버에만 있고 브라우저로 전달되지 않습니다.
- 기본 모델은 `claude-opus-5-5`이고, 응답은 JSON 스키마(structured outputs)로 받습니다. Claude가 거절하면 API가 자동으로 대체 모델로 다시 시도합니다(`fallbacks: "default"`).
- Claude가 고른 수는 항상 Rust 엔진이 규칙을 검사합니다. 둘 수 없는 수면 Claude에게 이유를 알려 주고 한 번 더 묻고, 그래도 안 되거나 API에 연결할 수 없으면 엔진의 대체 수를 둡니다. 이때 화면에 "(엔진 대체 수)"라고 표시됩니다.

## 실행

필요한 것: .NET SDK 10, Rust(cargo), Anthropic API 키

```bash
cp .env.example .env        # ANTHROPIC_API_KEY=sk-ant-... 입력
./run.sh                    # Rust 엔진 빌드 + C# 서버 실행
```

브라우저에서 <http://localhost:5000> 을 엽니다.

`dotnet build`만 해도 C# 빌드 전에 `cargo build --release`가 자동으로 실행되고, 만들어진 엔진 라이브러리가 출력 폴더로 복사됩니다.

## 맹기 바둑

- **맹기 (나만)**: 판에 돌이 보이지 않습니다. 직전 수만 1.5초 동안 표시되고, 오른쪽에 수순 목록이 나옵니다. 대국 중에는 서버가 판 정보를 가려서 보내므로 개발자 도구로도 볼 수 없습니다. "엿보기" 버튼을 누르고 있는 동안만 판이 보이고, 엿본 횟수가 기록됩니다.
- **맹기 (Claude도)**: Claude에게도 판면을 주지 않고 전체 수순만 줍니다. Claude는 머릿속으로 판을 재구성해서 둡니다.
- 종국(양쪽 패스) 후에는 판이 공개되고 계가합니다.

## AI 생각 깊이

상단의 "AI 생각 깊이"는 Claude API의 `effort` 값입니다. `low`가 가장 빠르고, `high`/`xhigh`로 갈수록 강하지만 느리고 비용이 늘어납니다.

## 테스트

```bash
cargo test --manifest-path engine/Cargo.toml     # 규칙·물리 엔진 단위 테스트
```

## API 요약

| 메서드 | 경로 | 설명 |
|---|---|---|
| GET | `/api/status` | 모델, API 키 설정 여부 |
| POST | `/api/go/new` | `{size, komi, humanColor, blind: "off"\|"human"\|"both"}` |
| POST | `/api/go/{id}/play` | `{move: "D4"\|"pass"}` 사람의 수 |
| POST | `/api/go/{id}/ai` | `{effort}` Claude의 수 |
| POST | `/api/go/{id}/score` | 종국 후 Claude 사석 판정 + 계가 |
| POST | `/api/go/{id}/undo`, `/resign`, `/peek` | 무르기, 기권, 맹기 엿보기 |
| POST | `/api/omok/new`, `/{id}/play`, `/{id}/ai`, `/{id}/undo` | 오목 |
| POST | `/api/alkkagi/new`, `/{id}/shoot`, `/{id}/ai` | 알까기 (`shoot`: `{stone, angle, power}`) |
