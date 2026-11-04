//! 写出层脱敏（Redactor）。
//!
//! **不要**在每个 `tracing::info!` 调用点手动打码——一定会漏。
//! 统一在写出层做一次，诊断包生成时再跑一遍（双重保险），
//! 因为诊断包会额外收集游戏日志与命令行，那些内容不经过启动器的写出层。
//!
//! 禁止记录：密码、TOTP、完整 JWT、完整 refresh token、私钥、`.env` 内容、
//! 安装器完整命令行（含 token 参数时）。
//!
//! 必须记录：命令行的**结构**（程序路径、`-Xmx`、`--version`、`--gameDir`），
//! 但要跳过 `--accessToken`、`--clientToken`、`--uuid` 的值。

/// 需要脱敏的键名（小写比较）。命中的键，其后的值一律替换为 `***`。
const SENSITIVE_KEYS: &[&str] = &[
    "accesstoken",
    "access_token",
    "refresh_token",
    "refreshtoken",
    "client_token",
    "clienttoken",
    "api_key",
    "apikey",
    "authorization",
    "cookie",
    "set-cookie",
    "password",
    "passwd",
    "secret",
    "totp",
    "private_key",
    "uuid",
    "xuid",
    "uhs",
    "user_hash",
    "userhash",
    "rpsticket",
    "identitytoken",
];

/// 脱敏后的占位串。
const MASK: &str = "***";

/// 对单行文本做脱敏。
///
/// 处理顺序：
/// 1. 形如 `accessToken=xxx` / `"accessToken":"xxx"` 的键值对
/// 2. `Authorization: Bearer xxx` / `Bearer xxx`
/// 3. 邮箱
/// 4. 用户目录前缀
/// 5. 最后断言一次：不含 `eyJ`（JWT 头）
pub fn redact_line(line: &str) -> String {
    let mut out = redact_key_values(line);
    out = redact_bearer(&out);
    out = redact_emails(&out);
    out = redact_user_profile(&out);
    out = redact_jwt(&out);
    out
}

/// 形如 `key=value` / `key: value` / `"key":"value"` 的键值对。
fn redact_key_values(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let lower = line.to_ascii_lowercase();

    let mut cursor = 0usize;
    while cursor < line.len() {
        // 找到下一个敏感键的出现位置
        let mut hit: Option<(usize, &str)> = None;
        for key in SENSITIVE_KEYS {
            if let Some(pos) = lower[cursor..].find(key) {
                let abs = cursor + pos;
                if hit.is_none_or(|(best, _)| abs < best) {
                    hit = Some((abs, key));
                }
            }
        }

        let Some((key_start, key)) = hit else {
            result.push_str(&line[cursor..]);
            break;
        };

        result.push_str(&line[cursor..key_start]);

        // 键后面必须紧跟分隔符，否则不算键值对（例如 URL 里的普通单词）。
        // JSON 里键名外面还有一对引号：`"accessToken": "..."`，
        // 所以要先跳过空白与可选的收尾引号，再判断 `=` / `:`。
        let after_key = key_start + key.len();
        let sep_end = {
            let tail = &line[after_key..];
            let trimmed = tail.trim_start_matches([' ', '\t']);
            let leading = tail.len() - trimmed.len();
            let without_quote = trimmed.strip_prefix(['"', '\'']);
            let quote_len = if without_quote.is_some() { 1 } else { 0 };
            match without_quote.unwrap_or(trimmed).chars().next() {
                Some('=') | Some(':') => Some(after_key + leading + quote_len + 1),
                _ => None,
            }
        };

        let Some(value_start) = sep_end else {
            // 不是 `key=value` 形式。若这是命令行开关 `--flag value`，
            // 值是下一个空白分隔的 token——Minecraft 的 `--accessToken <值>`
            // 就是这种形态，必须遮掉。
            let is_cli_flag = key_start >= 2 && line[..key_start].ends_with("--");
            if is_cli_flag {
                let tail = &line[after_key..];
                let trimmed = tail.trim_start_matches([' ', '\t']);
                if trimmed.is_empty() {
                    result.push_str(&line[key_start..after_key]);
                    cursor = after_key;
                    continue;
                }
                let value_len = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
                let value_start = after_key + (tail.len() - trimmed.len());
                result.push_str(&line[key_start..value_start]);
                result.push_str(MASK);
                cursor = value_start + value_len;
                continue;
            }

            // 不是键值对，原样输出并继续
            result.push_str(&line[key_start..after_key]);
            cursor = after_key;
            continue;
        };

        // 值可能带引号，取到配对的结束引号为止
        let value_end = find_value_end(line, value_start);

        result.push_str(&line[key_start..value_start]);
        result.push_str(MASK);
        cursor = value_end;
    }

    result
}

/// 找到值的结束位置（返回下一个未消费字符的索引）。
fn find_value_end(line: &str, value_start: usize) -> usize {
    let bytes = line.as_bytes();
    let mut i = value_start;

    // 跳过分隔符后的空白
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }

    if i >= bytes.len() {
        return i;
    }

    if bytes[i] == b'"' || bytes[i] == b'\'' {
        let quote = bytes[i];
        i += 1;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == quote {
                return i + 1;
            }
            i += 1;
        }
        return i;
    }

    // 非引号值：取到空白或行尾
    while i < bytes.len() && bytes[i] != b' ' && bytes[i] != b'\t' {
        i += 1;
    }
    i
}

/// `Authorization: Bearer <token>` 与裸 `Bearer <token>`。
fn redact_bearer(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    let mut result = String::with_capacity(line.len());
    let mut cursor = 0usize;

    while let Some(pos) = lower[cursor..].find("bearer ") {
        let abs = cursor + pos;
        let token_start = abs + "bearer ".len();
        let token_end = line[token_start..]
            .find(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '"')
            .map(|i| token_start + i)
            .unwrap_or(line.len());

        result.push_str(&line[cursor..token_start]);
        result.push_str(MASK);
        cursor = token_end;
    }

    result.push_str(&line[cursor..]);
    result
}

/// 邮箱打码：`a***@example.com`。
fn redact_emails(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut result = String::with_capacity(line.len());
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i] == '@' {
            // 向前找本地部分，向后找域名
            let mut start = i;
            while start > 0 && is_email_local_char(chars[start - 1]) {
                start -= 1;
            }
            let mut end = i + 1;
            while end < chars.len() && is_email_domain_char(chars[end]) {
                end += 1;
            }

            let local: String = chars[start..i].iter().collect();
            let domain: String = chars[i..end].iter().collect();

            if !local.is_empty() && domain.contains('.') && local.chars().count() >= 2 {
                // `result` 此时与 `chars[..start]` 一一对应，回退到本地部分之前
                result = chars[..start].iter().collect();
                result.push(local.chars().next().unwrap());
                result.push_str(MASK);
                result.push_str(&domain);
                i = end;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}

fn is_email_local_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+')
}

fn is_email_domain_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '.' || c == '-'
}

/// 用户目录前缀 → `%USERPROFILE%`。
///
/// **注意 JSON 转义**：文件 sink 写出的是 JSON Lines，路径里的反斜杠已经被
/// 转义成 `\\`。因此这里同时替换**原始形式**与 **JSON 转义形式**，
/// 否则 JSON 输出里的路径永远脱敏不到。
fn redact_user_profile(line: &str) -> String {
    let Some(home) = dirs::home_dir() else {
        return line.to_string();
    };
    let home_str = home.display().to_string();
    if home_str.is_empty() {
        return line.to_string();
    }

    let mut result = line.replace(&home_str, "%USERPROFILE%");

    // JSON 转义形式：反斜杠翻倍
    let escaped_home = home_str.replace('\\', "\\\\");
    if escaped_home != home_str {
        result = result.replace(&escaped_home, "%USERPROFILE%");
    }

    // 兜底：任意盘符下的 Users\<用户名> 段（不限 C: 盘）
    crate::utils::paths::redact_user_segment(&result)
}

/// 最后一道断言：把疑似 JWT 的片段打码。
///
/// JWT 的 payload 以 `eyJ` 开头。日志里不应该出现完整 JWT，
/// 出现就说明上游把 token 塞进了别的地方。
fn redact_jwt(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut cursor = 0usize;

    while let Some(pos) = line[cursor..].find("eyJ") {
        let abs = cursor + pos;
        result.push_str(&line[cursor..abs]);
        result.push_str(MASK);

        // 跳过 JWT 本体：base64url 字符集，连续 3 段（签名段必须一起吞掉）
        let mut end = abs;
        let mut dots = 0;
        while end < line.len() {
            let c = line.as_bytes()[end];
            if c == b'.' {
                dots += 1;
                end += 1;
                if dots >= 2 {
                    while end < line.len() {
                        let b = line.as_bytes()[end];
                        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
                            end += 1;
                        } else {
                            break;
                        }
                    }
                    break;
                }
                continue;
            }
            let ok = c.is_ascii_alphanumeric() || c == b'-' || c == b'_';
            if !ok {
                break;
            }
            end += 1;
        }
        cursor = end;
    }

    result.push_str(&line[cursor..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_access_token_pair() {
        let out = redact_line("--accessToken abc.def.ghi --version 1.21.1");
        assert!(!out.contains("abc.def.ghi"), "token 未脱敏: {out}");
        assert!(out.contains("--version 1.21.1"), "结构信息丢失: {out}");
    }

    #[test]
    fn masks_cli_flag_values() {
        // Minecraft 启动参数是 `--flag <值>` 的空格分隔形态
        let out = redact_line("--accessToken TOKEN --clientToken CT --uuid 1234 --gameDir D:/game");
        assert!(!out.contains("TOKEN"), "accessToken 未脱敏: {out}");
        assert!(!out.contains("CT "), "clientToken 未脱敏: {out}");
        assert!(!out.contains("1234"), "uuid 未脱敏: {out}");
        assert!(out.contains("--gameDir D:/game"), "结构信息丢失: {out}");
    }

    #[test]
    fn masks_equals_separated_pairs() {
        let out = redact_line("accessToken=abc123&uuid=xyz");
        assert!(!out.contains("abc123"), "accessToken 未脱敏: {out}");
        assert!(!out.contains("xyz"), "uuid 未脱敏: {out}");
    }

    #[test]
    fn masks_bearer_header() {
        let out = redact_line("Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload.sig");
        assert!(!out.contains("eyJhbGciOiJIUzI1NiJ9"), "JWT 未脱敏: {out}");
    }

    #[test]
    fn masks_email() {
        let out = redact_line("login as alice@example.com done");
        assert!(!out.contains("alice@example.com"), "邮箱未脱敏: {out}");
        assert!(out.contains("a***@example.com"), "邮箱格式不符: {out}");
    }

    #[test]
    fn masks_api_key_fields() {
        // 通用的 `api_key` 键名仍要脱敏：它是脱敏器的兜底规则，
        // 不针对某个具体服务（第三方 API Key 也可能以这个名字出现）。
        let out = redact_line("api_key: super-secret-value");
        assert!(!out.contains("super-secret-value"), "api key 未脱敏: {out}");
    }

    #[test]
    fn masks_user_profile_in_raw_and_json_escaped_form() {
        let home = dirs::home_dir().unwrap();
        let home_str = home.display().to_string();

        let raw = format!("state dir: {home_str}\\AppData\\Roaming\\Sereno");
        let out = redact_line(&raw);
        assert!(!out.contains(&home_str), "原始路径未脱敏: {out}");

        // JSON Lines 里的路径，反斜杠被转义成双反斜杠
        let escaped = raw.replace('\\', "\\\\");
        let out = redact_line(&escaped);
        assert!(!out.contains(&home_str), "JSON 转义路径未脱敏: {out}");
        assert!(out.contains("%USERPROFILE%"), "未替换为占位符: {out}");
    }

    #[test]
    fn masks_user_segment_on_any_drive() {
        for raw in [
            r"path: C:\Users\alice\Games",
            r"path: D:\Users\bob\Games",
            r"path: C:\\Users\\carol\\Games",
        ] {
            let out = redact_line(raw);
            assert!(!out.contains("alice"), "未脱敏: {out}");
            assert!(!out.contains("bob"), "未脱敏: {out}");
            assert!(!out.contains("carol"), "未脱敏: {out}");
            assert!(out.contains("<user>"), "未替换为占位符: {out}");
        }
    }

    #[test]
    fn masks_microsoft_account_fields() {
        for raw in [
            "refresh_token=abc123&access_token=def456",
            r#"{"accessToken":"def456","xuid":"2535411234567890"}"#,
            "--xuid 2535411234567890 --accessToken def456",
            "uhs=1234567890123456789",
        ] {
            let out = redact_line(raw);
            assert!(!out.contains("abc123"), "refresh token 未脱敏: {out}");
            assert!(!out.contains("def456"), "access token 未脱敏: {out}");
            assert!(!out.contains("2535411234567890"), "XUID 未脱敏: {out}");
            assert!(!out.contains("1234567890123456789"), "uhs 未脱敏: {out}");
        }
    }

    #[test]
    fn keeps_benign_text() {
        let out = redact_line("下载会话 session-1 开始，共 42 个文件");
        assert_eq!(out, "下载会话 session-1 开始，共 42 个文件");
    }
}
