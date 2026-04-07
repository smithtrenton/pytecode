use crate::error::{EngineError, EngineErrorKind, Result};

pub fn encode_modified_utf8(value: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    for code_unit in value.encode_utf16() {
        encode_code_unit(code_unit, &mut out);
    }
    out
}

pub fn decode_modified_utf8(data: &[u8]) -> Result<String> {
    let mut utf16_units = Vec::with_capacity(data.len());
    let mut index = 0_usize;

    while index < data.len() {
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
            if second & 0xC0 != 0x80 || third & 0xC0 != 0x80 {
                return Err(EngineError::new(
                    index,
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
        } else {
            return Err(EngineError::new(
                index,
                EngineErrorKind::InvalidModifiedUtf8 {
                    reason: "modified UTF-8 does not permit four-byte sequences".to_owned(),
                },
            ));
        };

        utf16_units.push(code_unit);
    }

    String::from_utf16(&utf16_units).map_err(|err| {
        EngineError::new(
            data.len(),
            EngineErrorKind::InvalidModifiedUtf8 {
                reason: format!("invalid UTF-16 surrogate sequence: {err}"),
            },
        )
    })
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
