use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

/// Decode a JVM Modified UTF-8 byte sequence into a Python string.
///
/// Implements §4.4.7 of the JVM specification.  Modified UTF-8 differs from
/// standard UTF-8 in two ways:
///   1. The null character U+0000 is encoded as the two-byte sequence C0 80.
///   2. Supplementary characters (U+10000..U+10FFFF) are represented as a
///      surrogate pair, each half encoded independently as three bytes.
#[pyfunction]
pub fn decode_modified_utf8(data: &[u8]) -> PyResult<String> {
    let mut out = String::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i];
        if b0 == 0 {
            return Err(PyValueError::new_err(
                "bare 0x00 byte is invalid in Modified UTF-8",
            ));
        } else if b0 < 0x80 {
            // Single-byte character (U+0001..U+007F)
            out.push(b0 as char);
            i += 1;
        } else if b0 & 0xE0 == 0xC0 {
            // Two-byte form (U+0000 via C0 80, or U+0080..U+07FF)
            if i + 1 >= data.len() {
                return Err(PyValueError::new_err(
                    "truncated 2-byte Modified UTF-8 sequence",
                ));
            }
            let b1 = data[i + 1];
            if b1 & 0xC0 != 0x80 {
                return Err(PyValueError::new_err(
                    "invalid continuation byte in 2-byte sequence",
                ));
            }
            let cp = (((b0 & 0x1F) as u32) << 6) | ((b1 & 0x3F) as u32);
            if cp < 0x80 && cp != 0 {
                return Err(PyValueError::new_err(
                    "overlong 2-byte Modified UTF-8 sequence",
                ));
            }
            out.push(
                char::from_u32(cp).ok_or_else(|| PyValueError::new_err("invalid code point"))?,
            );
            i += 2;
        } else if b0 & 0xF0 == 0xE0 {
            // Three-byte form (U+0800..U+FFFF, including surrogates for supplementary)
            if i + 2 >= data.len() {
                return Err(PyValueError::new_err(
                    "truncated 3-byte Modified UTF-8 sequence",
                ));
            }
            let b1 = data[i + 1];
            let b2 = data[i + 2];
            if (b1 & 0xC0 != 0x80) || (b2 & 0xC0 != 0x80) {
                return Err(PyValueError::new_err(
                    "invalid continuation byte in 3-byte sequence",
                ));
            }
            let cp =
                (((b0 & 0x0F) as u32) << 12) | (((b1 & 0x3F) as u32) << 6) | ((b2 & 0x3F) as u32);
            if cp < 0x800 {
                return Err(PyValueError::new_err(
                    "overlong 3-byte Modified UTF-8 sequence",
                ));
            }

            // Check for surrogate pair (supplementary character)
            if (0xD800..=0xDBFF).contains(&cp) {
                // High surrogate — expect a low surrogate to follow
                if i + 5 >= data.len() {
                    return Err(PyValueError::new_err("truncated surrogate pair"));
                }
                let b3 = data[i + 3];
                let b4 = data[i + 4];
                let b5 = data[i + 5];
                if b3 & 0xF0 != 0xE0 || b4 & 0xC0 != 0x80 || b5 & 0xC0 != 0x80 {
                    return Err(PyValueError::new_err("invalid low surrogate encoding"));
                }
                let cp2 = (((b3 & 0x0F) as u32) << 12)
                    | (((b4 & 0x3F) as u32) << 6)
                    | ((b5 & 0x3F) as u32);
                if !(0xDC00..=0xDFFF).contains(&cp2) {
                    return Err(PyValueError::new_err("expected low surrogate"));
                }
                let supplementary = 0x10000 + ((cp - 0xD800) << 10) + (cp2 - 0xDC00);
                out.push(
                    char::from_u32(supplementary)
                        .ok_or_else(|| PyValueError::new_err("invalid supplementary code point"))?,
                );
                i += 6;
            } else if (0xDC00..=0xDFFF).contains(&cp) {
                return Err(PyValueError::new_err("unexpected low surrogate"));
            } else {
                out.push(
                    char::from_u32(cp)
                        .ok_or_else(|| PyValueError::new_err("invalid code point"))?,
                );
                i += 3;
            }
        } else {
            return Err(PyValueError::new_err(format!(
                "invalid Modified UTF-8 lead byte: 0x{b0:02X}"
            )));
        }
    }
    Ok(out)
}

/// Encode a Python string into JVM Modified UTF-8 bytes.
///
/// Inverse of `decode_modified_utf8`.
pub(crate) fn encode_modified_utf8_bytes(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for ch in s.chars() {
        let cp = ch as u32;
        if cp == 0 {
            // Null character → overlong two-byte encoding
            out.push(0xC0);
            out.push(0x80);
        } else if cp <= 0x7F {
            out.push(cp as u8);
        } else if cp <= 0x7FF {
            out.push((0xC0 | (cp >> 6)) as u8);
            out.push((0x80 | (cp & 0x3F)) as u8);
        } else if cp <= 0xFFFF {
            out.push((0xE0 | (cp >> 12)) as u8);
            out.push((0x80 | ((cp >> 6) & 0x3F)) as u8);
            out.push((0x80 | (cp & 0x3F)) as u8);
        } else {
            // Supplementary character → surrogate pair, each half as 3 bytes
            let adjusted = cp - 0x10000;
            let high = 0xD800 + (adjusted >> 10);
            let low = 0xDC00 + (adjusted & 0x3FF);
            out.push((0xE0 | (high >> 12)) as u8);
            out.push((0x80 | ((high >> 6) & 0x3F)) as u8);
            out.push((0x80 | (high & 0x3F)) as u8);
            out.push((0xE0 | (low >> 12)) as u8);
            out.push((0x80 | ((low >> 6) & 0x3F)) as u8);
            out.push((0x80 | (low & 0x3F)) as u8);
        }
    }
    out
}

#[pyfunction]
pub fn encode_modified_utf8<'py>(py: Python<'py>, s: &str) -> Bound<'py, PyBytes> {
    let out = encode_modified_utf8_bytes(s);
    PyBytes::new(py, &out)
}

/// Register modified_utf8 functions on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "modified_utf8")?;
    m.add_function(wrap_pyfunction!(decode_modified_utf8, &m)?)?;
    m.add_function(wrap_pyfunction!(encode_modified_utf8, &m)?)?;
    crate::register_submodule(parent, &m, "pytecode._rust.classfile.modified_utf8")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_roundtrip() {
        let input = "Hello, World!";
        let encoded = encode_modified_utf8_bytes(input);
        assert_eq!(encoded, input.as_bytes());
        let decoded = decode_modified_utf8(&encoded).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn null_character() {
        let encoded = encode_modified_utf8_bytes("\0");
        assert_eq!(encoded, vec![0xC0, 0x80]);
        let decoded = decode_modified_utf8(&encoded).unwrap();
        assert_eq!(decoded, "\0");
    }

    #[test]
    fn bare_null_is_invalid() {
        assert!(decode_modified_utf8(&[0x00]).is_err());
    }

    #[test]
    fn overlong_two_byte_sequence_is_invalid() {
        assert!(decode_modified_utf8(&[0xC1, 0x81]).is_err());
    }

    #[test]
    fn overlong_three_byte_sequence_is_invalid() {
        assert!(decode_modified_utf8(&[0xE0, 0x80, 0x80]).is_err());
    }

    #[test]
    fn two_byte_char() {
        // U+00A9 © = C2 A9
        let encoded = encode_modified_utf8_bytes("©");
        assert_eq!(encoded, vec![0xC2, 0xA9]);
        let decoded = decode_modified_utf8(&encoded).unwrap();
        assert_eq!(decoded, "©");
    }

    #[test]
    fn three_byte_char() {
        // U+2603 ☃ = E2 98 83
        let encoded = encode_modified_utf8_bytes("☃");
        assert_eq!(encoded, vec![0xE2, 0x98, 0x83]);
        let decoded = decode_modified_utf8(&encoded).unwrap();
        assert_eq!(decoded, "☃");
    }

    #[test]
    fn supplementary_surrogate_pair() {
        // U+1F600 😀 → surrogate pair D83D DE00
        let input = "😀";
        let encoded = encode_modified_utf8_bytes(input);
        assert_eq!(encoded.len(), 6); // two 3-byte surrogates
        let decoded = decode_modified_utf8(&encoded).unwrap();
        assert_eq!(decoded, input);
    }
}
