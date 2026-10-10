using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

namespace GoArcade;

/// <summary>
/// 계정별 대국 기록 저장소: data/users/{계정 ID}/records/{대국 ID}.json
/// 대국이 바뀔 때마다 저장하므로, 서버를 껐다 켜도 끝나지 않은 대국을 이어서 둘 수 있다.
/// </summary>
public sealed class RecordStore(DataPaths paths)
{
    private readonly object _lock = new();

    private string Dir(string userId)
    {
        var dir = Path.Combine(paths.UserDir(userId), "records");
        Directory.CreateDirectory(dir);
        return dir;
    }

    private static bool ValidId(string id) => Regex.IsMatch(id, "^[a-f0-9]{32}$");

    public void Save(Session s)
    {
        if (s.UserId == null) return;
        var rec = new JsonObject
        {
            ["id"] = s.Id,
            ["kind"] = s.Kind.ToString(),
            ["title"] = Title(s),
            ["status"] = s.Over ? "finished" : "playing",
            ["outcome"] = Outcome(s),
            ["result"] = s.Result,
            ["humanColor"] = s.HumanColor,
            ["blind"] = s.Blind,
            ["peeks"] = s.Peeks,
            ["meta"] = s.Meta.DeepClone(),
            ["score"] = s.Score?.DeepClone(),
            ["initial"] = s.Initial.DeepClone(),
            ["state"] = s.State.DeepClone(),
            ["moves"] = s.State["history"]?.AsArray().Count ?? 0,
            ["created"] = s.Created.ToString("o"),
            ["updated"] = DateTime.UtcNow.ToString("o"),
        };
        lock (_lock)
            DataPaths.WriteAtomic(Path.Combine(Dir(s.UserId), s.Id + ".json"), rec.ToJsonString());
    }

    public JsonObject? Load(string userId, string id)
    {
        if (!ValidId(id)) return null;
        var path = Path.Combine(Dir(userId), id + ".json");
        lock (_lock)
            return File.Exists(path) ? JsonNode.Parse(File.ReadAllText(path))?.AsObject() : null;
    }

    public bool Delete(string userId, string id)
    {
        if (!ValidId(id)) return false;
        var path = Path.Combine(Dir(userId), id + ".json");
        lock (_lock)
        {
            if (!File.Exists(path)) return false;
            File.Delete(path);
            return true;
        }
    }

    /// <summary>목록용 요약 (판 상태는 빼고), 최근 것부터</summary>
    public List<JsonObject> List(string userId)
    {
        var list = new List<JsonObject>();
        lock (_lock)
        {
            foreach (var file in Directory.EnumerateFiles(Dir(userId), "*.json"))
            {
                try
                {
                    var r = JsonNode.Parse(File.ReadAllText(file))!.AsObject();
                    var meta = r["meta"]?.AsObject();
                    list.Add(new JsonObject
                    {
                        ["id"] = r["id"]!.DeepClone(),
                        ["kind"] = r["kind"]!.DeepClone(),
                        ["variant"] = r["state"]?["variant"]?.DeepClone(),
                        ["problemId"] = meta?["problem"]?["id"]?.DeepClone(),
                        ["title"] = r["title"]!.DeepClone(),
                        ["status"] = r["status"]!.DeepClone(),
                        ["outcome"] = r["outcome"]!.DeepClone(),
                        ["result"] = r["result"]?.DeepClone(),
                        ["humanColor"] = r["humanColor"]!.DeepClone(),
                        ["moves"] = r["moves"]!.DeepClone(),
                        ["created"] = r["created"]!.DeepClone(),
                        ["updated"] = r["updated"]!.DeepClone(),
                    });
                }
                catch (Exception)
                {
                    // 깨진 파일은 건너뛴다
                }
            }
        }
        return list.OrderByDescending(r => r["updated"]!.GetValue<string>()).ToList();
    }

    public static string Title(Session s) => s.Kind switch
    {
        GameKind.Go => $"바둑 {s.State["size"]}줄" + (s.Blind switch { "human" => " (맹기)", "strict" => " (완전 맹기)", _ => "" }),
        GameKind.AtariGo => $"따내기 바둑 {s.State["size"]}줄",
        GameKind.Line => s.State["variant"]!.GetValue<string>() switch { "pente" => "펜테", "connect6" => "육목", _ => "오목" },
        GameKind.Tsumego => $"사활 문제: {s.Meta["problem"]?["title"]}",
        _ => s.Kind.ToString(),
    };

    /// <summary>"win" | "loss" | "draw" | "none"(승부 없음) | "playing" | "solved" | "failed"</summary>
    public static string Outcome(Session s)
    {
        if (s.Kind == GameKind.Tsumego)
            return s.Meta["status"]?.GetValue<string>() switch { "solved" => "solved", "failed" => "failed", _ => "playing" };
        if (!s.Over) return "playing";
        var result = s.Result ?? "";
        if (result.Contains("시간승") || result.Contains("불계승")) return "loss";
        if (s.Score is { } score)
            return (score["winner"]!.GetValue<string>() == "B" ? 1 : 2) == s.HumanColor ? "win" : "loss";
        if (s.State["winner"] is JsonValue w)
        {
            var c = w.GetValue<int>();
            return c == 3 ? "draw" : c == s.HumanColor ? "win" : "loss";
        }
        if (result.StartsWith("무승부")) return "draw";
        if (result.Contains("승리")) return result.Contains("(나)") ? "win" : "loss";
        return "none";
    }
}
