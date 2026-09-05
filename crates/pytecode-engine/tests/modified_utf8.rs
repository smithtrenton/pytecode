use pytecode_engine::modified_utf8::{decode_modified_utf8, encode_modified_utf8};

#[test]
fn java_strings_preserve_every_utf16_code_unit() {
    use pytecode_engine::modified_utf8::JavaString;
    for units in [
        vec![0xd800],
        vec![0xdc00],
        vec![0, 0xd800, 65, 0xdc00, 0xd83d, 0xde00],
    ] {
        let value = JavaString::from_utf16(units.clone());
        let bytes = value.to_modified_utf8();
        assert_eq!(
            JavaString::from_modified_utf8(&bytes).unwrap().as_utf16(),
            units
        );
        assert!(value.to_unicode().is_err());
        assert!(decode_modified_utf8(&bytes).is_err());
    }
    let all_units: Vec<u16> = (0..=u16::MAX).collect();
    let value = JavaString::from_utf16(all_units.clone());
    assert_eq!(
        JavaString::from_modified_utf8(&value.to_modified_utf8())
            .unwrap()
            .as_utf16(),
        all_units
    );
}

#[test]
fn encode_modified_utf8_matches_expected_cases() {
    let cases = [
        ("", Vec::new()),
        ("Hello", b"Hello".to_vec()),
        ("\0", vec![0xC0, 0x80]),
        ("😀", vec![0xED, 0xA0, 0xBD, 0xED, 0xB8, 0x80]),
    ];

    for (value, expected) in cases {
        assert_eq!(encode_modified_utf8(value), expected);
    }
}

#[test]
fn modified_utf8_round_trips() {
    for value in ["", "Hello", "cafe\u{301}", "\0", "😀", "a\0😀b"] {
        let encoded = encode_modified_utf8(value);
        assert_eq!(decode_modified_utf8(&encoded).unwrap(), value);
    }
}

#[test]
fn decode_rejects_raw_nul_byte() {
    let err = decode_modified_utf8(&[0x00]).unwrap_err();
    assert!(err.to_string().contains("NUL"));
}

#[test]
fn decode_rejects_four_byte_sequence() {
    let err = decode_modified_utf8("😀".as_bytes()).unwrap_err();
    assert!(err.to_string().contains("four-byte sequences"));
}
