using System.Security.Cryptography;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace GoArcade;

/// <summary>
/// 플랫폼 데이터 폴더. 개발 중에는 저장소 루트의 data/, 배포판에서는 실행 폴더의 data/.
/// 환경변수 DATA_DIR로 바꿀 수 있다.
/// </summary>
public sealed class DataPaths
{
    public string Root { get; }

    public DataPaths(IHostEnvironment env)
    {
        var dev = File.Exists(Path.Combine(env.ContentRootPath, "GoArcade.csproj"));
        Root = Environment.GetEnvironmentVariable("DATA_DIR") is { Length: > 0 } dir
            ? Path.GetFullPath(dir)
            : Path.GetFullPath(dev ? Path.Combine(env.ContentRootPath, "..", "data") : Path.Combine(env.ContentRootPath, "data"));
        Directory.CreateDirectory(Root);
    }

    public string AccountsFile => Path.Combine(Root, "accounts.json");

    /// <summary>계정별 폴더 (data/users/{계정 ID})</summary>
    public string UserDir(string userId)
    {
        if (!Regex.IsMatch(userId, "^[a-f0-9]{32}$")) throw new ArgumentException("잘못된 계정 ID");
        var dir = Path.Combine(Root, "users", userId);
        Directory.CreateDirectory(dir);
        return dir;
    }

    /// <summary>임시 파일에 쓴 뒤 바꿔치기해서, 쓰는 도중 꺼져도 파일이 깨지지 않게 한다</summary>
    public static void WriteAtomic(string path, string content)
    {
        var tmp = path + ".tmp";
        File.WriteAllText(tmp, content);
        File.Move(tmp, path, overwrite: true);
    }
}

public sealed record Account(
    string Id,
    /// <summary>로그인 아이디 = 이메일 (소문자)</summary>
    string Username,
    string DisplayName,
    /// <summary>"local" | "google"</summary>
    string Provider,
    string? PasswordHash,
    string? GoogleSub,
    string? Email,
    DateTime Created);

/// <summary>계정 저장소 (data/accounts.json). 비밀번호는 PBKDF2 해시로만 저장한다.</summary>
public sealed class AccountStore
{
    private readonly DataPaths _paths;
    private readonly object _lock = new();
    private readonly List<Account> _accounts;
    private static readonly JsonSerializerOptions Json = new() { WriteIndented = true };

    public AccountStore(DataPaths paths)
    {
        _paths = paths;
        _accounts = File.Exists(paths.AccountsFile)
            ? JsonSerializer.Deserialize<List<Account>>(File.ReadAllText(paths.AccountsFile)) ?? []
            : [];
    }

    public Account? FindById(string id)
    {
        lock (_lock) return _accounts.FirstOrDefault(a => a.Id == id);
    }

    /// <summary>이메일 정규화: 앞뒤 공백 제거 + 소문자. 형식이 틀리면 null</summary>
    public static string? NormalizeEmail(string? email)
    {
        var e = (email ?? "").Trim().ToLowerInvariant();
        return e.Length <= 254 && Regex.IsMatch(e, @"^[a-z0-9._%+\-]+@[a-z0-9\-]+(\.[a-z0-9\-]+)*\.[a-z]{2,}$") ? e : null;
    }

    private Account? FindByEmail(string email) => _accounts.FirstOrDefault(a => a.Username == email);

    public (Account? Account, string? Error) Register(string email, string password, string? displayName)
    {
        var e = NormalizeEmail(email);
        if (e == null)
            return (null, "이메일 형식이 맞지 않습니다. 예: name@example.com");
        if (password.Length < 6)
            return (null, "비밀번호는 6자 이상이어야 합니다");
        lock (_lock)
        {
            var existing = FindByEmail(e);
            if (existing != null)
                return (null, existing.PasswordHash == null
                    ? "이 이메일은 Google 로그인으로 가입되어 있습니다. Google로 로그인하세요"
                    : "이미 가입된 이메일입니다");
            var name = string.IsNullOrWhiteSpace(displayName) ? e.Split('@')[0] : displayName.Trim()[..Math.Min(displayName.Trim().Length, 20)];
            var acc = new Account(NewId(), e, name, "local", HashPassword(password), null, e, DateTime.UtcNow);
            _accounts.Add(acc);
            Save();
            return (acc, null);
        }
    }

    public Account? VerifyPassword(string email, string password)
    {
        var e = NormalizeEmail(email);
        if (e == null) return null;
        Account? acc;
        lock (_lock) acc = FindByEmail(e);
        return acc != null && acc.PasswordHash != null && CheckPassword(password, acc.PasswordHash) ? acc : null;
    }

    /// <summary>
    /// Google 계정으로 찾는다. 처음이면, Google이 인증한 이메일과 같은 계정이 있으면 그 계정에 연결하고,
    /// 없으면 새 플랫폼 계정을 만든다 — 이메일 하나에 계정 하나.
    /// </summary>
    public Account GetOrCreateGoogle(string sub, string? email, bool emailVerified, string? name)
    {
        lock (_lock)
        {
            var acc = _accounts.FirstOrDefault(a => a.GoogleSub == sub);
            if (acc != null) return acc;
            var e = emailVerified ? NormalizeEmail(email) : null;
            if (e != null && FindByEmail(e) is { } same)
            {
                var linked = same with { GoogleSub = sub };
                _accounts[_accounts.IndexOf(same)] = linked;
                Save();
                return linked;
            }
            var display = string.IsNullOrWhiteSpace(name) ? (e ?? "Google 사용자") : name;
            acc = new Account(NewId(), e ?? $"google:{sub}", display, "google", null, sub, e, DateTime.UtcNow);
            _accounts.Add(acc);
            Save();
            return acc;
        }
    }

    private void Save() => DataPaths.WriteAtomic(_paths.AccountsFile, JsonSerializer.Serialize(_accounts, Json));

    private static string NewId() => Guid.NewGuid().ToString("N");

    private const int Iterations = 100_000;

    private static string HashPassword(string password)
    {
        var salt = RandomNumberGenerator.GetBytes(16);
        var hash = Rfc2898DeriveBytes.Pbkdf2(password, salt, Iterations, HashAlgorithmName.SHA256, 32);
        return $"pbkdf2${Iterations}${Convert.ToBase64String(salt)}${Convert.ToBase64String(hash)}";
    }

    private static bool CheckPassword(string password, string stored)
    {
        var parts = stored.Split('$');
        if (parts.Length != 4 || parts[0] != "pbkdf2") return false;
        var iter = int.Parse(parts[1]);
        var salt = Convert.FromBase64String(parts[2]);
        var expected = Convert.FromBase64String(parts[3]);
        var actual = Rfc2898DeriveBytes.Pbkdf2(password, salt, iter, HashAlgorithmName.SHA256, expected.Length);
        return CryptographicOperations.FixedTimeEquals(actual, expected);
    }
}
