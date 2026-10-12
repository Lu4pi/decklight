//! Felder der dbserver-Nachrichten: Zahlen (1, 2, 4 Byte), Blobs, UTF-16-Strings.

use std::fmt;

pub const NUM1: u8 = 0x0f;
pub const NUM2: u8 = 0x10;
pub const NUM4: u8 = 0x11;
pub const BLOB: u8 = 0x14;
pub const STRING: u8 = 0x26;

#[derive(Clone, PartialEq, Eq)]
pub enum Field {
    /// Zahl mit Breite in Bytes (1, 2 oder 4).
    Number {
        value: u32,
        width: u8,
    },
    Blob(Vec<u8>),
    String(String),
}

impl fmt::Debug for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number { value, .. } => write!(f, "{value:#x}"),
            Self::Blob(b) => write!(f, "Blob({} Bytes)", b.len()),
            Self::String(s) => write!(f, "{s:?}"),
        }
    }
}

impl Field {
    pub fn n1(v: u8) -> Self {
        Self::Number {
            value: u32::from(v),
            width: 1,
        }
    }
    pub fn n2(v: u16) -> Self {
        Self::Number {
            value: u32::from(v),
            width: 2,
        }
    }
    pub fn n4(v: u32) -> Self {
        Self::Number { value: v, width: 4 }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Self::Number { value, .. } => Some(*value),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_blob(&self) -> Option<&[u8]> {
        match self {
            Self::Blob(b) => Some(b),
            _ => None,
        }
    }

    /// Kennung in der Argumentliste einer Nachricht.
    pub fn arg_tag(&self) -> u8 {
        match self {
            Self::Number { .. } => 0x06,
            Self::Blob(_) => 0x03,
            Self::String(_) => 0x02,
        }
    }

    pub fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Number { value, width: 1 } => {
                out.push(NUM1);
                out.push(*value as u8);
            }
            Self::Number { value, width: 2 } => {
                out.push(NUM2);
                out.extend_from_slice(&(*value as u16).to_be_bytes());
            }
            Self::Number { value, .. } => {
                out.push(NUM4);
                out.extend_from_slice(&value.to_be_bytes());
            }
            Self::Blob(b) => {
                out.push(BLOB);
                out.extend_from_slice(&(b.len() as u32).to_be_bytes());
                out.extend_from_slice(b);
            }
            Self::String(s) => {
                let units: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
                out.push(STRING);
                out.extend_from_slice(&(units.len() as u32).to_be_bytes());
                for u in units {
                    out.extend_from_slice(&u.to_be_bytes());
                }
            }
        }
    }

    /// Liest ein Feld. `Ok(None)`: noch nicht genug Bytes.
    pub fn decode(d: &[u8]) -> Result<Option<(Field, usize)>, String> {
        let Some(&tag) = d.first() else {
            return Ok(None);
        };
        let need = |n: usize| d.len() >= n;
        let u32_at = |at: usize| u32::from_be_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]);
        Ok(match tag {
            NUM1 if need(2) => Some((Self::n1(d[1]), 2)),
            NUM2 if need(3) => Some((Self::n2(u16::from_be_bytes([d[1], d[2]])), 3)),
            NUM4 if need(5) => Some((Self::n4(u32_at(1)), 5)),
            BLOB if need(5) => {
                let len = u32_at(1) as usize;
                need(5 + len).then(|| (Self::Blob(d[5..5 + len].to_vec()), 5 + len))
            }
            STRING if need(5) => {
                let units = u32_at(1) as usize;
                let end = 5 + units * 2;
                need(end).then(|| {
                    let chars: Vec<u16> = d[5..end]
                        .chunks_exact(2)
                        .map(|c| u16::from_be_bytes([c[0], c[1]]))
                        .take_while(|&u| u != 0)
                        .collect();
                    (Self::String(String::from_utf16_lossy(&chars)), end)
                })
            }
            NUM1 | NUM2 | NUM4 | BLOB | STRING => None,
            other => return Err(format!("unbekannter Feldtyp {other:#04x}")),
        })
    }
}
