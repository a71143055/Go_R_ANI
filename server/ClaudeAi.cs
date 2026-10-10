using System.Text.Json;
using System.Text.Json.Nodes;
using Anthropic;
using Anthropic.Exceptions;
using Anthropic.Models.Beta.Messages;

namespace GoArcade;

public sealed class ClaudeException(string message) : Exception(message);

/// <summary>
/// Claude를 게임 AI로 쓰는 클라이언트. Claude 앱 로그인이 아니라 Anthropic API 키로 직접 호출한다.
/// 매 수는 상태 없는 단일 요청이며, 응답은 JSON 스키마(structured output)로 강제한다.
/// </summary>
public sealed class ClaudeAi
{
    public string Model { get; } = Environment.GetEnvironmentVariable("CLAUDE_MODEL") ?? "claude-opus-5-5";

    public bool KeyConfigured =>
        !string.IsNullOrEmpty(Environment.GetEnvironmentVariable("ANTHROPIC_API_KEY")) ||
        !string.IsNullOrEmpty(Environment.GetEnvironmentVariable("ANTHROPIC_AUTH_TOKEN"));

    private readonly Lazy<AnthropicClient> _client = new(() => new AnthropicClient());

    public async Task<JsonObject> Ask(string system, string prompt, JsonObject schema, string? effort, CancellationToken ct)
    {
        if (!KeyConfigured)
            throw new ClaudeException("ANTHROPIC_API_KEY가 설정되지 않았습니다 (.env 확인)");

        var schemaDict = schema.ToDictionary(kv => kv.Key, kv => JsonSerializer.SerializeToElement(kv.Value));
        BetaMessage resp;
        try
        {
            resp = await _client.Value.Beta.Messages.Create(new MessageCreateParams
            {
                Model = Model,
                MaxTokens = 16000,
                // 안전 분류기가 거절하면 서버 측에서 권장 대체 모델로 자동 재시도
                Betas = ["server-side-fallback-2026-07-01"],
                Fallbacks = new Default(), // "default": 거절 사유별 권장 모델로 라우팅
                System = system,
                Messages = [new() { Role = Role.User, Content = prompt }],
                OutputConfig = new BetaOutputConfig
                {
                    Effort = ParseEffort(effort),
                    Format = new BetaJsonOutputFormat { Schema = schemaDict },
                },
            }, ct);
        }
        catch (AnthropicUnauthorizedException)
        {
            throw new ClaudeException("API 키 인증 실패: ANTHROPIC_API_KEY를 확인하세요");
        }
        catch (AnthropicRateLimitException)
        {
            throw new ClaudeException("요청 한도 초과(429). 잠시 후 다시 시도하세요");
        }
        catch (AnthropicApiException e)
        {
            throw new ClaudeException($"Claude API 오류: {e.Message}");
        }
        catch (AnthropicIOException)
        {
            throw new ClaudeException("Claude API에 연결할 수 없습니다");
        }

        if (resp.StopReason == "refusal")
            throw new ClaudeException("Claude가 이 요청을 거절했습니다");
        if (resp.StopReason == "max_tokens")
            throw new ClaudeException("응답이 너무 길어 잘렸습니다");

        var text = string.Concat(resp.Content.Select(b => b.Value).OfType<BetaTextBlock>().Select(t => t.Text));
        try
        {
            return JsonNode.Parse(text)!.AsObject();
        }
        catch (Exception)
        {
            throw new ClaudeException("Claude 응답을 해석하지 못했습니다");
        }
    }

    private static Effort ParseEffort(string? e) => e switch
    {
        "low" => Effort.Low,
        "high" => Effort.High,
        "xhigh" => Effort.Xhigh,
        "max" => Effort.Max,
        _ => Effort.Medium,
    };
}
