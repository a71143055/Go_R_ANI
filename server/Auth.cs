using System.Security.Claims;
using System.Text.Json.Nodes;
using Microsoft.AspNetCore.Authentication;
using Microsoft.AspNetCore.Authentication.Cookies;

namespace GoArcade;

public record RegisterRequest(string Email, string Password, string? DisplayName = null);
public record LoginRequest(string Email, string Password);
public record GoogleLoginRequest(string Credential);

/// <summary>로그인(자체 계정 + Google)과 계정별 기록 API</summary>
public static class AuthApi
{
    /// <summary>Google 로그인용 OAuth 클라이언트 ID (없으면 Google 로그인 버튼을 숨긴다)</summary>
    private static string? GoogleClientId(IConfiguration cfg) =>
        Environment.GetEnvironmentVariable("GOOGLE_CLIENT_ID") is { Length: > 0 } id ? id : cfg["Google:ClientId"];

    private static async Task SignIn(HttpContext ctx, Account acc)
    {
        var claims = new[]
        {
            new Claim(ClaimTypes.NameIdentifier, acc.Id),
            new Claim(ClaimTypes.Name, acc.DisplayName),
        };
        var principal = new ClaimsPrincipal(new ClaimsIdentity(claims, CookieAuthenticationDefaults.AuthenticationScheme));
        await ctx.SignInAsync(CookieAuthenticationDefaults.AuthenticationScheme, principal,
            new AuthenticationProperties { IsPersistent = true, ExpiresUtc = DateTimeOffset.UtcNow.AddDays(30) });
    }

    private static object Me(Account acc) => new { id = acc.Id, email = acc.Email, displayName = acc.DisplayName, provider = acc.Provider };

    public static void MapAuth(this WebApplication app)
    {
        var accounts = app.Services.GetRequiredService<AccountStore>();
        var auth = app.MapGroup("/api/auth");

        auth.MapGet("/config", (IConfiguration cfg) => Results.Json(new { googleClientId = GoogleClientId(cfg) }));

        auth.MapGet("/me", (HttpContext ctx) =>
        {
            var id = ctx.User.FindFirstValue(ClaimTypes.NameIdentifier);
            var acc = id == null ? null : accounts.FindById(id);
            return acc == null ? Results.Json(new { error = "로그인이 필요합니다" }, statusCode: 401) : Results.Json(Me(acc));
        });

        auth.MapPost("/register", async (RegisterRequest req, HttpContext ctx) =>
        {
            var (acc, error) = accounts.Register(req.Email ?? "", req.Password ?? "", req.DisplayName);
            if (acc == null) return Results.BadRequest(new { error });
            await SignIn(ctx, acc);
            return Results.Json(Me(acc));
        });

        auth.MapPost("/login", async (LoginRequest req, HttpContext ctx) =>
        {
            var acc = accounts.VerifyPassword(req.Email ?? "", req.Password ?? "");
            if (acc == null)
            {
                await Task.Delay(500); // 무차별 대입을 늦춘다
                return Results.BadRequest(new { error = "이메일 또는 비밀번호가 맞지 않습니다" });
            }
            await SignIn(ctx, acc);
            return Results.Json(Me(acc));
        });

        auth.MapPost("/google", async (GoogleLoginRequest req, HttpContext ctx, IConfiguration cfg) =>
        {
            var clientId = GoogleClientId(cfg);
            if (clientId == null) return Results.BadRequest(new { error = "Google 로그인이 설정되지 않았습니다" });
            Google.Apis.Auth.GoogleJsonWebSignature.Payload payload;
            try
            {
                payload = await Google.Apis.Auth.GoogleJsonWebSignature.ValidateAsync(req.Credential,
                    new Google.Apis.Auth.GoogleJsonWebSignature.ValidationSettings { Audience = [clientId] });
            }
            catch (Exception)
            {
                return Results.BadRequest(new { error = "Google 로그인 정보를 확인하지 못했습니다" });
            }
            var acc = accounts.GetOrCreateGoogle(payload.Subject, payload.Email, payload.EmailVerified, payload.Name);
            await SignIn(ctx, acc);
            return Results.Json(Me(acc));
        });

        auth.MapPost("/logout", async (HttpContext ctx) =>
        {
            await ctx.SignOutAsync(CookieAuthenticationDefaults.AuthenticationScheme);
            return Results.Json(new { ok = true });
        });
    }

    public static void MapRecords(this WebApplication app)
    {
        var records = app.Services.GetRequiredService<RecordStore>();
        var store = app.Services.GetRequiredService<SessionStore>();
        var rec = app.MapGroup("/api/records").RequireAuthorization();

        rec.MapGet("/", () => Results.Json(records.List(Games.CurrentUser()!)));

        rec.MapDelete("/{id}", (string id) =>
            records.Delete(Games.CurrentUser()!, id) ? Results.Json(new { ok = true }) : Results.NotFound(new { error = "기록이 없습니다" }));

        // 끝나지 않은 대국 이어하기: 저장된 기록으로 대국을 되살린다
        rec.MapPost("/{id}/resume", (string id) =>
        {
            var user = Games.CurrentUser()!;
            var r = records.Load(user, id);
            if (r == null) return Results.NotFound(new { error = "기록이 없습니다" });
            if (r["status"]!.GetValue<string>() != "playing") return Results.Json(new { error = "이미 끝난 대국입니다" }, statusCode: 409);
            var kind = Enum.Parse<GameKind>(r["kind"]!.GetValue<string>());
            var s = new Session(kind, r["state"]!.DeepClone().AsObject(), r["humanColor"]!.GetValue<int>())
            {
                Id = id,
                UserId = user,
                Blind = r["blind"]?.GetValue<string>() ?? "off",
                Initial = r["initial"]!.DeepClone().AsObject(),
                Created = DateTime.Parse(r["created"]!.GetValue<string>()).ToUniversalTime(),
                Resumed = true,
            };
            s.Peeks = r["peeks"]?.GetValue<int>() ?? 0;
            foreach (var (k, v) in r["meta"]!.AsObject()) s.Meta[k] = v?.DeepClone();
            store.Add(s);
            var tab = kind switch
            {
                GameKind.Go => "go",
                GameKind.AtariGo => "atari",
                GameKind.Tsumego => "tsumego",
                _ => s.State["variant"]!.GetValue<string>(),
            };
            return Results.Json(new { tab, view = s.View() });
        });

        // 기보 다시 듣기: 시작 국면에서 한 수씩 다시 두어 각 수의 판을 만든다
        rec.MapGet("/{id}/replay", (string id) =>
        {
            var r = records.Load(Games.CurrentUser()!, id);
            if (r == null) return Results.NotFound(new { error = "기록이 없습니다" });
            var kind = Enum.Parse<GameKind>(r["kind"]!.GetValue<string>());
            var playCmd = kind == GameKind.Line ? "line_play" : "go_play";
            var state = r["initial"]!.DeepClone().AsObject();
            var size = state["size"]!.GetValue<int>();
            var frames = new JsonArray { new JsonObject { ["label"] = "시작", ["board"] = state["board"]!.DeepClone(), ["captured"] = new JsonArray() } };
            var history = r["state"]!["history"]!.AsArray().Select(h => h!.GetValue<string>()).ToList();
            for (var i = 0; i < history.Count; i++)
            {
                var parts = history[i].Split(' ');
                var color = parts[0] == "B" ? "흑" : "백";
                var move = parts.Length > 1 ? parts[1] : "pass";
                try
                {
                    var res = Engine.Call(playCmd, new JsonObject { ["state"] = state.DeepClone(), ["move"] = move });
                    state = res["state"]!.AsObject();
                    frames.Add(new JsonObject
                    {
                        ["label"] = $"{i + 1}수 {color} {(move == "pass" ? "패스" : move)}",
                        ["move"] = move,
                        ["board"] = state["board"]!.DeepClone(),
                        ["captured"] = res["captured"]!.DeepClone(),
                    });
                }
                catch (EngineException)
                {
                    break;
                }
            }
            return Results.Json(new { title = r["title"]!.DeepClone(), result = r["result"]?.DeepClone(), size, frames });
        });
    }
}
