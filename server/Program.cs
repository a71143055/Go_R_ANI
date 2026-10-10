using System.Threading.RateLimiting;
using GoArcade;
using Microsoft.AspNetCore.Authentication.Cookies;
using Microsoft.AspNetCore.HttpOverrides;

// 배포판: 실행 파일 옆에 wwwroot가 있으면 그 폴더를 기준으로 동작 (어디서 실행하든 화면 파일을 찾도록)
var baseDir = AppContext.BaseDirectory;
var builder = Directory.Exists(Path.Combine(baseDir, "wwwroot"))
    ? WebApplication.CreateBuilder(new WebApplicationOptions { Args = args, ContentRootPath = baseDir })
    : WebApplication.CreateBuilder(args);
builder.Services.AddSingleton<SessionStore>();
builder.Services.AddSingleton<DataPaths>();
builder.Services.AddSingleton<AccountStore>();
builder.Services.AddSingleton<RecordStore>();
builder.Services.AddHttpContextAccessor();

// 로그인: 쿠키 인증 (30일 유지). API는 로그인이 없으면 401을 돌려준다.
builder.Services.AddAuthentication(CookieAuthenticationDefaults.AuthenticationScheme)
    .AddCookie(o =>
    {
        o.Cookie.Name = "goarcade";
        o.Cookie.HttpOnly = true;
        o.Cookie.SameSite = SameSiteMode.Lax;
        o.SlidingExpiration = true;
        o.ExpireTimeSpan = TimeSpan.FromDays(30);
        o.Events.OnRedirectToLogin = ctx => { ctx.Response.StatusCode = 401; return Task.CompletedTask; };
        o.Events.OnRedirectToAccessDenied = ctx => { ctx.Response.StatusCode = 403; return Task.CompletedTask; };
    });
builder.Services.AddAuthorization();

// 로그인·회원가입은 접속지(IP)마다 1분에 10번까지 (비밀번호 대입 방지)
builder.Services.AddRateLimiter(o =>
{
    o.RejectionStatusCode = 429;
    o.OnRejected = async (ctx, ct) =>
        await ctx.HttpContext.Response.WriteAsJsonAsync(new { error = "요청이 너무 많습니다. 잠시 후 다시 시도하세요" }, ct);
    o.AddPolicy("auth", ctx => RateLimitPartition.GetFixedWindowLimiter(
        ctx.Connection.RemoteIpAddress?.ToString() ?? "unknown",
        _ => new FixedWindowRateLimiterOptions { PermitLimit = 10, Window = TimeSpan.FromMinutes(1) }));
});

var app = builder.Build();
// Cloudflare 터널 등 이 컴퓨터의 프록시를 거쳐 온 요청: 실제 접속자 IP와 HTTPS 여부를 반영
app.UseForwardedHeaders(new ForwardedHeadersOptions { ForwardedHeaders = ForwardedHeaders.XForwardedFor | ForwardedHeaders.XForwardedProto });
app.UseDefaultFiles();
app.UseStaticFiles();
app.UseAuthentication();
app.UseAuthorization();
app.UseRateLimiter();

app.MapAuth();
app.MapRecords();
app.MapGames();

// 앱 끝내기: 이 컴퓨터에서 온 요청만 받아 응답을 보낸 뒤 서버를 멈춘다
app.MapPost("/api/shutdown", (HttpContext ctx, IHostApplicationLifetime life) =>
{
    if (!AuthApi.IsLocalRequest(ctx))
        return Results.Json(new { error = "이 컴퓨터에서만 끝낼 수 있습니다" }, statusCode: 403);
    _ = Task.Run(async () =>
    {
        await Task.Delay(300);
        life.StopApplication();
    });
    return Results.Json(new { ok = true });
});

var info = Engine.Call("engine_info");
var data = app.Services.GetRequiredService<DataPaths>();
app.Logger.LogInformation("Rust 엔진 로드 완료 — AI 탐색 스레드 {Threads}개", info["threads"]);
app.Logger.LogInformation("플랫폼 데이터 폴더: {Dir}", data.Root);

app.Run();
