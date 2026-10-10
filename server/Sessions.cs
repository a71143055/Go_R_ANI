using System.Collections.Concurrent;
using System.Text.Json.Nodes;

namespace GoArcade;

public enum GameKind { Go, Omok, Alkkagi }

/// <summary>한 판의 게임. 상태 JSON은 Rust 엔진이 만들고 읽는다.</summary>
public sealed class Session(GameKind kind, JsonObject state, int humanColor)
{
    public string Id { get; } = Guid.NewGuid().ToString("N");
    public GameKind Kind { get; } = kind;
    public JsonObject State { get; set; } = state;
    public int HumanColor { get; } = humanColor;
    public bool Over { get; set; }
    public string? Result { get; set; }
    public JsonObject? Score { get; set; }
    /// <summary>사람이 수를 두기 직전의 상태들 (무르기용)</summary>
    public Stack<JsonObject> Undo { get; } = new();
    public SemaphoreSlim Lock { get; } = new(1, 1);
    /// <summary>바둑 맹기 모드: "off" | "human" | "both"</summary>
    public string Blind { get; init; } = "off";
    public int Peeks { get; set; }
    public DateTime Touched { get; set; } = DateTime.UtcNow;

    public int ToPlay => State["to_play"]!.GetValue<int>();
    public int AiColor => 3 - HumanColor;

    public JsonObject View(JsonObject? extra = null)
    {
        var state = State.DeepClone().AsObject();
        // 맹기 대국 중에는 판 내용을 클라이언트에 보내지 않는다 (수순·직전 수만 공개)
        var hidden = Blind != "off" && !Over;
        if (hidden)
            state["board"] = new JsonArray(Enumerable.Range(0, state["board"]!.AsArray().Count).Select(_ => (JsonNode)0).ToArray());
        var v = new JsonObject
        {
            ["id"] = Id,
            ["state"] = state,
            ["blind"] = Blind,
            ["hidden"] = hidden,
            ["peeks"] = Peeks,
            ["humanColor"] = HumanColor,
            ["over"] = Over,
            ["result"] = Result,
            ["score"] = Score?.DeepClone(),
            ["canUndo"] = Undo.Count > 0,
        };
        if (extra != null)
            foreach (var (k, val) in extra)
                v[k] = val?.DeepClone();
        return v;
    }
}

public sealed class SessionStore
{
    private readonly ConcurrentDictionary<string, Session> _sessions = new();

    public Session Add(Session s)
    {
        // 오래된 판 정리
        foreach (var (id, old) in _sessions)
            if (DateTime.UtcNow - old.Touched > TimeSpan.FromHours(6))
                _sessions.TryRemove(id, out _);
        _sessions[s.Id] = s;
        return s;
    }

    public Session? Get(string id, GameKind kind)
    {
        if (_sessions.TryGetValue(id, out var s) && s.Kind == kind)
        {
            s.Touched = DateTime.UtcNow;
            return s;
        }
        return null;
    }
}
