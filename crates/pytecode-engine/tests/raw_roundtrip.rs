use pytecode_engine::constants::MAGIC;
use pytecode_engine::error::EngineErrorKind;
use pytecode_engine::fixtures::{
    compiled_fixture_paths as rust_compiled_fixture_paths,
    compiled_fixture_paths_for as rust_compiled_fixture_paths_for,
};
use pytecode_engine::modified_utf8::{decode_modified_utf8, encode_modified_utf8};
use pytecode_engine::parse_class;
use pytecode_engine::raw::AttributeInfo;
use pytecode_engine::write_class;

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[test]
fn minimal_classfile_parses() -> TestResult<()> {
    let parsed = parse_class(&minimal_classfile())?;
    assert_eq!(parsed.magic, MAGIC);
    assert_eq!(parsed.major_version, 52);
    assert_eq!(parsed.minor_version, 0);
    assert_eq!(parsed.this_class, 2);
    assert_eq!(parsed.super_class, 4);
    assert_eq!(parsed.interfaces.len(), 0);
    assert_eq!(parsed.fields.len(), 0);
    assert_eq!(parsed.methods.len(), 0);
    assert_eq!(parsed.attributes.len(), 0);
    Ok(())
}

#[test]
fn writer_roundtrip_preserves_unknown_attribute_bytes() -> TestResult<()> {
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: utf8_entry_bytes("CustomAttr"),
        extra_cp_count: 1,
        class_attrs_count: 1,
        class_attrs_bytes: make_attribute_blob(5, &[0x01, 0x02, 0x03]),
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn writer_roundtrip_preserves_long_gap_slots() -> TestResult<()> {
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: long_entry_bytes(0x11111111, 0x22222222),
        extra_cp_count: 2,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn invalid_magic_and_version_return_structured_errors() {
    let mut invalid_magic = minimal_classfile();
    invalid_magic[0] = 0;
    let err = parse_class(&invalid_magic).unwrap_err();
    assert!(matches!(err.kind, EngineErrorKind::InvalidMagic { .. }));

    let invalid_version = minimal_classfile_with_version(56, 1);
    let err = parse_class(&invalid_version).unwrap_err();
    assert!(matches!(err.kind, EngineErrorKind::InvalidVersion { .. }));
}

#[test]
fn hello_world_fixture_exposes_code_and_source_file() -> TestResult<()> {
    let class_path = rust_compiled_fixture_paths_for("HelloWorld.java")?
        .into_iter()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some("HelloWorld.class"))
        .ok_or("HelloWorld.class not found")?;
    let original = std::fs::read(&class_path)?;
    let parsed = parse_class(&original)?;

    let main_method = parsed
        .methods
        .iter()
        .find(|method| matches!(constant_pool_utf8(&parsed, method.name_index), Ok(name) if name == "main"))
        .ok_or("main method not found")?;

    let code_attr = main_method
        .attributes
        .iter()
        .find_map(|attribute| match attribute {
            AttributeInfo::Code(code) => Some(code),
            _ => None,
        })
        .ok_or("Code attribute not found")?;
    let opcodes: Vec<u8> = code_attr
        .code
        .iter()
        .map(|instruction| instruction.opcode())
        .collect();
    assert_eq!(opcodes, vec![0xB2, 0x12, 0xB6, 0xB1]);

    assert!(
        parsed
            .attributes
            .iter()
            .any(|attribute| matches!(attribute, AttributeInfo::SourceFile(_)))
    );
    assert_eq!(write_class(&parsed)?, original);
    Ok(())
}

#[test]
fn writer_roundtrip_all_java_resources() -> TestResult<()> {
    for class_path in rust_compiled_fixture_paths(25)? {
        let original = std::fs::read(&class_path)?;
        let parsed = parse_class(&original)?;
        let emitted = write_class(&parsed)?;
        assert_eq!(
            emitted,
            original,
            "roundtrip mismatch for {}",
            class_path.display()
        );
    }
    Ok(())
}

fn constant_pool_utf8(
    classfile: &pytecode_engine::raw::ClassFile,
    index: u16,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let entry = classfile
        .constant_pool
        .get(index as usize)
        .and_then(Option::as_ref)
        .ok_or("missing constant-pool entry")?;
    match entry {
        pytecode_engine::raw::ConstantPoolEntry::Utf8(info) => {
            Ok(decode_modified_utf8(&info.bytes)?)
        }
        _ => Err("constant-pool entry is not Utf8".into()),
    }
}

#[derive(Debug, Clone)]
struct MinimalClassfileOptions {
    extra_cp_bytes: Vec<u8>,
    extra_cp_count: u16,
    access_flags: u16,
    this_class: u16,
    super_class: u16,
    interfaces: Vec<u16>,
    fields_bytes: Vec<u8>,
    methods_bytes: Vec<u8>,
    class_attrs_count: u16,
    class_attrs_bytes: Vec<u8>,
}

impl Default for MinimalClassfileOptions {
    fn default() -> Self {
        Self {
            extra_cp_bytes: Vec::new(),
            extra_cp_count: 0,
            access_flags: 0x0021,
            this_class: 2,
            super_class: 4,
            interfaces: Vec::new(),
            fields_bytes: Vec::new(),
            methods_bytes: Vec::new(),
            class_attrs_count: 0,
            class_attrs_bytes: Vec::new(),
        }
    }
}

fn u2(value: u16) -> [u8; 2] {
    value.to_be_bytes()
}

fn u4(value: u32) -> [u8; 4] {
    value.to_be_bytes()
}

fn utf8_entry_bytes(value: &str) -> Vec<u8> {
    let encoded = encode_modified_utf8(value);
    let mut bytes = vec![1];
    bytes.extend_from_slice(&u2(encoded.len() as u16));
    bytes.extend_from_slice(&encoded);
    bytes
}

fn class_entry_bytes(name_index: u16) -> Vec<u8> {
    let mut bytes = vec![7];
    bytes.extend_from_slice(&u2(name_index));
    bytes
}

fn long_entry_bytes(high: u32, low: u32) -> Vec<u8> {
    let mut bytes = vec![5];
    bytes.extend_from_slice(&u4(high));
    bytes.extend_from_slice(&u4(low));
    bytes
}

fn make_attribute_blob(name_index: u16, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&u2(name_index));
    bytes.extend_from_slice(&u4(payload.len() as u32));
    bytes.extend_from_slice(payload);
    bytes
}

fn minimal_classfile() -> Vec<u8> {
    minimal_classfile_with_version(52, 0)
}

fn minimal_classfile_with_version(major: u16, minor: u16) -> Vec<u8> {
    minimal_classfile_with_options(MinimalClassfileOptions::default())
        .into_iter()
        .enumerate()
        .map(|(index, byte)| match index {
            4 => (minor >> 8) as u8,
            5 => minor as u8,
            6 => (major >> 8) as u8,
            7 => major as u8,
            _ => byte,
        })
        .collect()
}

fn minimal_classfile_with_options(options: MinimalClassfileOptions) -> Vec<u8> {
    let mut base_cp = Vec::new();
    base_cp.extend_from_slice(&utf8_entry_bytes("TestClass"));
    base_cp.extend_from_slice(&class_entry_bytes(1));
    base_cp.extend_from_slice(&utf8_entry_bytes("java/lang/Object"));
    base_cp.extend_from_slice(&class_entry_bytes(3));

    let cp_count = 5_u16 + options.extra_cp_count;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&MAGIC.to_be_bytes());
    bytes.extend_from_slice(&u2(0));
    bytes.extend_from_slice(&u2(52));
    bytes.extend_from_slice(&u2(cp_count));
    bytes.extend_from_slice(&base_cp);
    bytes.extend_from_slice(&options.extra_cp_bytes);
    bytes.extend_from_slice(&u2(options.access_flags));
    bytes.extend_from_slice(&u2(options.this_class));
    bytes.extend_from_slice(&u2(options.super_class));
    bytes.extend_from_slice(&u2(options.interfaces.len() as u16));
    for interface in &options.interfaces {
        bytes.extend_from_slice(&u2(*interface));
    }
    bytes.extend_from_slice(&u2(0));
    bytes.extend_from_slice(&options.fields_bytes);
    bytes.extend_from_slice(&u2(0));
    bytes.extend_from_slice(&options.methods_bytes);
    bytes.extend_from_slice(&u2(options.class_attrs_count));
    bytes.extend_from_slice(&options.class_attrs_bytes);
    bytes
}
