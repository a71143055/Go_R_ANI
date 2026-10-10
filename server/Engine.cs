using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json.Nodes;

namespace GoArcade;

public sealed class EngineException(string message) : Exception(message);

/// <summary>
/// Rust 엔진(baduk_engine) P/Invoke 래퍼. 요청/응답은 모두 JSON 문자열이다.
/// </summary>
public static class Engine
{
    private const string Lib = "baduk_engine";

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr engine_call(byte[] request);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    private static extern void engine_free(IntPtr p);

    public static JsonObject Call(string cmd, JsonObject? args = null)
    {
        var req = args?.DeepClone().AsObject() ?? new JsonObject();
        req["cmd"] = cmd;
        var bytes = Encoding.UTF8.GetBytes(req.ToJsonString() + "\0");
        var p = engine_call(bytes);
        try
        {
            var text = Marshal.PtrToStringUTF8(p) ?? throw new EngineException("엔진 응답 없음");
            var res = JsonNode.Parse(text)!.AsObject();
            if (res["ok"]?.GetValue<bool>() != true)
                throw new EngineException(res["error"]?.GetValue<string>() ?? "엔진 오류");
            return res;
        }
        finally
        {
            engine_free(p);
        }
    }
}
