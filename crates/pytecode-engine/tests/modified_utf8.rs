use pytecode_engine::modified_utf8::{decode_modified_utf8, encode_modified_utf8};

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
