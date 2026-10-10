using System.Security.Cryptography;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace GoArcade;

/// <summary>플랫폼 데이터 폴더. 기본값은 저장소 루트의 data/ (환경변수 DATA_DIR로 바꿀 수 있음)</summary>
public sealed class DataPaths
{
    public string Root { get; }

    public DataPaths(IHostEnvironment env)
    {
        Root = Environment.GetEnvironmentVariable("DATA_DIR") is { Length: > 0 } dir
            ? Path.GetFullPath(dir)
            : Path.GetFullPath(Path.Combine(env.ContentRootPath, "..", "data"));
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

    public (Account? Account, string? Error) Register(string username, string password, string? displayName)
    {
        username = username.Trim();
        if (!Regex.IsMatch(username, @"^[A-Za-z0-9가-힣_]{3,20}$"))
            return (null, "아이디는 3~20자의 한글, 영문, 숫자, 밑줄(_)만 쓸 수 있습니다");
        if (password.Length < 6)
            return (null, "비밀번호는 6자 이상이어야 합니다");
        lock (_lock)
        {
            if (_accounts.Any(a => a.Provider == "local" && a.Username.Equals(username, StringComparison.OrdinalIgnoreCase)))
                return (null, "이미 있는 아이디입니다");
            var name = string.IsNullOrWhiteSpace(displayName) ? username : displayName.Trim()[..Math.Min(displayName.Trim().Length, 20)];
            var acc = new Account(NewId(), username, name, "local", HashPassword(password), null, null, DateTime.UtcNow);
            _accounts.Add(acc);
            Save();
            return (acc, null);
        }
    }

    public Account? VerifyPassword(string username, string password)
    {
        Account? acc;
        lock (_lock)
            acc = _accounts.FirstOrDefault(a => a.Provider == "local" && a.Username.Equals(username.Trim(), StringComparison.OrdinalIgnoreCase));
        return acc != null && acc.PasswordHash != null && CheckPassword(password, acc.PasswordHash) ? acc : null;
    }

    /// <summary>Google 계정(sub 기준)으로 찾거나, 처음이면 플랫폼 계정을 만든다</summary>
    public Account GetOrCreateGoogle(string sub, string? email, string? name)
    {
        lock (_lock)
        {
            var acc = _accounts.FirstOrDefault(a => a.Provider == "google" && a.GoogleSub == sub);
            if (acc != null) return acc;
            var display = string.IsNullOrWhiteSpace(name) ? (email ?? "Google 사용자") : name;
            acc = new Account(NewId(), email ?? sub, display, "google", null, sub, email, DateTime.UtcNow);
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
