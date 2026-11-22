use crate::error::{EngineError, EngineErrorKind, Result};

/// Java string content as UTF-16 code units, including unmatched surrogates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct JavaString(Vec<u16>);

impl JavaString {
    pub fn from_utf16(units: Vec<u16>) -> Self {
        Self(units)
    }

    pub fn as_utf16(&self) -> &[u16] {
        &self.0
    }

    pub fn from_modified_utf8(bytes: &[u8]) -> Result<Self> {
        decode_modified_utf8_units(bytes).map(Self)
    }

    pub fn to_modified_utf8(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.0.len());
        for &unit in &self.0 {
            encode_code_unit(unit, &mut bytes);
        }
        bytes
    }

    /// Convert only when the code units form Unicode scalar values.
    pub fn to_unicode(&self) -> std::result::Result<String, std::string::FromUtf16Error> {
        String::from_utf16(&self.0)
    }

    /// Convert to a Rust `String`, replacing unpaired UTF-16 surrogates with U+FFFD.
    pub fn to_string_lossy(&self) -> String {
        String::from_utf16_lossy(&self.0)
    }
}

impl std::fmt::Display for JavaString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_string_lossy())
    }
}

impl From<&str> for JavaString {
    fn from(value: &str) -> Self {
        Self(value.encode_utf16().collect())
    }
}

impl From<String> for JavaString {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl PartialEq<str> for JavaString {
    fn eq(&self, other: &str) -> bool {
        self.0.iter().copied().eq(other.encode_utf16())
    }
}

impl PartialEq<&str> for JavaString {
    fn eq(&self, other: &&str) -> bool {
        self == *other
    }
}

impl PartialEq<String> for JavaString {
    fn eq(&self, other: &String) -> bool {
        self == other.as_str()
    }
}

pub fn encode_modified_utf8(value: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    for code_unit in value.encode_utf16() {
        encode_code_unit(code_unit, &mut out);
    }
    out
}

pub fn decode_modified_utf8(data: &[u8]) -> Result<String> {
    let (utf16_units, offsets) = decode_modified_utf8_units_with_offsets(data)?;
    String::from_utf16(&utf16_units).map_err(|err| {
        let mut bad_offset = data.len();
        for (idx, &unit) in utf16_units.iter().enumerate() {
            if (0xD800..=0xDBFF).contains(&unit) {
                let next_is_low = utf16_units
                    .get(idx + 1)
                    .is_some_and(|next| (0xDC00..=0xDFFF).contains(next));
                if !next_is_low {
                    bad_offset = offsets[idx];
                    break;
                }
            } else if (0xDC00..=0xDFFF).contains(&unit) {
                let prev_is_high = idx > 0 && (0xD800..=0xDBFF).contains(&utf16_units[idx - 1]);
                if !prev_is_high {
                    bad_offset = offsets[idx];
                    break;
                }
            }
        }
        EngineError::new(
            bad_offset,
            EngineErrorKind::InvalidModifiedUtf8 {
                reason: format!("invalid UTF-16 surrogate sequence: {err}"),
            },
        )
    })
}

fn decode_modified_utf8_units(data: &[u8]) -> Result<Vec<u16>> {
    Ok(decode_modified_utf8_units_with_offsets(data)?.0)
}

fn decode_modified_utf8_units_with_offsets(data: &[u8]) -> Result<(Vec<u16>, Vec<usize>)> {
    let mut utf16_units = Vec::with_capacity(data.len());
    let mut unit_offsets = Vec::with_capacity(data.len());
    let mut index = 0_usize;

    while index < data.len() {
        let unit_offset = index;
        let first = data[index];
        let code_unit = if first == 0 {
            return Err(EngineError::new(
                index,
                EngineErrorKind::InvalidModifiedUtf8 {
                    reason: "NUL must use the modified UTF-8 two-byte form".to_owned(),
                },
            ));
        } else if first <= 0x7F {
            index += 1;
            first as u16
        } else if (0xC0..=0xDF).contains(&first) {
            if index + 1 >= data.len() {
                return Err(EngineError::new(
                    index,
                    EngineErrorKind::InvalidModifiedUtf8 {
                        reason: "truncated two-byte sequence".to_owned(),
                    },
                ));
            }
            let second = data[index + 1];
            if second & 0xC0 != 0x80 {
                return Err(EngineError::new(
                    index + 1,
                    EngineErrorKind::InvalidModifiedUtf8 {
                        reason: "invalid continuation byte".to_owned(),
                    },
                ));
            }
            let code_unit = (((first & 0x1F) as u16) << 6) | ((second & 0x3F) as u16);
            if code_unit < 0x80 && code_unit != 0 {
                return Err(EngineError::new(
                    index,
                    EngineErrorKind::InvalidModifiedUtf8 {
                        reason: "overlong two-byte sequence".to_owned(),
                    },
                ));
            }
            index += 2;
            code_unit
        } else if (0xE0..=0xEF).contains(&first) {
            if index + 2 >= data.len() {
                return Err(EngineError::new(
                    index,
                    EngineErrorKind::InvalidModifiedUtf8 {
                        reason: "truncated three-byte sequence".to_owned(),
                    },
                ));
            }
            let second = data[index + 1];
            let third = data[index + 2];
            if second & 0xC0 != 0x80 {
                return Err(EngineError::new(
                    index + 1,
                    EngineErrorKind::InvalidModifiedUtf8 {
                        reason: "invalid continuation byte".to_owned(),
                    },
                ));
            }
            if third & 0xC0 != 0x80 {
                return Err(EngineError::new(
                    index + 2,
                    EngineErrorKind::InvalidModifiedUtf8 {
                        reason: "invalid continuation byte".to_owned(),
                    },
                ));
            }
            let code_unit = (((first & 0x0F) as u16) << 12)
                | (((second & 0x3F) as u16) << 6)
                | ((third & 0x3F) as u16);
            if code_unit < 0x800 {
                return Err(EngineError::new(
                    index,
                    EngineErrorKind::InvalidModifiedUtf8 {
                        reason: "overlong three-byte sequence".to_owned(),
                    },
                ));
            }
            index += 3;
            code_unit
        } else if (0x80..=0xBF).contains(&first) {
            return Err(EngineError::new(
                index,
                EngineErrorKind::InvalidModifiedUtf8 {
                    reason: format!("unexpected continuation byte 0x{first:02x}"),
                },
            ));
        } else if (0xF0..=0xF7).contains(&first) {
            return Err(EngineError::new(
                index,
                EngineErrorKind::InvalidModifiedUtf8 {
                    reason: "modified UTF-8 does not permit four-byte sequences".to_owned(),
                },
            ));
        } else {
            return Err(EngineError::new(
                index,
                EngineErrorKind::InvalidModifiedUtf8 {
                    reason: format!("invalid modified UTF-8 lead byte 0x{first:02x}"),
                },
            ));
        };

        utf16_units.push(code_unit);
        unit_offsets.push(unit_offset);
    }

    Ok((utf16_units, unit_offsets))
}

fn encode_code_unit(code_unit: u16, out: &mut Vec<u8>) {
    if code_unit == 0 {
        out.extend_from_slice(&[0xC0, 0x80]);
    } else if code_unit <= 0x7F {
        out.push(code_unit as u8);
    } else if code_unit <= 0x07FF {
        out.extend_from_slice(&[
            0xC0 | ((code_unit >> 6) as u8),
            0x80 | ((code_unit & 0x3F) as u8),
        ]);
    } else {
        out.extend_from_slice(&[
            0xE0 | ((code_unit >> 12) as u8),
            0x80 | (((code_unit >> 6) & 0x3F) as u8),
            0x80 | ((code_unit & 0x3F) as u8),
        ]);
    }
}
