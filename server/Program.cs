using GoArcade;

var builder = WebApplication.CreateBuilder(args);
builder.Services.AddSingleton<SessionStore>();

var app = builder.Build();
app.UseDefaultFiles();
app.UseStaticFiles();
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
app.Logger.LogInformation("Rust 엔진 로드 완료 — AI 탐색 스레드 {Threads}개", info["threads"]);

app.Run();
