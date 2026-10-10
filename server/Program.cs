using GoArcade;

var builder = WebApplication.CreateBuilder(args);
builder.Services.AddSingleton<SessionStore>();

var app = builder.Build();
app.UseDefaultFiles();
app.UseStaticFiles();
app.MapGames();

var info = Engine.Call("engine_info");
app.Logger.LogInformation("Rust 엔진 로드 완료 — AI 탐색 스레드 {Threads}개", info["threads"]);

app.Run();
