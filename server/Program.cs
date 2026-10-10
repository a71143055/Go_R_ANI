using GoArcade;

// API 키 등은 저장소 루트(또는 server/)의 .env 파일에서 읽는다. 이미 설정된 환경변수가 우선한다.
DotEnv.Load(Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", ".env"));
DotEnv.Load(Path.Combine(Directory.GetCurrentDirectory(), ".env"));
DotEnv.Load(Path.Combine(Directory.GetCurrentDirectory(), "..", ".env"));

var builder = WebApplication.CreateBuilder(args);
builder.Services.AddSingleton<ClaudeAi>();
builder.Services.AddSingleton<SessionStore>();

var app = builder.Build();
app.UseDefaultFiles();
app.UseStaticFiles();
app.MapGames();

var claude = app.Services.GetRequiredService<ClaudeAi>();
app.Logger.LogInformation("AI 모델: {Model} | API 키: {Key}", claude.Model,
    claude.KeyConfigured ? "설정됨" : "없음 (.env에 ANTHROPIC_API_KEY를 넣으세요)");

app.Run();

static class DotEnv
{
    public static void Load(string path)
    {
        if (!File.Exists(path)) return;
        foreach (var raw in File.ReadAllLines(path))
        {
            var line = raw.Trim();
            if (line.Length == 0 || line.StartsWith('#')) continue;
            var eq = line.IndexOf('=');
            if (eq <= 0) continue;
            var key = line[..eq].Trim();
            var value = line[(eq + 1)..].Trim().Trim('"', '\'');
            if (string.IsNullOrEmpty(Environment.GetEnvironmentVariable(key)))
                Environment.SetEnvironmentVariable(key, value);
        }
    }
}
