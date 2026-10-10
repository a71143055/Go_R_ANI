using GoArcade;
using Microsoft.AspNetCore.Authentication.Cookies;

var builder = WebApplication.CreateBuilder(args);
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

var app = builder.Build();
app.UseDefaultFiles();
app.UseStaticFiles();
app.UseAuthentication();
app.UseAuthorization();

app.MapAuth();
app.MapRecords();
app.MapGames();

// 앱 끝내기: 이 컴퓨터에서 온 요청만 받아 응답을 보낸 뒤 서버를 멈춘다
app.MapPost("/api/shutdown", (HttpContext ctx, IHostApplicationLifetime life) =>
{
    var ip = ctx.Connection.RemoteIpAddress;
    if (ip == null || !System.Net.IPAddress.IsLoopback(ip))
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
