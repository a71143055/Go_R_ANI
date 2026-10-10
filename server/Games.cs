using System.Text.Json.Nodes;

namespace GoArcade;

/// <param name="Blind">맹기 모드: "off" | "human"(돌을 숨기고 수순은 공개) | "strict"(돌과 수순 모두 숨김)</param>
public record NewGoRequest(int Size = 9, double Komi = 6.5, int HumanColor = 1, string Blind = "off");
public record NewOmokRequest(int HumanColor = 1);
public record MoveRequest(string Move);
/// <param name="Think">AI 생각 시간: "fast" | "normal" | "deep" | "max"</param>
public record AiRequest(string? Think = null);

/// <summary>게임별 HTTP 엔드포인트. 규칙 판정과 AI 모두 Rust 엔진이 맡는다.</summary>
public static class Games
{
    private static int GoMs(string? think) => think switch
    {
        "fast" => 1000, "deep" => 8000, "max" => 15000, _ => 3000,
    };

    private static int OmokMs(string? think) => think switch
    {
        "fast" => 300, "deep" => 3000, "max" => 6000, _ => 1000,
    };

    public static void MapGames(this WebApplication app)
    {
        var store = app.Services.GetRequiredService<SessionStore>();

        app.MapGet("/api/status", () =>
        {
            var info = Engine.Call("engine_info");
            return Results.Json(new { engine = "Rust 자체 AI", threads = info["threads"]!.GetValue<int>() });
        });

        // ------------------------------------------------------------ 바둑
        var go = app.MapGroup("/api/go");

        go.MapPost("/new", (NewGoRequest req) => Guard(() =>
        {
            if (req.HumanColor is not (1 or 2)) return Results.BadRequest(new { error = "humanColor는 1(흑) 또는 2(백)" });
            if (req.Blind is not ("off" or "human" or "strict")) return Results.BadRequest(new { error = "blind는 off / human / strict" });
            var res = Engine.Call("go_new", new JsonObject { ["size"] = req.Size, ["komi"] = req.Komi });
            var s = store.Add(new Session(GameKind.Go, res["state"]!.AsObject(), req.HumanColor) { Blind = req.Blind });
            return Results.Json(s.View());
        }));

        go.MapPost("/{id}/play", (string id, MoveRequest req) => WithSession(store, id, GameKind.Go, s =>
        {
            if (s.Over) return Conflict("이미 끝난 대국입니다");
            if (s.ToPlay != s.HumanColor) return Conflict("AI 차례입니다");
            var before = s.State.DeepClone().AsObject();
            var res = Engine.Call("go_play", new JsonObject { ["state"] = s.State.DeepClone(), ["move"] = req.Move });
            s.Undo.Push(before);
            s.State = res["state"]!.AsObject();
            CheckGoEnd(s);
            return Results.Json(s.View(new JsonObject { ["captured"] = res["captured"]!.DeepClone() }));
        }));

        go.MapPost("/{id}/ai", (string id, AiRequest req, CancellationToken ct) => WithSession(store, id, GameKind.Go, async s =>
        {
            if (s.Over) return Conflict("이미 끝난 대국입니다");
            if (s.ToPlay != s.AiColor) return Conflict("사람 차례입니다");
            var state = s.State.DeepClone();
            var gen = await Task.Run(() => Engine.Call("go_genmove", new JsonObject { ["state"] = state, ["ms"] = GoMs(req.Think) }), ct);
            var res = Engine.Call("go_play", new JsonObject { ["state"] = s.State.DeepClone(), ["move"] = gen["move"]!.DeepClone() });
            s.State = res["state"]!.AsObject();
            CheckGoEnd(s);
            var ai = new JsonObject
            {
                ["move"] = gen["move"]!.GetValue<string>().ToUpperInvariant(),
                ["comment"] = gen["comment"]!.DeepClone(),
                ["winrate"] = gen["winrate"]!.DeepClone(),
                ["playouts"] = gen["playouts"]!.DeepClone(),
                ["captured"] = res["captured"]!.DeepClone(),
            };
            // 맹기 대국에서는 예상 진행(돌 위치 힌트)을 숨긴다
            if (s.Blind != "off")
                ai["comment"] = StripPv(ai["comment"]!.GetValue<string>());
            return Results.Json(s.View(new JsonObject { ["ai"] = ai }));
        }));

        go.MapPost("/{id}/score", (string id, CancellationToken ct) => WithSession(store, id, GameKind.Go, async s =>
        {
            if (s.Score != null) return Results.Json(s.View());
            if (s.State["passes"]!.GetValue<int>() < 2) return Conflict("아직 종국되지 않았습니다 (양쪽 모두 패스해야 함)");
            var state = s.State.DeepClone();
            var dead = await Task.Run(() => Engine.Call("go_dead", new JsonObject { ["state"] = state, ["ms"] = 1500 }), ct);
            var deadList = dead["dead"]!.AsArray();
            var score = Engine.Call("go_score", new JsonObject { ["state"] = s.State.DeepClone(), ["dead"] = deadList.DeepClone() });
            score.Remove("ok");
            s.Score = score;
            s.Over = true;
            var bw = score["winner"]!.GetValue<string>() == "B" ? 1 : 2;
            s.Result = $"{ColorName(bw)}({Who(s, bw)}) {score["margin"]}집 승 — 흑 {score["black"]} : 백 {score["white"]}";
            var comment = deadList.Count == 0
                ? "사석 없이 계가했습니다."
                : $"사석 {deadList.Count}개를 판정했습니다: {string.Join(", ", deadList.Select(d => d!.GetValue<string>()))}";
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

        // ------------------------------------------------------------ 오목
        var omok = app.MapGroup("/api/omok");

        omok.MapPost("/new", (NewOmokRequest req) => Guard(() =>
        {
            if (req.HumanColor is not (1 or 2)) return Results.BadRequest(new { error = "humanColor는 1(흑) 또는 2(백)" });
            var res = Engine.Call("omok_new");
            var s = store.Add(new Session(GameKind.Omok, res["state"]!.AsObject(), req.HumanColor));
            return Results.Json(s.View());
        }));

        omok.MapPost("/{id}/play", (string id, MoveRequest req) => WithSession(store, id, GameKind.Omok, s =>
        {
            if (s.Over) return Conflict("이미 끝난 게임입니다");
            if (s.ToPlay != s.HumanColor) return Conflict("AI 차례입니다");
            var before = s.State.DeepClone().AsObject();
            var res = Engine.Call("omok_play", new JsonObject { ["state"] = s.State.DeepClone(), ["move"] = req.Move });
            s.Undo.Push(before);
            s.State = res["state"]!.AsObject();
            CheckOmokEnd(s);
            return Results.Json(s.View());
        }));

        omok.MapPost("/{id}/ai", (string id, AiRequest req, CancellationToken ct) => WithSession(store, id, GameKind.Omok, async s =>
        {
            if (s.Over) return Conflict("이미 끝난 게임입니다");
            if (s.ToPlay != s.AiColor) return Conflict("사람 차례입니다");
            var state = s.State.DeepClone();
            var gen = await Task.Run(() => Engine.Call("omok_genmove", new JsonObject { ["state"] = state, ["ms"] = OmokMs(req.Think) }), ct);
            var res = Engine.Call("omok_play", new JsonObject { ["state"] = s.State.DeepClone(), ["move"] = gen["move"]!.DeepClone() });
            s.State = res["state"]!.AsObject();
            CheckOmokEnd(s);
            // 다음 수로 5목이 되는 자리를 알려 준다 — 화면을 보지 않는 사용자를 위해
            var threats = s.Over ? new JsonArray() : Engine.Call("omok_info", new JsonObject { ["state"] = s.State.DeepClone() })["threats"]!.DeepClone();
            var ai = new JsonObject
            {
                ["move"] = gen["move"]!.GetValue<string>(),
                ["comment"] = gen["comment"]!.DeepClone(),
                ["threats"] = threats,
            };
            return Results.Json(s.View(new JsonObject { ["ai"] = ai }));
        }));

        omok.MapPost("/{id}/undo", (string id) => WithSession(store, id, GameKind.Omok, s => Undo(s)));
    }

    // ------------------------------------------------------------ 공용

    private static string StripPv(string comment)
    {
        var i = comment.IndexOf(", 예상 진행:", StringComparison.Ordinal);
        return i < 0 ? comment : comment[..i] + ")";
    }

    private static void CheckGoEnd(Session s)
    {
        if (s.State["passes"]!.GetValue<int>() >= 2)
            s.Over = true; // 결과는 /score에서 확정
    }

    private static void CheckOmokEnd(Session s)
    {
        if (s.State["winner"] is not JsonValue w) return;
        var c = w.GetValue<int>();
        s.Over = true;
        s.Result = c == 3 ? "무승부" : $"{ColorName(c)}({Who(s, c)}) 5목 승리";
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
        var s = store.Get(id, kind);
        if (s == null) return Results.NotFound(new { error = "게임을 찾을 수 없습니다. 새 게임을 시작하세요" });
        // 같은 판에 대한 요청(예: 사람 수 + AI 수)이 겹치지 않도록 직렬화
        await s.Lock.WaitAsync();
        try
        {
            var snapshot = s.State.DeepClone().AsObject();
            try { return await f(s); }
            catch (EngineException e)
            {
                s.State = snapshot;
                return Results.BadRequest(new { error = e.Message });
            }
        }
        finally
        {
            s.Lock.Release();
        }
    }
}
