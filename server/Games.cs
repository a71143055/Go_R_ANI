using System.Text.Json.Nodes;

namespace GoArcade;

/// <param name="Blind">맹기 모드: "off" | "human"(나만 판을 못 봄) | "both"(Claude도 수순만 보고 둠)</param>
public record NewGoRequest(int Size = 9, double Komi = 6.5, int HumanColor = 1, string Blind = "off");
public record NewOmokRequest(int HumanColor = 1);
public record NewAlkkagiRequest(int Count = 5);
public record MoveRequest(string Move);
public record ShootRequest(int Stone, double Angle, double Power);
public record AiRequest(string? Effort = null);

/// <summary>게임별 HTTP 엔드포인트. 규칙 판정은 Rust 엔진, 수 선택은 Claude가 맡는다.</summary>
public static class Games
{
    public static void MapGames(this WebApplication app)
    {
        var store = app.Services.GetRequiredService<SessionStore>();
        var claude = app.Services.GetRequiredService<ClaudeAi>();

        app.MapGet("/api/status", () => Results.Json(new { model = claude.Model, keyConfigured = claude.KeyConfigured }));

        // ------------------------------------------------------------ 바둑
        var go = app.MapGroup("/api/go");

        go.MapPost("/new", (NewGoRequest req) => Guard(() =>
        {
            if (req.HumanColor is not (1 or 2)) return Results.BadRequest(new { error = "humanColor는 1(흑) 또는 2(백)" });
            if (req.Blind is not ("off" or "human" or "both")) return Results.BadRequest(new { error = "blind는 off / human / both" });
            var res = Engine.Call("go_new", new JsonObject { ["size"] = req.Size, ["komi"] = req.Komi });
            var s = store.Add(new Session(GameKind.Go, res["state"]!.AsObject(), req.HumanColor) { Blind = req.Blind });
            return Results.Json(s.View());
        }));

        go.MapPost("/{id}/play", (string id, MoveRequest req) => WithSession(store, id, GameKind.Go, s =>
        {
            if (s.Over) return Conflict("이미 끝난 대국입니다");
            if (s.ToPlay != s.HumanColor) return Conflict("Claude 차례입니다");
            var before = s.State.DeepClone().AsObject();
            var res = Engine.Call("go_play", new JsonObject { ["state"] = s.State.DeepClone(), ["move"] = req.Move });
            s.Undo.Push(before);
            s.State = res["state"]!.AsObject();
            CheckGoEnd(s);
            return Results.Json(s.View());
        }));

        go.MapPost("/{id}/ai", (string id, AiRequest req, CancellationToken ct) => WithSession(store, id, GameKind.Go, async s =>
        {
            if (s.Over) return Conflict("이미 끝난 대국입니다");
            if (s.ToPlay != s.AiColor) return Conflict("사람 차례입니다");
            var info = Engine.Call("go_info", new JsonObject { ["state"] = s.State.DeepClone() });
            var st = s.State;
            var color = s.AiColor == 1 ? "흑(X)" : "백(O)";
            // Claude도 맹기: 판면과 착수 금지점을 주지 않고 전체 수순만 준다
            var prompt = s.Blind == "both" ? $"""
                [맹기 대국] 당신은 판을 볼 수 없습니다. 아래 전체 수순만으로 현재 판을 머릿속에 재구성하세요.
                (따낸 돌은 판에서 사라진다는 점, 패와 자충수 규칙을 잊지 마세요.)

                판 크기: {st["size"]}x{st["size"]}
                덤: {st["komi"]} (백에게)
                당신의 색: {color}
                따낸 돌: 흑 {st["captures"]![0]}개, 백 {st["captures"]![1]}개
                전체 수순: {FullHistory(st)}

                {color} 차례입니다. 최선의 수를 고르세요.
                """ : $"""
                판 크기: {st["size"]}x{st["size"]}
                덤: {st["komi"]} (백에게)
                수순 번호: {st["history"]!.AsArray().Count + 1}
                당신의 색: {color}
                따낸 돌: 흑 {st["captures"]![0]}개, 백 {st["captures"]![1]}개
                최근 수순(오래된 것 → 최근): {RecentHistory(st)}
                현재 착수 금지점(패/자충): {JoinOr(info["illegal"]!.AsArray(), "(없음)")}

                현재 판면:
                {info["ascii"]}

                {color} 차례입니다. 최선의 수를 고르세요.
                """;
            var ai = await PlayCandidates(s, claude, Prompts.GoSystem, prompt, req.Effort, "go_play", "go_fallback", ct);
            CheckGoEnd(s);
            return Results.Json(s.View(new JsonObject { ["ai"] = ai }));
        }));

        go.MapPost("/{id}/score", (string id, AiRequest req, CancellationToken ct) => WithSession(store, id, GameKind.Go, async s =>
        {
            if (s.Score != null) return Results.Json(s.View());
            if (s.State["passes"]!.GetValue<int>() < 2) return Conflict("아직 종국되지 않았습니다 (양쪽 모두 패스해야 함)");
            var st = s.State;
            var info = Engine.Call("go_info", new JsonObject { ["state"] = st.DeepClone() });
            var dead = new JsonArray();
            string? comment = null, error = null;
            try
            {
                var r = await claude.Ask(Prompts.GoScoreSystem, $"""
                    판 크기: {st["size"]}x{st["size"]}, 덤 {st["komi"]}
                    따낸 돌: 흑 {st["captures"]![0]}개, 백 {st["captures"]![1]}개

                    종국 판면:
                    {info["ascii"]}

                    사석을 판정하세요.
                    """, Prompts.ScoreSchema, req.Effort, ct);
                dead = r["dead"]?.AsArray() ?? dead;
                comment = r["comment"]?.GetValue<string>();
            }
            catch (ClaudeException e)
            {
                error = $"{e.Message} — 사석 없이 계가합니다";
            }
            var score = Engine.Call("go_score", new JsonObject { ["state"] = st.DeepClone(), ["dead"] = dead.DeepClone() });
            score.Remove("ok");
            s.Score = score;
            s.Over = true;
            var bw = score["winner"]!.GetValue<string>() == "B" ? 1 : 2;
            s.Result = $"{ColorName(bw)}({Who(s, bw)}) {score["margin"]}집 승 — 흑 {score["black"]} : 백 {score["white"]}";
            return Results.Json(s.View(new JsonObject { ["ai"] = new JsonObject { ["comment"] = comment, ["error"] = error } }));
        }));

        go.MapPost("/{id}/undo", (string id) => WithSession(store, id, GameKind.Go, s => Undo(s)));

        go.MapPost("/{id}/resign", (string id) => WithSession(store, id, GameKind.Go, s =>
        {
            if (s.Over) return Conflict("이미 끝난 대국입니다");
            s.Over = true;
            s.Result = $"{ColorName(s.AiColor)}(Claude) 불계승";
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
            if (s.ToPlay != s.HumanColor) return Conflict("Claude 차례입니다");
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
            var info = Engine.Call("omok_info", new JsonObject { ["state"] = s.State.DeepClone() });
            var color = s.AiColor == 1 ? "흑(X)" : "백(O)";
            var prompt = $"""
                당신의 색: {color}
                최근 수순(오래된 것 → 최근): {RecentHistory(s.State)}
                엔진이 찾은 긴급 지점: {JoinOr(info["threats"]!.AsArray(), "(없음)")}

                현재 판면:
                {info["ascii"]}

                {color} 차례입니다. 최선의 수를 고르세요.
                """;
            var ai = await PlayCandidates(s, claude, Prompts.OmokSystem, prompt, req.Effort ?? "low", "omok_play", "omok_fallback", ct);
            CheckOmokEnd(s);
            return Results.Json(s.View(new JsonObject { ["ai"] = ai }));
        }));

        omok.MapPost("/{id}/undo", (string id) => WithSession(store, id, GameKind.Omok, s => Undo(s)));

        // ------------------------------------------------------------ 알까기
        var alk = app.MapGroup("/api/alkkagi");

        alk.MapPost("/new", (NewAlkkagiRequest req) => Guard(() =>
        {
            var res = Engine.Call("alk_new", new JsonObject { ["count"] = req.Count });
            var s = store.Add(new Session(GameKind.Alkkagi, res["state"]!.AsObject(), humanColor: 1));
            return Results.Json(s.View());
        }));

        alk.MapPost("/{id}/shoot", (string id, ShootRequest req) => WithSession(store, id, GameKind.Alkkagi, s =>
        {
            if (s.Over) return Conflict("이미 끝난 게임입니다");
            if (s.ToPlay != s.HumanColor) return Conflict("Claude 차례입니다");
            var frames = Shoot(s, req.Stone, req.Angle, req.Power);
            return Results.Json(s.View(new JsonObject { ["frames"] = frames }));
        }));

        alk.MapPost("/{id}/ai", (string id, AiRequest req, CancellationToken ct) => WithSession(store, id, GameKind.Alkkagi, async s =>
        {
            if (s.Over) return Conflict("이미 끝난 게임입니다");
            if (s.ToPlay != s.AiColor) return Conflict("사람 차례입니다");
            var options = Engine.Call("alk_options", new JsonObject { ["state"] = s.State.DeepClone() })["options"]!.AsArray();
            if (options.Count == 0) return Conflict("쏠 수 있는 돌이 없습니다");

            JsonObject? chosen = null;
            double power = 0;
            string? comment = null, error = null;
            try
            {
                var prompt = $"""
                    내 돌(백): {StoneList(s.State, s.AiColor)}
                    상대 돌(흑): {StoneList(s.State, s.HumanColor)}

                    선택지:
                    {options.ToJsonString()}

                    가장 좋은 샷을 고르세요.
                    """;
                var r = await claude.Ask(Prompts.AlkkagiSystem, prompt, Prompts.AlkkagiSchema, req.Effort ?? "low", ct);
                var optId = r["option_id"]!.GetValue<int>();
                chosen = options.Select(o => o!.AsObject()).FirstOrDefault(o => o["id"]!.GetValue<int>() == optId);
                power = r["power"]!.GetValue<double>();
                comment = r["comment"]?.GetValue<string>();
                if (chosen == null) error = $"Claude가 없는 선택지({optId})를 골라 대체 샷을 씁니다";
            }
            catch (ClaudeException e)
            {
                error = $"{e.Message} — 대체 샷을 씁니다";
            }
            var fallback = chosen == null;
            if (chosen == null)
            {
                chosen = options[0]!.AsObject(); // 엔진이 막히지 않은 가까운 샷부터 정렬해 둠
                var need = chosen["distance"]!.GetValue<double>() + chosen["target_exit"]!.GetValue<double>();
                power = Math.Clamp(Math.Sqrt(need * 1.15 / 1.6), 0.3, 1.0);
            }
            var frames = Shoot(s, chosen["shooter"]!.GetValue<int>(), chosen["angle"]!.GetValue<double>(), power);
            var ai = new JsonObject
            {
                ["move"] = $"{chosen["shooter"]}번 돌 → {chosen["target"]}번 돌 (힘 {power:0.00})",
                ["comment"] = comment,
                ["fallback"] = fallback,
                ["error"] = error,
            };
            return Results.Json(s.View(new JsonObject { ["ai"] = ai, ["frames"] = frames }));
        }));
    }

    // ------------------------------------------------------------ 공용

    /// <summary>Claude 후보 수를 순서대로 엔진에 시도하고, 전부 실패하면 엔진 대체 수를 둔다.</summary>
    private static async Task<JsonObject> PlayCandidates(Session s, ClaudeAi claude, string system, string prompt,
        string? effort, string playCmd, string fallbackCmd, CancellationToken ct)
    {
        string? comment = null, error = null, played = null;
        var rejected = new List<string>();
        try
        {
            // 후보가 전부 둘 수 없는 자리면, 거절 사유를 알려 주고 한 번 더 묻는다
            for (var attempt = 0; attempt < 2 && played == null; attempt++)
            {
                var ask = attempt == 0 ? prompt :
                    prompt + $"\n\n주의: 방금 고른 수는 모두 둘 수 없었습니다 — {string.Join(", ", rejected)}. 다른 수를 고르세요.";
                var r = await claude.Ask(system, ask, Prompts.MoveSchema, effort, ct);
                comment = r["comment"]?.GetValue<string>();
                foreach (var c in r["candidates"]?.AsArray() ?? [])
                {
                    var mv = c?.GetValue<string>();
                    if (string.IsNullOrWhiteSpace(mv)) continue;
                    try
                    {
                        var res = Engine.Call(playCmd, new JsonObject { ["state"] = s.State.DeepClone(), ["move"] = mv });
                        s.State = res["state"]!.AsObject();
                        played = mv.Trim().ToUpperInvariant();
                        break;
                    }
                    catch (EngineException e)
                    {
                        rejected.Add(e.Message);
                    }
                }
            }
            if (played == null)
                error = "Claude의 후보 수가 모두 둘 수 없는 자리여서 대체 수를 둡니다" +
                        (rejected.Count > 0 ? $" ({string.Join(", ", rejected)})" : "");
        }
        catch (ClaudeException e)
        {
            error = $"{e.Message} — 엔진 대체 수를 둡니다";
        }

        var fallback = played == null;
        if (played == null)
        {
            var mv = Engine.Call(fallbackCmd, new JsonObject { ["state"] = s.State.DeepClone() })["move"]!.GetValue<string>();
            var res = Engine.Call(playCmd, new JsonObject { ["state"] = s.State.DeepClone(), ["move"] = mv });
            s.State = res["state"]!.AsObject();
            played = mv.ToUpperInvariant();
            comment = null;
        }
        return new JsonObject { ["move"] = played, ["comment"] = comment, ["fallback"] = fallback, ["error"] = error };
    }

    private static JsonArray Shoot(Session s, int stone, double angle, double power)
    {
        var res = Engine.Call("alk_shoot", new JsonObject
        {
            ["state"] = s.State.DeepClone(), ["stone"] = stone, ["angle"] = angle, ["power"] = power,
        });
        s.State = res["state"]!.AsObject();
        if (s.State["winner"] is JsonValue w)
        {
            var c = w.GetValue<int>();
            s.Over = true;
            s.Result = c == s.HumanColor ? "승리! 상대 돌을 모두 떨어뜨렸습니다" : "Claude 승리 — 내 돌이 모두 떨어졌습니다";
        }
        return res["frames"]!.DeepClone().AsArray();
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
    private static string Who(Session s, int c) => c == s.HumanColor ? "나" : "Claude";

    private static string FullHistory(JsonObject st)
    {
        var h = st["history"]!.AsArray().Select(x => x!.GetValue<string>()).ToList();
        return h.Count == 0 ? "(없음)" : string.Join(", ", h.Select((m, i) => $"{i + 1}.{m}"));
    }

    private static string RecentHistory(JsonObject st)
    {
        var h = st["history"]!.AsArray().Select(x => x!.GetValue<string>()).ToList();
        return h.Count == 0 ? "(없음)" : string.Join(", ", h.Skip(Math.Max(0, h.Count - 30)));
    }

    private static string JoinOr(JsonArray arr, string empty) =>
        arr.Count == 0 ? empty : string.Join(", ", arr.Select(x => x!.GetValue<string>()));

    private static string StoneList(JsonObject st, int color) =>
        new JsonArray(st["stones"]!.AsArray()
            .Select(x => x!.AsObject())
            .Where(x => x["alive"]!.GetValue<bool>() && x["color"]!.GetValue<int>() == color)
            .Select(x => (JsonNode)new JsonObject
            {
                ["id"] = x["id"]!.GetValue<int>(),
                ["x"] = Math.Round(x["x"]!.GetValue<double>(), 3),
                ["y"] = Math.Round(x["y"]!.GetValue<double>(), 3),
            }).ToArray()).ToJsonString();

    private static IResult Conflict(string msg) => Results.Json(new { error = msg }, statusCode: 409);

    private static IResult Guard(Func<IResult> f)
    {
        try { return f(); }
        catch (EngineException e) { return Results.BadRequest(new { error = e.Message }); }
    }

    private static Task<IResult> WithSession(SessionStore store, string id, GameKind kind, Func<Session, Task<IResult>> f) =>
        WithSessionAsync(store, id, kind, f);

    private static Task<IResult> WithSession(SessionStore store, string id, GameKind kind, Func<Session, IResult> f) =>
        WithSessionAsync(store, id, kind, s => Task.FromResult(f(s)));

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
