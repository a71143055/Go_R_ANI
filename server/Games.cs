using System.Text.Json.Nodes;

namespace GoArcade;

/// <param name="Blind">맹기 모드: "off" | "human"(돌을 숨기고 수순은 공개) | "strict"(돌과 수순 모두 숨김)</param>
public record NewGoRequest(int Size = 9, double Komi = 6.5, int HumanColor = 1, string Blind = "off");
/// <param name="Cross">십자 배치(가운데 흑백 2개씩)로 시작</param>
public record NewAtariRequest(int Size = 7, int HumanColor = 1, bool Cross = true);
/// <param name="Variant">"omok" | "pente" | "connect6"</param>
public record NewLineRequest(string Variant = "omok", int HumanColor = 1, int? Size = null);
public record NewTsumegoRequest(string Id);
public record MoveRequest(string Move);
/// <param name="Level">AI 난이도: "easy"(하) | "normal"(중) | "hard"(상)</param>
public record AiRequest(string? Level = null);

/// <summary>
/// 게임별 HTTP 엔드포인트. 규칙 판정과 AI 모두 Rust 엔진이 맡는다.
/// 착수 응답에는 따낸 돌의 좌표 목록(captured)이 들어 있어 화면의 사석함과 로봇의 돌 회수에 쓴다.
/// </summary>
public static class Games
{
    /// <summary>난이도 → 엔진 level(1 하, 2 중, 3 상)</summary>
    private static int Level(string? level) => level switch { "easy" => 1, "hard" => 3, _ => 2 };

    /// <summary>바둑 AI 생각 시간(ms)</summary>
    private static int GoMs(string? level) => level switch { "easy" => 500, "hard" => 6000, _ => 2000 };

    /// <summary>오목류·따내기 바둑 AI 생각 시간(ms)</summary>
    private static int QuickMs(string? level) => level switch { "easy" => 200, "hard" => 3000, _ => 800 };

    private static IHttpContextAccessor _http = null!;

    /// <summary>AI·풀이기처럼 무거운 계산은 동시에 2개까지만 (인터넷에 열었을 때 컴퓨터 마비 방지)</summary>
    private static readonly SemaphoreSlim AiSlots = new(2, 2);

    private static async Task<JsonObject> Heavy(Func<JsonObject> work, CancellationToken ct)
    {
        await AiSlots.WaitAsync(ct);
        try { return await Task.Run(work, ct); }
        finally { AiSlots.Release(); }
    }
    private static RecordStore _records = null!;

    /// <summary>로그인한 계정 ID</summary>
    public static string? CurrentUser() =>
        _http.HttpContext?.User.FindFirst(System.Security.Claims.ClaimTypes.NameIdentifier)?.Value;

    /// <summary>새 대국을 등록하고 바로 기록한다</summary>
    private static Session Start(SessionStore store, Session s)
    {
        store.Add(s);
        _records.Save(s);
        return s;
    }

    public static void MapGames(this WebApplication app)
    {
        var store = app.Services.GetRequiredService<SessionStore>();
        _http = app.Services.GetRequiredService<IHttpContextAccessor>();
        _records = app.Services.GetRequiredService<RecordStore>();

        app.MapGet("/api/status", () =>
        {
            var info = Engine.Call("engine_info");
            return Results.Json(new { engine = "Rust 자체 AI", threads = info["threads"]!.GetValue<int>() });
        });

        // 게임 API는 모두 로그인이 필요하다
        var api = app.MapGroup("/api").RequireAuthorization();
        MapGo(api.MapGroup("/go"), store);
        MapAtari(api.MapGroup("/atari"), store);
        MapLine(api.MapGroup("/line"), store);
        MapTsumego(api.MapGroup("/tsumego"), store);

        // 대국 끝내기 (바둑은 MapGo에서 계가로 처리)
        api.MapPost("/atari/{id}/finish", (string id) => WithSession(store, id, GameKind.AtariGo, s => FinishNoResult(s, "")));
        api.MapPost("/line/{id}/finish", (string id) => WithSession(store, id, GameKind.Line, s =>
        {
            var caps = s.State["captures"]!.AsArray();
            var extra = s.State["variant"]!.GetValue<string>() == "pente" ? $" 따낸 돌은 흑 {caps[0]}개, 백 {caps[1]}개입니다." : "";
            return FinishNoResult(s, extra);
        }));
        api.MapPost("/tsumego/{id}/finish", (string id, CancellationToken ct) => WithSession(store, id, GameKind.Tsumego, async s =>
        {
            if (s.Over) return Conflict("이 문제는 끝났습니다");
            // 그만 풀기: 처음 국면의 정답을 알려 준다
            var pid = s.Meta["problem"]!["id"]!.GetValue<string>();
            var start = Engine.Call("tsumego_new", new JsonObject { ["id"] = pid })["state"]!.DeepClone();
            var hint = await Heavy(() => Engine.Call("tsumego_hint", new JsonObject { ["id"] = pid, ["state"] = start }), ct);
            var answers = string.Join(", ", hint["moves"]!.AsArray().Select(m => m!.GetValue<string>()));
            s.Over = true;
            s.Result = "그만 풀기";
            s.Meta["status"] = "failed";
            return Results.Json(s.View(new JsonObject { ["ai"] = new JsonObject { ["comment"] = $"풀기를 그만둡니다. 첫 수 정답은 {answers}입니다." } }));
        }));

        // 타이머: 사람의 시간이 다 되면 시간패
        foreach (var (prefix, kind) in new[] { ("go", GameKind.Go), ("atari", GameKind.AtariGo), ("line", GameKind.Line), ("tsumego", GameKind.Tsumego) })
        {
            api.MapPost($"/{prefix}/{{id}}/timeout", (string id) => WithSession(store, id, kind, s =>
            {
                if (s.Over) return Results.Json(s.View());
                s.Over = true;
                s.Result = kind == GameKind.Tsumego ? "시간 초과" : $"{ColorName(s.AiColor)}(AI) 시간승 — 내 시간이 다 되었습니다";
                if (kind == GameKind.Tsumego) s.Meta["status"] = "failed";
                return Results.Json(s.View());
            }));
        }
    }

    // ------------------------------------------------------------ 바둑

    private static void MapGo(RouteGroupBuilder go, SessionStore store)
    {
        go.MapPost("/new", (NewGoRequest req) => Guard(() =>
        {
            if (req.HumanColor is not (1 or 2)) return BadColor();
            if (req.Blind is not ("off" or "human" or "strict")) return Results.BadRequest(new { error = "blind는 off / human / strict" });
            var res = Engine.Call("go_new", new JsonObject { ["size"] = req.Size, ["komi"] = req.Komi });
            var s = Start(store, new Session(GameKind.Go, res["state"]!.AsObject(), req.HumanColor) { Blind = req.Blind, UserId = CurrentUser() });
            return Results.Json(s.View());
        }));

        go.MapPost("/{id}/play", (string id, MoveRequest req) => WithSession(store, id, GameKind.Go, s =>
        {
            if (s.Over) return Conflict("이미 끝난 대국입니다");
            if (s.ToPlay != s.HumanColor) return Conflict("AI 차례입니다");
            var captured = HumanGoMove(s, req.Move);
            if (s.State["passes"]!.GetValue<int>() >= 2) s.Over = true; // 결과는 /score에서 확정
            return Results.Json(s.View(GoExtra(s, captured)));
        }));

        go.MapPost("/{id}/ai", (string id, AiRequest req, CancellationToken ct) => WithSession(store, id, GameKind.Go, async s =>
        {
            if (s.Over) return Conflict("이미 끝난 대국입니다");
            if (s.ToPlay != s.AiColor) return Conflict("사람 차례입니다");
            var state = s.State.DeepClone();
            var gen = await Heavy(() => Engine.Call("go_genmove", new JsonObject { ["state"] = state, ["ms"] = GoMs(req.Level), ["level"] = Level(req.Level) }), ct);
            var captured = ApplyGoMove(s, gen["move"]!.GetValue<string>());
            if (s.State["passes"]!.GetValue<int>() >= 2) s.Over = true;
            var comment = gen["comment"]!.GetValue<string>();
            var ai = new JsonObject
            {
                ["move"] = gen["move"]!.GetValue<string>().ToUpperInvariant(),
                // 맹기 대국에서는 예상 진행(돌 위치 힌트)을 숨긴다
                ["comment"] = s.Blind != "off" ? StripPv(comment) : comment,
                ["winrate"] = gen["winrate"]!.DeepClone(),
                ["captured"] = captured,
            };
            var extra = GoExtra(s, null);
            extra["ai"] = ai;
            return Results.Json(s.View(extra));
        }));

        go.MapPost("/{id}/score", (string id, CancellationToken ct) => WithSession(store, id, GameKind.Go, async s =>
        {
            if (s.Score != null) return Results.Json(s.View());
            if (s.State["passes"]!.GetValue<int>() < 2) return Conflict("아직 종국되지 않았습니다 (양쪽 모두 패스해야 함)");
            var comment = await ScoreGo(s, ct);
            return Results.Json(s.View(new JsonObject { ["ai"] = new JsonObject { ["comment"] = comment } }));
        }));

        // 대국 끝내기: 패스를 주고받지 않아도 지금 판으로 바로 계가
        go.MapPost("/{id}/finish", (string id, CancellationToken ct) => WithSession(store, id, GameKind.Go, async s =>
        {
            if (s.Score != null) return Results.Json(s.View());
            if (s.Over) return Conflict("이미 끝난 대국입니다");
            var comment = "대국을 끝내고 지금 판으로 계가합니다. " + await ScoreGo(s, ct);
            return Results.Json(s.View(new JsonObject { ["ai"] = new JsonObject { ["comment"] = comment } }));
        }));

        go.MapPost("/{id}/undo", (string id) => WithSession(store, id, GameKind.Go, s => Undo(s)));

        go.MapPost("/{id}/resign", (string id) => WithSession(store, id, GameKind.Go, s =>
        {
            if (s.Over) return Conflict("이미 끝난 대국입니다");
            s.Over = true;
            s.Result = $"{ColorName(s.AiColor)}(AI) 불계승";
            return Results.Json(s.View());
        }));

        // 맹기 모드에서 판 엿보기 (횟수 기록)
        go.MapPost("/{id}/peek", (string id) => WithSession(store, id, GameKind.Go, s =>
        {
            if (s.Blind == "off") return Conflict("맹기 대국이 아닙니다");
            if (!s.Over) s.Peeks++;
            return Results.Json(new { board = s.State["board"]!.DeepClone(), peeks = s.Peeks });
        }));
    }

    /// <summary>사석을 판정하고 계가해 결과를 세션에 기록한다. 해설 문장을 돌려준다.</summary>
    private static async Task<string> ScoreGo(Session s, CancellationToken ct)
    {
        var state = s.State.DeepClone();
        var dead = await Heavy(() => Engine.Call("go_dead", new JsonObject { ["state"] = state, ["ms"] = 1500 }), ct);
        var deadList = dead["dead"]!.AsArray();
        var score = Engine.Call("go_score", new JsonObject { ["state"] = s.State.DeepClone(), ["dead"] = deadList.DeepClone() });
        score.Remove("ok");
        s.Score = score;
        s.Over = true;
        var bw = score["winner"]!.GetValue<string>() == "B" ? 1 : 2;
        s.Result = $"{ColorName(bw)}({Who(s, bw)}) {score["margin"]}집 승 — 흑 {score["black"]} : 백 {score["white"]}";
        return deadList.Count == 0
            ? "사석 없이 계가했습니다."
            : $"사석 {deadList.Count}개를 판정했습니다: {string.Join(", ", deadList.Select(d => d!.GetValue<string>()))}";
    }

    /// <summary>바둑 계열 공통 추가 정보: 사람이 방금 따낸 돌 + 단수 경고(맹기 중에는 생략)</summary>
    private static JsonObject GoExtra(Session s, JsonArray? captured)
    {
        var extra = new JsonObject();
        if (captured != null) extra["captured"] = captured;
        if (s.Blind == "off" && !s.Over)
            extra["ataris"] = Engine.Call("go_ataris", new JsonObject { ["state"] = s.State.DeepClone() })["ataris"]!.DeepClone();
        return extra;
    }

    private static JsonArray HumanGoMove(Session s, string move)
    {
        var before = s.State.DeepClone().AsObject();
        var captured = ApplyGoMove(s, move);
        s.Undo.Push(before);
        return captured;
    }

    private static JsonArray ApplyGoMove(Session s, string move)
    {
        var res = Engine.Call("go_play", new JsonObject { ["state"] = s.State.DeepClone(), ["move"] = move });
        s.State = res["state"]!.AsObject();
        return res["captured"]!.DeepClone().AsArray();
    }

    // ------------------------------------------------------------ 따내기 바둑

    private static void MapAtari(RouteGroupBuilder atari, SessionStore store)
    {
        atari.MapPost("/new", (NewAtariRequest req) => Guard(() =>
        {
            if (req.HumanColor is not (1 or 2)) return BadColor();
            if (req.Size is < 5 or > 13) return Results.BadRequest(new { error = "따내기 바둑 판은 5~13줄" });
            var state = Engine.Call("go_new", new JsonObject { ["size"] = req.Size, ["komi"] = 0 })["state"]!.AsObject();
            if (req.Cross)
            {
                // 가운데 십자 배치: 흑 (c,c)(c+1,c+1), 백 (c+1,c)(c,c+1) — 처음부터 싸움이 붙는다
                var n = req.Size;
                var c = n / 2 - (n % 2 == 0 ? 1 : 0);
                var board = state["board"]!.AsArray();
                board[c * n + c] = 1;
                board[(c + 1) * n + c + 1] = 1;
                board[c * n + c + 1] = 2;
                board[(c + 1) * n + c] = 2;
            }
            var s = Start(store, new Session(GameKind.AtariGo, state, req.HumanColor) { UserId = CurrentUser() });
            return Results.Json(s.View());
        }));

        atari.MapPost("/{id}/play", (string id, MoveRequest req) => WithSession(store, id, GameKind.AtariGo, s =>
        {
            if (s.Over) return Conflict("이미 끝난 게임입니다");
            if (s.ToPlay != s.HumanColor) return Conflict("AI 차례입니다");
            var captured = HumanGoMove(s, req.Move);
            CheckAtariEnd(s, s.HumanColor, captured);
            return Results.Json(s.View(GoExtra(s, captured)));
        }));

        atari.MapPost("/{id}/ai", (string id, AiRequest req, CancellationToken ct) => WithSession(store, id, GameKind.AtariGo, async s =>
        {
            if (s.Over) return Conflict("이미 끝난 게임입니다");
            if (s.ToPlay != s.AiColor) return Conflict("사람 차례입니다");
            var state = s.State.DeepClone();
            var gen = await Heavy(() => Engine.Call("atari_genmove", new JsonObject { ["state"] = state, ["ms"] = QuickMs(req.Level), ["level"] = Level(req.Level) }), ct);
            var captured = ApplyGoMove(s, gen["move"]!.GetValue<string>());
            CheckAtariEnd(s, s.AiColor, captured);
            var extra = GoExtra(s, null);
            extra["ai"] = new JsonObject
            {
                ["move"] = gen["move"]!.GetValue<string>().ToUpperInvariant(),
                ["comment"] = gen["comment"]!.DeepClone(),
                ["captured"] = captured,
            };
            return Results.Json(s.View(extra));
        }));

        atari.MapPost("/{id}/undo", (string id) => WithSession(store, id, GameKind.AtariGo, s => Undo(s)));
    }

    private static void CheckAtariEnd(Session s, int mover, JsonArray captured)
    {
        if (captured.Count > 0)
        {
            s.Over = true;
            s.Result = $"{ColorName(mover)}({Who(s, mover)}) 승리 — 돌 {captured.Count}개를 먼저 따냈습니다";
        }
        else if (s.State["passes"]!.GetValue<int>() >= 2)
        {
            s.Over = true;
            s.Result = "무승부 — 양쪽 모두 둘 곳이 없습니다";
        }
    }

    // ------------------------------------------------------------ 줄 잇기 (오목·펜테·육목)

    private static void MapLine(RouteGroupBuilder line, SessionStore store)
    {
        line.MapPost("/new", (NewLineRequest req) => Guard(() =>
        {
            if (req.HumanColor is not (1 or 2)) return BadColor();
            var args = new JsonObject { ["variant"] = req.Variant };
            if (req.Size is int size) args["size"] = size;
            var res = Engine.Call("line_new", args);
            var s = Start(store, new Session(GameKind.Line, res["state"]!.AsObject(), req.HumanColor) { UserId = CurrentUser() });
            return Results.Json(s.View());
        }));

        line.MapPost("/{id}/play", (string id, MoveRequest req) => WithSession(store, id, GameKind.Line, s =>
        {
            if (s.Over) return Conflict("이미 끝난 게임입니다");
            if (s.ToPlay != s.HumanColor) return Conflict("AI 차례입니다");
            var before = s.State.DeepClone().AsObject();
            var res = Engine.Call("line_play", new JsonObject { ["state"] = s.State.DeepClone(), ["move"] = req.Move });
            // 무르기는 사람 차례가 시작될 때의 상태로 (육목은 한 차례에 2개)
            if (before["placed_in_turn"]!.GetValue<int>() == 0) s.Undo.Push(before);
            s.State = res["state"]!.AsObject();
            CheckLineEnd(s);
            return Results.Json(s.View(new JsonObject { ["captured"] = res["captured"]!.DeepClone() }));
        }));

        line.MapPost("/{id}/ai", (string id, AiRequest req, CancellationToken ct) => WithSession(store, id, GameKind.Line, async s =>
        {
            if (s.Over) return Conflict("이미 끝난 게임입니다");
            if (s.ToPlay != s.AiColor) return Conflict("사람 차례입니다");
            var state = s.State.DeepClone();
            var gen = await Heavy(() => Engine.Call("line_genmove", new JsonObject { ["state"] = state, ["ms"] = QuickMs(req.Level), ["level"] = Level(req.Level) }), ct);
            var captured = new JsonArray();
            var moves = gen["moves"]!.AsArray().Select(m => m!.GetValue<string>()).ToList();
            foreach (var mv in moves)
            {
                var res = Engine.Call("line_play", new JsonObject { ["state"] = s.State.DeepClone(), ["move"] = mv });
                s.State = res["state"]!.AsObject();
                foreach (var c in res["captured"]!.AsArray()) captured.Add(c!.DeepClone());
                if (s.State["winner"] is JsonValue) break;
            }
            CheckLineEnd(s);
            // 다음 수로 승리 줄이 완성되는 자리를 알려 준다 — 화면을 보지 않는 사용자를 위해
            var threats = s.Over ? new JsonArray() : Engine.Call("line_info", new JsonObject { ["state"] = s.State.DeepClone() })["threats"]!.DeepClone();
            var ai = new JsonObject
            {
                ["move"] = string.Join(", ", moves),
                ["moves"] = new JsonArray(moves.Select(m => (JsonNode)m).ToArray()),
                ["comment"] = gen["comment"]!.DeepClone(),
                ["captured"] = captured,
                ["threats"] = threats,
            };
            return Results.Json(s.View(new JsonObject { ["ai"] = ai }));
        }));

        line.MapPost("/{id}/undo", (string id) => WithSession(store, id, GameKind.Line, s => Undo(s)));
    }

    private static void CheckLineEnd(Session s)
    {
        if (s.State["winner"] is not JsonValue w) return;
        var c = w.GetValue<int>();
        s.Over = true;
        var variant = s.State["variant"]!.GetValue<string>();
        var how = variant switch
        {
            "connect6" => "6목",
            "pente" when s.State["win_line"]!.AsArray().Count == 0 => "돌 10개 따내기",
            _ => "5목",
        };
        s.Result = c == 3 ? "무승부" : $"{ColorName(c)}({Who(s, c)}) {how} 승리";
    }

    // ------------------------------------------------------------ 사활 문제

    private static void MapTsumego(RouteGroupBuilder ts, SessionStore store)
    {
        // 문제 목록 + 이 계정이 푼 문제 표시
        ts.MapGet("/list", () =>
        {
            var solved = _records.List(CurrentUser()!)
                .Where(r => r["kind"]!.GetValue<string>() == nameof(GameKind.Tsumego) && r["outcome"]!.GetValue<string>() == "solved")
                .Select(r => r["problemId"]?.GetValue<string>())
                .ToHashSet();
            var list = Engine.Call("tsumego_list")["problems"]!.AsArray();
            foreach (var p in list) p!["solved"] = solved.Contains(p["id"]!.GetValue<string>());
            return Results.Json(list);
        });

        ts.MapPost("/new", (NewTsumegoRequest req) => Guard(() =>
        {
            var res = Engine.Call("tsumego_new", new JsonObject { ["id"] = req.Id });
            var s = new Session(GameKind.Tsumego, res["state"]!.AsObject(), humanColor: 1) { UserId = CurrentUser() };
            s.Meta["problem"] = res["problem"]!.DeepClone();
            s.Meta["status"] = "solving";
            Start(store, s);
            return Results.Json(s.View());
        }));

        ts.MapPost("/{id}/play", (string id, MoveRequest req, CancellationToken ct) => WithSession(store, id, GameKind.Tsumego, async s =>
        {
            if (s.Over) return Conflict("이 문제는 끝났습니다. 다시 풀기를 누르세요");
            var pid = s.Meta["problem"]!["id"]!.GetValue<string>();
            var captured = HumanGoMove(s, req.Move);
            var state = s.State.DeepClone();
            var reply = await Heavy(() => Engine.Call("tsumego_reply", new JsonObject { ["id"] = pid, ["state"] = state }), ct);
            var status = reply["status"]!.GetValue<string>();
            JsonArray? aiCaptured = null;
            if (reply["move"] is JsonValue mv)
                aiCaptured = ApplyGoMove(s, mv.GetValue<string>());
            s.Meta["status"] = status == "continue" ? "solving" : status;
            if (status != "continue")
            {
                s.Over = true;
                s.Result = status == "solved" ? "정답!" : "실패";
            }
            var extra = new JsonObject
            {
                ["captured"] = captured,
                ["ai"] = new JsonObject
                {
                    ["move"] = reply["move"]?.GetValue<string>().ToUpperInvariant(),
                    ["comment"] = reply["comment"]!.DeepClone(),
                    ["captured"] = aiCaptured,
                },
            };
            return Results.Json(s.View(extra));
        }));

        ts.MapPost("/{id}/hint", (string id, CancellationToken ct) => WithSession(store, id, GameKind.Tsumego, async s =>
        {
            if (s.Over) return Conflict("이 문제는 끝났습니다");
            var pid = s.Meta["problem"]!["id"]!.GetValue<string>();
            var state = s.State.DeepClone();
            var res = await Heavy(() => Engine.Call("tsumego_hint", new JsonObject { ["id"] = pid, ["state"] = state }), ct);
            return Results.Json(new { moves = res["moves"]!.DeepClone() });
        }));

        ts.MapPost("/{id}/retry", (string id) => WithSession(store, id, GameKind.Tsumego, s =>
        {
            var pid = s.Meta["problem"]!["id"]!.GetValue<string>();
            s.State = Engine.Call("tsumego_new", new JsonObject { ["id"] = pid })["state"]!.AsObject();
            s.Undo.Clear();
            s.Over = false;
            s.Result = null;
            s.Meta["status"] = "solving";
            return Results.Json(s.View());
        }));
    }

    // ------------------------------------------------------------ 공용

    private static IResult FinishNoResult(Session s, string extra)
    {
        if (s.Over) return Conflict("이미 끝난 대국입니다");
        s.Over = true;
        s.Result = "대국을 끝냈습니다 — 승부 없음";
        return Results.Json(s.View(new JsonObject { ["ai"] = new JsonObject { ["comment"] = ("대국을 끝냈습니다." + extra).Trim() } }));
    }

    private static string StripPv(string comment)
    {
        var i = comment.IndexOf(", 예상 진행:", StringComparison.Ordinal);
        return i < 0 ? comment : comment[..i] + ")";
    }

    private static IResult Undo(Session s)
    {
        if (s.Undo.Count == 0) return Conflict("되돌릴 수가 없습니다");
        s.State = s.Undo.Pop();
        s.Over = false;
        s.Result = null;
        s.Score = null;
        return Results.Json(s.View());
    }

    private static string ColorName(int c) => c == 1 ? "흑" : "백";
    private static string Who(Session s, int c) => c == s.HumanColor ? "나" : "AI";

    private static IResult BadColor() => Results.BadRequest(new { error = "humanColor는 1(흑) 또는 2(백)" });
    private static IResult Conflict(string msg) => Results.Json(new { error = msg }, statusCode: 409);

    private static IResult Guard(Func<IResult> f)
    {
        try { return f(); }
        catch (EngineException e) { return Results.BadRequest(new { error = e.Message }); }
    }

    private static Task<IResult> WithSession(SessionStore store, string id, GameKind kind, Func<Session, IResult> f) =>
        WithSessionAsync(store, id, kind, s => Task.FromResult(f(s)));

    private static Task<IResult> WithSession(SessionStore store, string id, GameKind kind, Func<Session, Task<IResult>> f) =>
        WithSessionAsync(store, id, kind, f);

    private static async Task<IResult> WithSessionAsync(SessionStore store, string id, GameKind kind, Func<Session, Task<IResult>> f)
    {
        var s = store.Get(id, kind, CurrentUser());
        if (s == null) return Results.NotFound(new { error = "게임을 찾을 수 없습니다. 새 게임을 시작하세요" });
        // 같은 판에 대한 요청(예: 사람 수 + AI 수)이 겹치지 않도록 직렬화
        await s.Lock.WaitAsync();
        try
        {
            var snapshot = s.State.DeepClone().AsObject();
            var undoDepth = s.Undo.Count;
            try
            {
                var result = await f(s);
                _records.Save(s); // 바뀐 대국을 계정 폴더에 기록
                return result;
            }
            catch (EngineException e)
            {
                s.State = snapshot;
                while (s.Undo.Count > undoDepth) s.Undo.Pop();
                return Results.BadRequest(new { error = e.Message });
            }
        }
        finally
        {
            s.Lock.Release();
        }
    }
}
