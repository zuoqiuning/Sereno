//! 房间码：联机模块的用户入口。
//!
//! 房间码是**用户唯一需要交换的东西**（口头、QQ、微信发一串字符即可），
//! 因此设计目标按优先级排列：
//!
//! 1. **抗误读**：剔除易混淆的 `I`（像 1）与 `O`（像 0）；用户手输时
//!    `I→1`、`O→0`、小写→大写、缺分隔符都能自动纠偏。
//! 2. **抗手滑**：末位是校验位，敲错一位能立刻发现，而不是等到
//!    「房间不存在」才暴露。
//! 3. **零注册**：房间码本身派生组网凭证（network-name / secret），
//!    不需要任何中心服务器注册房间。
//!
//! 格式：`U/XXXX-XXXX-XXXX-XXXX`，16 个有效字符（Base-34），
//! 其中最后一个字符是前 15 个字符的加权校验和。
//!
//! **与 EasyTier 的关系**：房间码 → SHA-256（domain 分离）→
//! `network-name` + `network-secret`。知道房间码的人才能算出凭证，
//! 因此房间码本身就是访问凭据，**不要写进任何公开频道**。

use sha2::{Digest, Sha256};

/// Base-34 字母表：`0-9` + 去掉 `I`/`O` 的 `A-Z`。
const ALPHABET: &[u8; 34] = b"0123456789ABCDEFGHJKLMNPQRSTUVWXYZ";

/// 有效字符数（含末位校验位）。
const BODY_LEN: usize = 16;

/// 用户可见的分组长度。
const GROUP_LEN: usize = 4;

/// 前缀：所有 Sereno 房间码以 `U/` 开头，肉眼可辨「这是房间码」。
const PREFIX: &str = "U/";

/// 房间码错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RoomCodeError {
    /// 长度不对。
    #[error("房间码长度不正确")]
    BadLength,
    /// 含无法识别的字符（纠偏后仍然不是合法 Base-34）。
    #[error("房间码含有无法识别的字符")]
    BadCharacter(char),
    /// 校验位不匹配（大概率敲错了一位）。
    #[error("房间码校验失败，请检查是否抄错")]
    ChecksumMismatch,
}

/// 单字符 → Base-34 数值。
fn char_value(c: u8) -> Option<u8> {
    ALPHABET.iter().position(|&a| a == c).map(|p| p as u8)
}

/// 数值 → Base-34 字符。
fn value_char(v: u8) -> u8 {
    ALPHABET[v as usize]
}

/// 加权校验和：`sum(char_value * (index + 1)) mod 34`。
///
/// 位置权重能让「相邻两位互换」这种常见手误也被校验出来。
fn checksum(body: &[u8]) -> u8 {
    let mut sum: u32 = 0;
    for (i, &c) in body.iter().enumerate() {
        let v = char_value(c).unwrap_or(0) as u32;
        sum += v * (i as u32 + 1);
    }
    (sum % ALPHABET.len() as u32) as u8
}

/// 把用户输入规整成纯正文（大写、去分隔符、`I→1`、`O→0`、去掉前缀）。
///
/// 同时接受完整格式（`U/...`）与裸正文（16 个字符）。
#[must_use]
pub fn normalize(raw: &str) -> String {
    // 只保留字母数字与前缀斜杠；`-`、空格等分隔符直接丢弃
    let upper: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '/')
        .map(|c| c.to_ascii_uppercase())
        .map(|c| match c {
            'I' => '1',
            'O' => '0',
            other => other,
        })
        .collect();

    // 前缀可选，且裸正文本身也可能以 `U` 开头：
    // 两种解释里选「去掉前缀后长度为 16」的那个；都不满足则按原样返回
    match upper.strip_prefix(PREFIX) {
        Some(rest) if rest.chars().count() == BODY_LEN => rest.to_string(),
        _ if upper.chars().count() == BODY_LEN => upper,
        Some(rest) => rest.to_string(),
        None => upper,
    }
}

/// 校验并解析**已规整**的正文，返回 16 个有效字符。
fn validate_body(body: &str) -> Result<String, RoomCodeError> {
    let bytes = body.as_bytes();
    if bytes.len() != BODY_LEN {
        return Err(RoomCodeError::BadLength);
    }
    for &c in bytes {
        if char_value(c).is_none() {
            return Err(RoomCodeError::BadCharacter(c as char));
        }
    }
    let (payload, expected) = body.split_at(BODY_LEN - 1);
    let actual = value_char(checksum(payload.as_bytes()));
    if actual != expected.as_bytes()[0] {
        return Err(RoomCodeError::ChecksumMismatch);
    }
    Ok(body.to_string())
}

/// 校验用户输入的房间码（自动纠偏），成功返回规范格式。
///
/// ```
/// use sereno_core::multiplayer::room_code;
///
/// // 混淆输入：小写、I/O 混淆、缺分隔符都能还原成规范格式
/// let real = room_code::generate();               // U/XXXX-XXXX-XXXX-XXXX
/// let body = real.trim_start_matches("U/").replace('-', "").to_lowercase();
/// let parsed = room_code::parse(&body).unwrap();
/// assert_eq!(parsed, real);
/// assert!(parsed.starts_with("U/"));
/// ```
pub fn parse(raw: &str) -> Result<String, RoomCodeError> {
    let body = normalize(raw);
    let valid = validate_body(&body)?;
    Ok(format!("{PREFIX}{}", group(&valid)))
}

/// 生成一个新房间码（规范格式，含校验位）。
///
/// 随机源是 OS 级 CSPRNG（`rand::rng`），不带任何机器特征。
#[must_use]
pub fn generate() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();

    loop {
        // 前 15 位随机，第 16 位放校验和
        let mut body = [0u8; BODY_LEN - 1];
        for slot in body.iter_mut() {
            let v: u8 = rng.random_range(0..ALPHABET.len() as u8);
            *slot = value_char(v);
        }
        let cs = value_char(checksum(&body));
        let full = {
            let mut v = Vec::with_capacity(BODY_LEN);
            v.extend_from_slice(&body);
            v.push(cs);
            v
        };
        // 校验生成的码能被 parse 接受（自洽性；理论必过，防御性兜底）
        let text = String::from_utf8_lossy(&full).into_owned();
        if validate_body(&text).is_ok() {
            return format!("{PREFIX}{}", group(&text));
        }
    }
}

/// 房间码正文按 4 位分组（展示用）。
fn group(body: &str) -> String {
    body.as_bytes()
        .chunks(GROUP_LEN)
        .map(std::str::from_utf8)
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join("-"))
        .unwrap_or_else(|_| body.to_string())
}

/// 从房间码派生 EasyTier 组网凭证。
///
/// domain 分离确保「房间码 → name/secret」不可互换用途；
/// SHA-256 单向性保证**看到组网流量也无法反推房间码**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkCredentials {
    /// EasyTier `--network-name`。
    pub network_name: String,
    /// EasyTier `--network-secret`。
    pub network_secret: String,
}

/// 派生组网凭证。
///
/// 输入是**规范格式**房间码（`parse` / `generate` 的返回值）；
/// 用户原始输入请先过 `parse`。
#[must_use]
pub fn credentials(code: &str) -> NetworkCredentials {
    // 取正文（去前缀与分组符），保留校验位：正文即房间身份
    let body = normalize(code);
    let seed = format!("sereno-multiplayer-v1|{body}");

    let name_hash = Sha256::digest(format!("{seed}|name").as_bytes());
    let secret_hash = Sha256::digest(format!("{seed}|secret").as_bytes());

    NetworkCredentials {
        network_name: format!("sereno-{}", hex(&name_hash[..8])),
        network_secret: hex(&secret_hash),
    }
}

/// 房主的固定虚拟 IP。
///
/// 每个房间是独立的 EasyTier 网络（凭证不同、互不可见），
/// 因此所有房间统一使用 `10.126.126.1` 不会冲突，
/// 房客无需任何发现流程即可知道房主地址。
pub const HOST_VIRTUAL_IP: &str = "10.126.126.1";

/// 房主虚拟 IP 的 `SocketAddr` 文本（带端口占位由调用方拼接）。
#[must_use]
pub fn host_virtual_addr(port: u16) -> String {
    format!("{HOST_VIRTUAL_IP}:{port}")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_code_round_trips() {
        for _ in 0..200 {
            let code = generate();
            let reparsed = parse(&code).expect("生成的房间码必须能被 parse 接受");
            assert_eq!(code, reparsed);
        }
    }

    #[test]
    fn generated_code_has_expected_shape() {
        let code = generate();
        assert!(code.starts_with("U/"));
        let body = code.trim_start_matches("U/");
        assert_eq!(body.len(), 16 + 3, "4 组 x4 字符 + 3 个分隔符");
        for c in code.chars() {
            if c != '/' && c != '-' {
                assert!(char_value(c as u8).is_some(), "非法字符 {c}");
            }
        }
        assert!(!code.contains('I') && !code.contains('O'), "不得含 I/O");
    }

    #[test]
    fn parse_tolerates_typos() {
        let code = generate();
        let body = code.trim_start_matches("U/").replace('-', "");

        // 小写、缺分隔符、带 # 前缀都能还原
        let noisy = format!("#{}/{}", "U", body.to_lowercase());
        assert_eq!(parse(&noisy).unwrap(), code);

        let bare = body.to_lowercase();
        assert_eq!(parse(&bare).unwrap(), code);

        // 混淆字符：把正文里的 0 换成 O、1 换成 I（存在时）
        let confused: String = body
            .chars()
            .map(|c| match c {
                '0' => 'O',
                '1' => 'I',
                other => other,
            })
            .collect();
        if confused != body {
            assert_eq!(parse(&confused).unwrap(), code, "I/O 混淆必须可还原");
        }
    }

    #[test]
    fn parse_rejects_checksum_errors() {
        let code = generate();
        let body = code.trim_start_matches("U/").replace('-', "");
        // 换掉最后一位（校验位）→ 必失败
        let corrupted = format!("{}{}", &body[..15], "0");
        let corrupted = if corrupted.ends_with('0') {
            format!("{}1", &body[..15])
        } else {
            corrupted
        };
        assert!(matches!(
            parse(&corrupted),
            Err(RoomCodeError::ChecksumMismatch) | Err(RoomCodeError::BadLength)
        ));
    }

    #[test]
    fn parse_rejects_wrong_length_and_bad_chars() {
        assert_eq!(parse("U/123"), Err(RoomCodeError::BadLength));
        // normalize 之后所有 ASCII 字母数字都是合法 Base-34，
        // BadCharacter 只可能来自 validate_body 的直接调用
        assert_eq!(
            validate_body("0123456789ABCD1@"),
            Err(RoomCodeError::BadCharacter('@'))
        );
    }

    #[test]
    fn credentials_are_deterministic_and_separated() {
        let code = generate();
        let a = credentials(&code);
        let b = credentials(&parse(&code).unwrap());
        assert_eq!(a, b, "同码必须同凭证");

        let other = credentials(&generate());
        assert_ne!(a, other, "不同码不得撞凭证");

        // name 与 secret 是两条独立域
        assert_ne!(a.network_name, a.network_secret);
        assert!(a.network_name.starts_with("sereno-"));
        assert_eq!(a.network_secret.len(), 64);
    }

    #[test]
    fn credentials_change_if_body_changes() {
        let code = generate();
        let body = code.trim_start_matches("U/").replace('-', "");
        let mutated = format!("{}{}", &body[..14], "00");
        // 伪造（校验位会不同，但凭证函数只看正文）——确认正文任何变化都换凭证
        assert_ne!(
            credentials(&code),
            credentials(&format!("U/{mutated}")),
            "正文变化必须换凭证"
        );
    }

    #[test]
    fn host_virtual_addr_formats() {
        assert_eq!(host_virtual_addr(25565), "10.126.126.1:25565");
    }
}
