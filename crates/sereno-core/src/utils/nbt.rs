//! 最小 NBT（Named Binary Text）编解码器。
//!
//! Minecraft 的 `servers.dat`、`level.dat` 都是 NBT 格式。本模块实现
//! **全量泛型**的读取与写出：所有标签原样进出一个树——这意味着
//! 读出来改一处再写回去，**未触碰的字段一个比特都不会丢**。
//! 这比引入完整 NBT 库更可控：我们只需要这十几种标签的大端编解码。
//!
//! 字符串用标准 UTF-8。Java 原版写的是「Modified UTF-8」，二者在
//! 实际出现的字符（服务器名、模组 ID）上完全兼容；仅代理对
//! （emoji 等）有差异，此场景极罕见且不会损坏数据结构。

use crate::{LauncherError, Result};

/// NBT 标签类型 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagId {
    End = 0,
    Byte = 1,
    Short = 2,
    Int = 3,
    Long = 4,
    Float = 5,
    Double = 6,
    ByteArray = 7,
    String = 8,
    List = 9,
    Compound = 10,
    IntArray = 11,
    LongArray = 12,
}

impl TagId {
    fn from_u8(value: u8) -> Result<Self> {
        Ok(match value {
            0 => Self::End,
            1 => Self::Byte,
            2 => Self::Short,
            3 => Self::Int,
            4 => Self::Long,
            5 => Self::Float,
            6 => Self::Double,
            7 => Self::ByteArray,
            8 => Self::String,
            9 => Self::List,
            10 => Self::Compound,
            11 => Self::IntArray,
            12 => Self::LongArray,
            other => {
                return Err(LauncherError::Parse {
                    message: format!("未知 NBT 标签类型: {other}"),
                });
            }
        })
    }
}

/// 泛型 NBT 值。Compound 保有**原始顺序**，写出时原样还原。
#[derive(Debug, Clone, PartialEq)]
pub enum NbtValue {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>),
    String(String),
    /// 元素类型 ID 也原样保留——空列表（长度 0）的元素类型不同实现有差异，
    /// 原样往返才不会让原版客户端读不懂。
    List(u8, Vec<NbtValue>),
    Compound(Vec<(String, NbtValue)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl NbtValue {
    pub fn as_compound(&self) -> Option<&Vec<(String, NbtValue)>> {
        match self {
            NbtValue::Compound(fields) => Some(fields),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<(&u8, &Vec<NbtValue>)> {
        match self {
            NbtValue::List(elem_type, items) => Some((elem_type, items)),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            NbtValue::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_byte_array(&self) -> Option<&[u8]> {
        match self {
            NbtValue::ByteArray(bytes) => Some(bytes),
            _ => None,
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        match self {
            NbtValue::Int(value) => Some(*value),
            _ => None,
        }
    }

    fn tag_id(&self) -> u8 {
        match self {
            NbtValue::Byte(_) => TagId::Byte as u8,
            NbtValue::Short(_) => TagId::Short as u8,
            NbtValue::Int(_) => TagId::Int as u8,
            NbtValue::Long(_) => TagId::Long as u8,
            NbtValue::Float(_) => TagId::Float as u8,
            NbtValue::Double(_) => TagId::Double as u8,
            NbtValue::ByteArray(_) => TagId::ByteArray as u8,
            NbtValue::String(_) => TagId::String as u8,
            NbtValue::List(_, _) => TagId::List as u8,
            NbtValue::Compound(_) => TagId::Compound as u8,
            NbtValue::IntArray(_) => TagId::IntArray as u8,
            NbtValue::LongArray(_) => TagId::LongArray as u8,
        }
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    depth: u8,
}

const MAX_DEPTH: u8 = 32;

impl<'a> Reader<'a> {
    fn need(&self, count: usize) -> Result<()> {
        if self.pos + count > self.data.len() {
            return Err(LauncherError::Parse {
                message: "NBT 数据被截断".into(),
            });
        }
        Ok(())
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        self.need(count)?;
        let slice = &self.data[self.pos..self.pos + count];
        self.pos += count;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn i32(&mut self) -> Result<i32> {
        let bytes = self.take(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn string(&mut self) -> Result<String> {
        let len = self.u16()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|e| LauncherError::Parse {
            message: format!("NBT 字符串不是合法 UTF-8: {e}"),
        })
    }

    /// 读取一个指定类型的标签 payload（不含 name 前缀）。
    fn payload(&mut self, tag: TagId) -> Result<NbtValue> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.depth -= 1;
            return Err(LauncherError::Parse {
                message: "NBT 嵌套过深（疑似恶意文件）".into(),
            });
        }
        let value = match tag {
            TagId::End => NbtValue::Byte(0), // 不应出现在此，防御性处理
            TagId::Byte => NbtValue::Byte(self.u8()? as i8),
            TagId::Short => {
                let raw = self.take(2)?;
                NbtValue::Short(i16::from_be_bytes([raw[0], raw[1]]))
            }
            TagId::Int => NbtValue::Int(self.i32()?),
            TagId::Long => {
                let raw = self.take(8)?;
                NbtValue::Long(i64::from_be_bytes([
                    raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
                ]))
            }
            TagId::Float => {
                let raw = self.take(4)?;
                NbtValue::Float(f32::from_bits(u32::from_be_bytes([
                    raw[0], raw[1], raw[2], raw[3],
                ])))
            }
            TagId::Double => {
                let raw = self.take(8)?;
                NbtValue::Double(f64::from_bits(u64::from_be_bytes([
                    raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
                ])))
            }
            TagId::ByteArray => {
                let len = self.i32()?;
                let len = usize::try_from(len).map_err(|_| LauncherError::Parse {
                    message: "NBT ByteArray 长度非法".into(),
                })?;
                NbtValue::ByteArray(self.take(len)?.to_vec())
            }
            TagId::String => NbtValue::String(self.string()?),
            TagId::List => {
                let elem_type = self.u8()?;
                let count = self.i32()?;
                let count = usize::try_from(count).map_err(|_| LauncherError::Parse {
                    message: "NBT List 长度非法".into(),
                })?;
                let mut items = Vec::with_capacity(count.min(1024));
                for _ in 0..count {
                    items.push(self.payload(TagId::from_u8(elem_type)?)?);
                }
                NbtValue::List(elem_type, items)
            }
            TagId::Compound => {
                let mut fields = Vec::new();
                loop {
                    let child_tag = TagId::from_u8(self.u8()?)?;
                    if child_tag == TagId::End {
                        break;
                    }
                    let name = self.string()?;
                    let value = self.payload(child_tag)?;
                    fields.push((name, value));
                }
                NbtValue::Compound(fields)
            }
            TagId::IntArray => {
                let count = self.i32()?;
                let count = usize::try_from(count).map_err(|_| LauncherError::Parse {
                    message: "NBT IntArray 长度非法".into(),
                })?;
                let mut items = Vec::with_capacity(count.min(1024));
                for _ in 0..count {
                    items.push(self.i32()?);
                }
                NbtValue::IntArray(items)
            }
            TagId::LongArray => {
                let count = self.i32()?;
                let count = usize::try_from(count).map_err(|_| LauncherError::Parse {
                    message: "NBT LongArray 长度非法".into(),
                })?;
                let mut items = Vec::with_capacity(count.min(1024));
                for _ in 0..count {
                    let raw = self.take(8)?;
                    items.push(i64::from_be_bytes([
                        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
                    ]));
                }
                NbtValue::LongArray(items)
            }
        };
        self.depth -= 1;
        Ok(value)
    }
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u16).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn write_payload(out: &mut Vec<u8>, value: &NbtValue) {
    match value {
        NbtValue::Byte(v) => out.push(*v as u8),
        NbtValue::Short(v) => out.extend_from_slice(&v.to_be_bytes()),
        NbtValue::Int(v) => out.extend_from_slice(&v.to_be_bytes()),
        NbtValue::Long(v) => out.extend_from_slice(&v.to_be_bytes()),
        NbtValue::Float(v) => out.extend_from_slice(&v.to_bits().to_be_bytes()),
        NbtValue::Double(v) => out.extend_from_slice(&v.to_bits().to_be_bytes()),
        NbtValue::ByteArray(bytes) => {
            out.extend_from_slice(&(bytes.len() as i32).to_be_bytes());
            out.extend_from_slice(bytes);
        }
        NbtValue::String(v) => write_string(out, v),
        NbtValue::List(elem_type, items) => {
            out.push(*elem_type);
            out.extend_from_slice(&(items.len() as i32).to_be_bytes());
            for item in items {
                write_payload(out, item);
            }
        }
        NbtValue::Compound(fields) => {
            for (name, child) in fields {
                out.push(child.tag_id());
                write_string(out, name);
                write_payload(out, child);
            }
            out.push(TagId::End as u8);
        }
        NbtValue::IntArray(items) => {
            out.extend_from_slice(&(items.len() as i32).to_be_bytes());
            for item in items {
                out.extend_from_slice(&item.to_be_bytes());
            }
        }
        NbtValue::LongArray(items) => {
            out.extend_from_slice(&(items.len() as i32).to_be_bytes());
            for item in items {
                out.extend_from_slice(&item.to_be_bytes());
            }
        }
    }
}

/// 读取完整 NBT 文档（根名 + 根 Compound）。
pub fn read_root(data: &[u8]) -> Result<(String, NbtValue)> {
    let mut reader = Reader {
        data,
        pos: 0,
        depth: 0,
    };
    let tag = TagId::from_u8(reader.u8()?)?;
    if tag != TagId::Compound {
        return Err(LauncherError::Parse {
            message: format!("NBT 根标签必须是 Compound，实际是 {tag:?}"),
        });
    }
    let name = reader.string()?;
    let root = reader.payload(tag)?;
    Ok((name, root))
}

/// 写出完整 NBT 文档。
pub fn write_root(name: &str, root: &NbtValue) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.push(TagId::Compound as u8);
    write_string(&mut out, name);
    write_payload(&mut out, root);
    out
}

/// 构造 Compound 的便捷函数。
#[must_use]
pub fn compound(fields: Vec<(&str, NbtValue)>) -> NbtValue {
    NbtValue::Compound(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_string(), value))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> NbtValue {
        compound(vec![
            (
                "servers",
                NbtValue::List(
                    TagId::Compound as u8,
                    vec![
                        compound(vec![
                            ("name", NbtValue::String("生存服".into())),
                            ("ip", NbtValue::String("mc.example.com:25565".into())),
                            (
                                "icon",
                                NbtValue::ByteArray(vec![0x89, b'P', b'N', b'G', 0, 1]),
                            ),
                        ]),
                        compound(vec![
                            ("name", NbtValue::String("创造服".into())),
                            ("ip", NbtValue::String("10.0.0.2".into())),
                        ]),
                    ],
                ),
            ),
            ("hiddenField", NbtValue::Long(123456789)),
        ])
    }

    #[test]
    fn round_trip_preserves_everything() {
        let root = sample();
        let bytes = write_root("", &root);
        let (name, parsed) = read_root(&bytes).unwrap();
        assert_eq!(name, "");
        assert_eq!(parsed, root, "往返后必须逐比特等价");
    }

    #[test]
    fn nested_and_all_tag_types_round_trip() {
        let root = compound(vec![
            ("b", NbtValue::Byte(-1)),
            ("s", NbtValue::Short(-2)),
            ("i", NbtValue::Int(-3)),
            ("l", NbtValue::Long(-4)),
            ("f", NbtValue::Float(1.5)),
            ("d", NbtValue::Double(-2.5)),
            ("ba", NbtValue::ByteArray(vec![1, 2, 3])),
            ("str", NbtValue::String("中文✓".into())),
            (
                "list",
                NbtValue::List(TagId::String as u8, vec![NbtValue::String("a".into())]),
            ),
            ("emptyList", NbtValue::List(TagId::End as u8, Vec::new())),
            ("ia", NbtValue::IntArray(vec![1, -2])),
            ("la", NbtValue::LongArray(vec![7])),
            ("nested", compound(vec![("deep", NbtValue::Int(42))])),
        ]);
        let bytes = write_root("Level", &root);
        let (name, parsed) = read_root(&bytes).unwrap();
        assert_eq!(name, "Level");
        assert_eq!(parsed, root);
    }

    #[test]
    fn truncated_data_is_rejected() {
        let bytes = write_root("", &sample());
        let err = read_root(&bytes[..bytes.len() - 3]);
        assert!(err.is_err());
    }
}
