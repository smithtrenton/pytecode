// Phase 6: comprehensive unit-level coverage for raw parsing modules.
//
// Tests constant-pool entry types, instruction opcodes, malformed-input error
// handling, and boundary conditions that previously had ZERO unit tests.

use pytecode_engine::constants::MAGIC;
use pytecode_engine::error::EngineErrorKind;
use pytecode_engine::indexes::*;
use pytecode_engine::modified_utf8::encode_modified_utf8;
use pytecode_engine::parse_class;
use pytecode_engine::raw::{
    ArrayType, ConstantPoolEntry, Instruction, InvokeDynamicInsn, InvokeInterfaceInsn,
    LookupSwitchInsn, MatchOffsetPair, NewArrayInsn, WideInstruction,
};
use pytecode_engine::write_class;

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

// ---------------------------------------------------------------------------
// Byte-level helpers (mirrored from raw_roundtrip.rs)
// ---------------------------------------------------------------------------

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

fn make_attribute_blob(name_index: u16, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&u2(name_index));
    bytes.extend_from_slice(&u4(payload.len() as u32));
    bytes.extend_from_slice(payload);
    bytes
}

fn method_info_blob(
    access_flags: u16,
    name_index: u16,
    descriptor_index: u16,
    attributes: &[Vec<u8>],
) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&u2(access_flags));
    bytes.extend_from_slice(&u2(name_index));
    bytes.extend_from_slice(&u2(descriptor_index));
    bytes.extend_from_slice(&u2(attributes.len() as u16));
    for attribute in attributes {
        bytes.extend_from_slice(attribute);
    }
    bytes
}

// ---------------------------------------------------------------------------
// Minimal classfile builder
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct MinimalClassfileOptions {
    minor_version: u16,
    major_version: u16,
    extra_cp_bytes: Vec<u8>,
    extra_cp_count: u16,
    access_flags: u16,
    this_class: u16,
    super_class: u16,
    interfaces: Vec<u16>,
    fields_count: u16,
    fields_bytes: Vec<u8>,
    methods_count: u16,
    methods_bytes: Vec<u8>,
    class_attrs_count: u16,
    class_attrs_bytes: Vec<u8>,
}

impl Default for MinimalClassfileOptions {
    fn default() -> Self {
        Self {
            minor_version: 0,
            major_version: 52,
            extra_cp_bytes: Vec::new(),
            extra_cp_count: 0,
            access_flags: 0x0021,
            this_class: 2,
            super_class: 4,
            interfaces: Vec::new(),
            fields_count: 0,
            fields_bytes: Vec::new(),
            methods_count: 0,
            methods_bytes: Vec::new(),
            class_attrs_count: 0,
            class_attrs_bytes: Vec::new(),
        }
    }
}

fn minimal_classfile() -> Vec<u8> {
    minimal_classfile_with_options(MinimalClassfileOptions::default())
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
    bytes.extend_from_slice(&u2(options.minor_version));
    bytes.extend_from_slice(&u2(options.major_version));
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
    bytes.extend_from_slice(&u2(options.fields_count));
    bytes.extend_from_slice(&options.fields_bytes);
    bytes.extend_from_slice(&u2(options.methods_count));
    bytes.extend_from_slice(&options.methods_bytes);
    bytes.extend_from_slice(&u2(options.class_attrs_count));
    bytes.extend_from_slice(&options.class_attrs_bytes);
    bytes
}

/// Build a classfile with a single method containing a Code attribute with the
/// given bytecode.  `extra_cp_bytes` / `extra_cp_count` allow adding entries
/// beyond the base 4 that every minimal classfile already has, plus the 3
/// required for the method (name, descriptor, "Code").
fn classfile_with_code(code_bytes: &[u8], extra_cp_bytes: Vec<u8>, extra_cp_count: u16) -> Vec<u8> {
    // The first 3 extra CP entries are always: "m" (index 5), "()V" (index 6),
    // "Code" (index 7).  Caller-provided extras start at index 8.
    let mut cp_bytes = Vec::new();
    cp_bytes.extend_from_slice(&utf8_entry_bytes("m"));
    cp_bytes.extend_from_slice(&utf8_entry_bytes("()V"));
    cp_bytes.extend_from_slice(&utf8_entry_bytes("Code"));
    cp_bytes.extend_from_slice(&extra_cp_bytes);

    let mut code_payload = Vec::new();
    code_payload.extend_from_slice(&u2(10)); // max_stack
    code_payload.extend_from_slice(&u2(10)); // max_locals
    code_payload.extend_from_slice(&u4(code_bytes.len() as u32));
    code_payload.extend_from_slice(code_bytes);
    code_payload.extend_from_slice(&u2(0)); // exception_table_length
    code_payload.extend_from_slice(&u2(0)); // attributes_count

    minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: cp_bytes,
        extra_cp_count: 3 + extra_cp_count,
        methods_count: 1,
        methods_bytes: method_info_blob(0x0001, 5, 6, &[make_attribute_blob(7, &code_payload)]),
        ..MinimalClassfileOptions::default()
    })
}

/// Short-hand: classfile with code and no extra CP entries beyond the method
/// boilerplate.
fn classfile_with_code_only(code_bytes: &[u8]) -> Vec<u8> {
    classfile_with_code(code_bytes, Vec::new(), 0)
}

/// Parse the classfile and return the instruction list from the first method's
/// Code attribute.
fn parse_instructions(raw: &[u8]) -> TestResult<Vec<Instruction>> {
    let parsed = parse_class(raw)?;
    let code_attr = parsed.methods[0]
        .attributes
        .iter()
        .find_map(|attr| match attr {
            pytecode_engine::raw::AttributeInfo::Code(code) => Some(code),
            _ => None,
        })
        .ok_or("Code attribute not found")?;
    Ok(code_attr.code.clone())
}

// =========================================================================
// 6a. Constant Pool Entry Tests
// =========================================================================

#[test]
fn cp_utf8_empty_string() -> TestResult<()> {
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: utf8_entry_bytes(""),
        extra_cp_count: 1,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    match &parsed.constant_pool[5] {
        Some(ConstantPoolEntry::Utf8(info)) => assert!(info.bytes.is_empty()),
        other => panic!("expected Utf8, got {other:?}"),
    }
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn cp_utf8_ascii() -> TestResult<()> {
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: utf8_entry_bytes("hello"),
        extra_cp_count: 1,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    match &parsed.constant_pool[5] {
        Some(ConstantPoolEntry::Utf8(info)) => assert_eq!(info.bytes, b"hello"),
        other => panic!("expected Utf8, got {other:?}"),
    }
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn cp_utf8_multibyte_mutf8() -> TestResult<()> {
    // \u{0000} is encoded as 0xC0 0x80 in MUTF-8
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: utf8_entry_bytes("\u{00E9}"), // é -> 2-byte MUTF-8
        extra_cp_count: 1,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    assert!(matches!(
        &parsed.constant_pool[5],
        Some(ConstantPoolEntry::Utf8(_))
    ));
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn cp_integer_values() -> TestResult<()> {
    for value in [0u32, 0x7FFF_FFFF, 0x8000_0000] {
        let mut entry = vec![3u8];
        entry.extend_from_slice(&u4(value));
        let raw = minimal_classfile_with_options(MinimalClassfileOptions {
            extra_cp_bytes: entry,
            extra_cp_count: 1,
            ..MinimalClassfileOptions::default()
        });
        let parsed = parse_class(&raw)?;
        match &parsed.constant_pool[5] {
            Some(ConstantPoolEntry::Integer(info)) => assert_eq!(info.value_bytes, value),
            other => panic!("expected Integer, got {other:?}"),
        }
        assert_eq!(write_class(&parsed)?, raw);
    }
    Ok(())
}

#[test]
fn cp_float_values() -> TestResult<()> {
    for bits in [
        0u32,
        0x7FC0_0000, /* NaN */
        0x7F80_0000, /* +Inf */
    ] {
        let mut entry = vec![4u8];
        entry.extend_from_slice(&u4(bits));
        let raw = minimal_classfile_with_options(MinimalClassfileOptions {
            extra_cp_bytes: entry,
            extra_cp_count: 1,
            ..MinimalClassfileOptions::default()
        });
        let parsed = parse_class(&raw)?;
        match &parsed.constant_pool[5] {
            Some(ConstantPoolEntry::Float(info)) => assert_eq!(info.value_bytes, bits),
            other => panic!("expected Float, got {other:?}"),
        }
        assert_eq!(write_class(&parsed)?, raw);
    }
    Ok(())
}

#[test]
fn cp_long_two_slots() -> TestResult<()> {
    let mut entry = vec![5u8];
    entry.extend_from_slice(&u4(0x11111111));
    entry.extend_from_slice(&u4(0x22222222));
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: entry,
        extra_cp_count: 2, // occupies two slots
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    match &parsed.constant_pool[5] {
        Some(ConstantPoolEntry::Long(info)) => {
            assert_eq!(info.high_bytes, 0x11111111);
            assert_eq!(info.low_bytes, 0x22222222);
        }
        other => panic!("expected Long, got {other:?}"),
    }
    assert!(parsed.constant_pool[6].is_none(), "gap slot must be None");
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn cp_double_two_slots() -> TestResult<()> {
    let mut entry = vec![6u8];
    entry.extend_from_slice(&u4(0xAAAA_BBBB));
    entry.extend_from_slice(&u4(0xCCCC_DDDD));
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: entry,
        extra_cp_count: 2,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    match &parsed.constant_pool[5] {
        Some(ConstantPoolEntry::Double(info)) => {
            assert_eq!(info.high_bytes, 0xAAAA_BBBB);
            assert_eq!(info.low_bytes, 0xCCCC_DDDD);
        }
        other => panic!("expected Double, got {other:?}"),
    }
    assert!(parsed.constant_pool[6].is_none());
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn cp_class_entry() -> TestResult<()> {
    let mut cp = utf8_entry_bytes("SomeClass");
    cp.extend_from_slice(&class_entry_bytes(5));
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: cp,
        extra_cp_count: 2,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    match &parsed.constant_pool[6] {
        Some(ConstantPoolEntry::Class(info)) => assert_eq!(info.name_index, Utf8Index(5)),
        other => panic!("expected Class, got {other:?}"),
    }
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn cp_string_entry() -> TestResult<()> {
    let mut cp = utf8_entry_bytes("some string value");
    let mut string_entry = vec![8u8];
    string_entry.extend_from_slice(&u2(5));
    cp.extend_from_slice(&string_entry);
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: cp,
        extra_cp_count: 2,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    match &parsed.constant_pool[6] {
        Some(ConstantPoolEntry::String(info)) => assert_eq!(info.string_index, Utf8Index(5)),
        other => panic!("expected String, got {other:?}"),
    }
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn cp_fieldref_entry() -> TestResult<()> {
    // CP layout: [5]=Utf8 "fieldName", [6]=Utf8 "I", [7]=NameAndType(5,6),
    //            [8]=FieldRef(2, 7) (class 2 = TestClass)
    let mut cp = Vec::new();
    cp.extend_from_slice(&utf8_entry_bytes("fieldName"));
    cp.extend_from_slice(&utf8_entry_bytes("I"));
    // NameAndType
    cp.push(12);
    cp.extend_from_slice(&u2(5));
    cp.extend_from_slice(&u2(6));
    // FieldRef
    cp.push(9);
    cp.extend_from_slice(&u2(2)); // class_index -> TestClass
    cp.extend_from_slice(&u2(7)); // name_and_type_index
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: cp,
        extra_cp_count: 4,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    match &parsed.constant_pool[8] {
        Some(ConstantPoolEntry::FieldRef(info)) => {
            assert_eq!(info.class_index, ClassIndex(2));
            assert_eq!(info.name_and_type_index, NameAndTypeIndex(7));
        }
        other => panic!("expected FieldRef, got {other:?}"),
    }
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn cp_methodref_entry() -> TestResult<()> {
    let mut cp = Vec::new();
    cp.extend_from_slice(&utf8_entry_bytes("<init>"));
    cp.extend_from_slice(&utf8_entry_bytes("()V"));
    cp.push(12);
    cp.extend_from_slice(&u2(5));
    cp.extend_from_slice(&u2(6));
    // MethodRef
    cp.push(10);
    cp.extend_from_slice(&u2(4)); // super_class
    cp.extend_from_slice(&u2(7));
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: cp,
        extra_cp_count: 4,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    match &parsed.constant_pool[8] {
        Some(ConstantPoolEntry::MethodRef(info)) => {
            assert_eq!(info.class_index, ClassIndex(4));
            assert_eq!(info.name_and_type_index, NameAndTypeIndex(7));
        }
        other => panic!("expected MethodRef, got {other:?}"),
    }
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn cp_interface_methodref_entry() -> TestResult<()> {
    let mut cp = Vec::new();
    cp.extend_from_slice(&utf8_entry_bytes("run"));
    cp.extend_from_slice(&utf8_entry_bytes("()V"));
    cp.push(12);
    cp.extend_from_slice(&u2(5));
    cp.extend_from_slice(&u2(6));
    // InterfaceMethodRef
    cp.push(11);
    cp.extend_from_slice(&u2(2));
    cp.extend_from_slice(&u2(7));
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: cp,
        extra_cp_count: 4,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    match &parsed.constant_pool[8] {
        Some(ConstantPoolEntry::InterfaceMethodRef(info)) => {
            assert_eq!(info.class_index, ClassIndex(2));
            assert_eq!(info.name_and_type_index, NameAndTypeIndex(7));
        }
        other => panic!("expected InterfaceMethodRef, got {other:?}"),
    }
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn cp_name_and_type_entry() -> TestResult<()> {
    let mut cp = Vec::new();
    cp.extend_from_slice(&utf8_entry_bytes("myField"));
    cp.extend_from_slice(&utf8_entry_bytes("J"));
    cp.push(12);
    cp.extend_from_slice(&u2(5));
    cp.extend_from_slice(&u2(6));
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: cp,
        extra_cp_count: 3,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    match &parsed.constant_pool[7] {
        Some(ConstantPoolEntry::NameAndType(info)) => {
            assert_eq!(info.name_index, Utf8Index(5));
            assert_eq!(info.descriptor_index, Utf8Index(6));
        }
        other => panic!("expected NameAndType, got {other:?}"),
    }
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn cp_method_handle_all_reference_kinds() -> TestResult<()> {
    for ref_kind in 1u8..=9 {
        // We need a MethodRef / FieldRef target.  Re-use CP[2] (this_class)
        // + a NameAndType for the reference_index.
        let mut cp = Vec::new();
        cp.extend_from_slice(&utf8_entry_bytes("x"));
        cp.extend_from_slice(&utf8_entry_bytes("I"));
        // NameAndType at index 7
        cp.push(12);
        cp.extend_from_slice(&u2(5));
        cp.extend_from_slice(&u2(6));
        // FieldRef at index 8 (good target for kinds 1-4)
        cp.push(9);
        cp.extend_from_slice(&u2(2));
        cp.extend_from_slice(&u2(7));
        // MethodHandle at index 9
        cp.push(15);
        cp.push(ref_kind);
        cp.extend_from_slice(&u2(8));
        let raw = minimal_classfile_with_options(MinimalClassfileOptions {
            extra_cp_bytes: cp,
            extra_cp_count: 5,
            ..MinimalClassfileOptions::default()
        });
        let parsed = parse_class(&raw)?;
        match &parsed.constant_pool[9] {
            Some(ConstantPoolEntry::MethodHandle(info)) => {
                assert_eq!(info.reference_kind, ref_kind);
                assert_eq!(info.reference_index, CpIndex(8));
            }
            other => panic!("expected MethodHandle kind {ref_kind}, got {other:?}"),
        }
        assert_eq!(write_class(&parsed)?, raw);
    }
    Ok(())
}

#[test]
fn cp_method_type_entry() -> TestResult<()> {
    let mut cp = Vec::new();
    cp.extend_from_slice(&utf8_entry_bytes("(I)V"));
    cp.push(16);
    cp.extend_from_slice(&u2(5));
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: cp,
        extra_cp_count: 2,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    match &parsed.constant_pool[6] {
        Some(ConstantPoolEntry::MethodType(info)) => {
            assert_eq!(info.descriptor_index, Utf8Index(5))
        }
        other => panic!("expected MethodType, got {other:?}"),
    }
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn cp_dynamic_entry() -> TestResult<()> {
    let mut cp = Vec::new();
    cp.extend_from_slice(&utf8_entry_bytes("dynName"));
    cp.extend_from_slice(&utf8_entry_bytes("LDyn;"));
    cp.push(12);
    cp.extend_from_slice(&u2(5));
    cp.extend_from_slice(&u2(6));
    // Dynamic (tag 17)
    cp.push(17);
    cp.extend_from_slice(&u2(0)); // bootstrap_method_attr_index
    cp.extend_from_slice(&u2(7)); // name_and_type_index
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: cp,
        extra_cp_count: 4,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    match &parsed.constant_pool[8] {
        Some(ConstantPoolEntry::Dynamic(info)) => {
            assert_eq!(info.bootstrap_method_attr_index, BootstrapMethodIndex(0));
            assert_eq!(info.name_and_type_index, NameAndTypeIndex(7));
        }
        other => panic!("expected Dynamic, got {other:?}"),
    }
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn cp_invoke_dynamic_entry() -> TestResult<()> {
    let mut cp = Vec::new();
    cp.extend_from_slice(&utf8_entry_bytes("invDynName"));
    cp.extend_from_slice(&utf8_entry_bytes("()V"));
    cp.push(12);
    cp.extend_from_slice(&u2(5));
    cp.extend_from_slice(&u2(6));
    // InvokeDynamic (tag 18)
    cp.push(18);
    cp.extend_from_slice(&u2(0));
    cp.extend_from_slice(&u2(7));
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: cp,
        extra_cp_count: 4,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    match &parsed.constant_pool[8] {
        Some(ConstantPoolEntry::InvokeDynamic(info)) => {
            assert_eq!(info.bootstrap_method_attr_index, BootstrapMethodIndex(0));
            assert_eq!(info.name_and_type_index, NameAndTypeIndex(7));
        }
        other => panic!("expected InvokeDynamic, got {other:?}"),
    }
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn cp_module_entry() -> TestResult<()> {
    let mut cp = Vec::new();
    cp.extend_from_slice(&utf8_entry_bytes("my.module"));
    cp.push(19); // Module tag
    cp.extend_from_slice(&u2(5));
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: cp,
        extra_cp_count: 2,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    match &parsed.constant_pool[6] {
        Some(ConstantPoolEntry::Module(info)) => assert_eq!(info.name_index, Utf8Index(5)),
        other => panic!("expected Module, got {other:?}"),
    }
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn cp_package_entry() -> TestResult<()> {
    let mut cp = Vec::new();
    cp.extend_from_slice(&utf8_entry_bytes("my/package"));
    cp.push(20); // Package tag
    cp.extend_from_slice(&u2(5));
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: cp,
        extra_cp_count: 2,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    match &parsed.constant_pool[6] {
        Some(ConstantPoolEntry::Package(info)) => assert_eq!(info.name_index, Utf8Index(5)),
        other => panic!("expected Package, got {other:?}"),
    }
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

// =========================================================================
// 6b. Instruction Opcode Tests
// =========================================================================

// ---- Simple (zero-operand) opcodes ----

#[test]
fn opcode_simple_zero_operand() -> TestResult<()> {
    // Test a representative set of simple opcodes.  We pack them into a single
    // code block and assert each parsed instruction is Simple with the expected
    // opcode and offset.
    let opcodes: Vec<u8> = vec![
        0x00, // nop
        0x01, // aconst_null
        0x02, // iconst_m1
        0x03, 0x04, 0x05, 0x06, 0x07, 0x08, // iconst_0..iconst_5
        0x09, 0x0A, // lconst_0, lconst_1
        0x0B, 0x0C, 0x0D, // fconst_0..fconst_2
        0x0E, 0x0F, // dconst_0, dconst_1
        0x1A, 0x1B, 0x1C, 0x1D, // iload_0..iload_3
        0x1E, 0x1F, 0x20, 0x21, // lload_0..lload_3
        0x22, 0x23, 0x24, 0x25, // fload_0..fload_3
        0x26, 0x27, 0x28, 0x29, // dload_0..dload_3
        0x2A, 0x2B, 0x2C, 0x2D, // aload_0..aload_3
        0x2E, 0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, // iaload..saload
        0x3B, 0x3C, 0x3D, 0x3E, // istore_0..istore_3
        0x3F, 0x40, 0x41, 0x42, // lstore_0..lstore_3
        0x43, 0x44, 0x45, 0x46, // fstore_0..fstore_3
        0x47, 0x48, 0x49, 0x4A, // dstore_0..dstore_3
        0x4B, 0x4C, 0x4D, 0x4E, // astore_0..astore_3
        0x4F, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, // iastore..sastore
        0x57, 0x58, // pop, pop2
        0x59, 0x5A, 0x5B, 0x5C, 0x5D, 0x5E, 0x5F, // dup..swap
        0x60, 0x61, 0x62, 0x63, // iadd, ladd, fadd, dadd
        0x64, 0x65, 0x66, 0x67, // isub..dsub
        0x68, 0x69, 0x6A, 0x6B, // imul..dmul
        0x6C, 0x6D, 0x6E, 0x6F, // idiv..ddiv
        0x70, 0x71, 0x72, 0x73, // irem..drem
        0x74, 0x75, 0x76, 0x77, // ineg..dneg
        0x78, 0x79, 0x7A, 0x7B, 0x7C, 0x7D, // ishl..lushr
        0x7E, 0x7F, 0x80, 0x81, 0x82, 0x83, // iand..lxor
        0x85, 0x86, 0x87, // i2l, i2f, i2d
        0x88, 0x89, 0x8A, // l2i, l2f, l2d
        0x8B, 0x8C, 0x8D, // f2i, f2l, f2d
        0x8E, 0x8F, 0x90, // d2i, d2l, d2f
        0x91, 0x92, 0x93, // i2b, i2c, i2s
        0x94, 0x95, 0x96, 0x97, 0x98, // lcmp, fcmpl, fcmpg, dcmpl, dcmpg
        0xAC, // ireturn
        0xAD, // lreturn
        0xAE, // freturn
        0xAF, // dreturn
        0xB0, // areturn
        0xB1, // return
        0xBE, // arraylength
        0xBF, // athrow
        0xC2, // monitorenter
        0xC3, // monitorexit
    ];
    let raw = classfile_with_code_only(&opcodes);
    let instructions = parse_instructions(&raw)?;
    assert_eq!(instructions.len(), opcodes.len());
    for (i, insn) in instructions.iter().enumerate() {
        match insn {
            Instruction::Simple { opcode, offset } => {
                assert_eq!(*opcode, opcodes[i], "opcode mismatch at index {i}");
                assert_eq!(*offset, i as u32, "offset mismatch at index {i}");
            }
            other => panic!("expected Simple for 0x{:02X}, got {other:?}", opcodes[i]),
        }
    }
    // Roundtrip
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

// ---- bipush (1-byte operand) ----

#[test]
fn opcode_bipush() -> TestResult<()> {
    let raw = classfile_with_code_only(&[0x10, 42]);
    let instructions = parse_instructions(&raw)?;
    assert_eq!(instructions.len(), 1);
    assert!(matches!(
        instructions[0],
        Instruction::Byte {
            opcode: 0x10,
            offset: 0,
            value: 42,
        }
    ));
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn opcode_bipush_negative() -> TestResult<()> {
    let raw = classfile_with_code_only(&[0x10, 0xFF]); // -1 as i8
    let instructions = parse_instructions(&raw)?;
    assert!(matches!(
        instructions[0],
        Instruction::Byte {
            opcode: 0x10,
            value: -1,
            ..
        }
    ));
    Ok(())
}

// ---- sipush (2-byte operand) ----

#[test]
fn opcode_sipush() -> TestResult<()> {
    let raw = classfile_with_code_only(&[0x11, 0x01, 0x00]); // 256
    let instructions = parse_instructions(&raw)?;
    assert!(matches!(
        instructions[0],
        Instruction::Short {
            opcode: 0x11,
            value: 256,
            ..
        }
    ));
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

// ---- Local-index opcodes ----

#[test]
fn opcode_local_index() -> TestResult<()> {
    let opcodes_with_index: Vec<(u8, u8)> = vec![
        (0x15, 0), // iload
        (0x16, 1), // lload
        (0x17, 2), // fload
        (0x18, 3), // dload
        (0x19, 4), // aload
        (0x36, 5), // istore
        (0x37, 0), // lstore
        (0x38, 1), // fstore
        (0x39, 2), // dstore
        (0x3A, 3), // astore
        (0xA9, 0), // ret
    ];
    let mut code = Vec::new();
    for &(op, idx) in &opcodes_with_index {
        code.push(op);
        code.push(idx);
    }
    let raw = classfile_with_code_only(&code);
    let instructions = parse_instructions(&raw)?;
    assert_eq!(instructions.len(), opcodes_with_index.len());
    let mut expected_offset = 0u32;
    for (i, &(op, idx)) in opcodes_with_index.iter().enumerate() {
        match instructions[i] {
            Instruction::LocalIndex {
                opcode,
                offset,
                index,
            } => {
                assert_eq!(opcode, op);
                assert_eq!(offset, expected_offset);
                assert_eq!(index, idx);
            }
            ref other => panic!("expected LocalIndex for 0x{op:02X}, got {other:?}"),
        }
        expected_offset += 2;
    }
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

// ---- ldc (1-byte CP index) ----

#[test]
fn opcode_ldc() -> TestResult<()> {
    // We need a valid CP entry at index 8 to reference.
    let mut extra_cp = Vec::new();
    // Integer at index 8
    extra_cp.push(3u8);
    extra_cp.extend_from_slice(&u4(99));
    let raw = classfile_with_code(&[0x12, 8], extra_cp, 1);
    let instructions = parse_instructions(&raw)?;
    assert!(matches!(
        instructions[0],
        Instruction::ConstantPoolIndex1 {
            opcode: 0x12,
            index: 8,
            ..
        }
    ));
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

// ---- 2-byte CP index opcodes ----

#[test]
fn opcode_cp_wide_index() -> TestResult<()> {
    // ldc_w, ldc2_w, getstatic, putstatic, getfield, putfield,
    // invokevirtual, invokespecial, invokestatic, new, anewarray,
    // checkcast, instanceof
    let wide_opcodes: Vec<u8> = vec![
        0x13, 0x14, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xBB, 0xBD, 0xC0, 0xC1,
    ];
    // Build CP entries that the indices can refer to
    let mut extra_cp = Vec::new();
    // Integer at index 8
    extra_cp.push(3u8);
    extra_cp.extend_from_slice(&u4(1));
    // Long at index 9 (takes 2 slots)
    extra_cp.push(5u8);
    extra_cp.extend_from_slice(&u4(0));
    extra_cp.extend_from_slice(&u4(0));

    let mut code = Vec::new();
    for &op in &wide_opcodes {
        code.push(op);
        code.extend_from_slice(&u2(8)); // all point to CP[8]
    }
    let raw = classfile_with_code(&code, extra_cp, 3);
    let instructions = parse_instructions(&raw)?;
    assert_eq!(instructions.len(), wide_opcodes.len());
    for (i, insn) in instructions.iter().enumerate() {
        match insn {
            Instruction::ConstantPoolIndexWide(cp_insn) => {
                assert_eq!(cp_insn.opcode, wide_opcodes[i]);
                assert_eq!(cp_insn.index, CpIndex(8));
            }
            other => panic!(
                "expected ConstantPoolIndexWide for 0x{:02X}, got {other:?}",
                wide_opcodes[i]
            ),
        }
    }
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

// ---- Branch (2-byte offset) ----

#[test]
fn opcode_branch() -> TestResult<()> {
    // All branch opcodes: ifeq(0x99)..jsr(0xA8), ifnull(0xC6), ifnonnull(0xC7)
    let branch_opcodes: Vec<u8> = (0x99..=0xA8).chain([0xC6, 0xC7]).collect();
    let mut code = Vec::new();
    for &op in &branch_opcodes {
        code.push(op);
        code.extend_from_slice(&u2(0x0003)); // branch offset = +3 (jump to next insn)
    }
    let raw = classfile_with_code_only(&code);
    let instructions = parse_instructions(&raw)?;
    assert_eq!(instructions.len(), branch_opcodes.len());
    let mut offset = 0u32;
    for (i, insn) in instructions.iter().enumerate() {
        match insn {
            Instruction::Branch(b) => {
                assert_eq!(b.opcode, branch_opcodes[i]);
                assert_eq!(b.branch_offset, 3);
                assert_eq!(b.offset, offset);
            }
            other => panic!(
                "expected Branch for 0x{:02X}, got {other:?}",
                branch_opcodes[i]
            ),
        }
        offset += 3;
    }
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

// ---- BranchWide (4-byte offset) ----

#[test]
fn opcode_branch_wide() -> TestResult<()> {
    for &op in &[0xC8u8, 0xC9] {
        let mut code = vec![op];
        code.extend_from_slice(&u4(0x00001000u32)); // offset as i32
        let raw = classfile_with_code_only(&code);
        let instructions = parse_instructions(&raw)?;
        assert!(matches!(
            instructions[0],
            Instruction::BranchWide {
                opcode,
                branch_offset: 0x1000,
                ..
            } if opcode == op
        ));
        let parsed = parse_class(&raw)?;
        assert_eq!(write_class(&parsed)?, raw);
    }
    Ok(())
}

// ---- iinc ----

#[test]
fn opcode_iinc() -> TestResult<()> {
    // iinc index=3, const=10
    let raw = classfile_with_code_only(&[0x84, 3, 10]);
    let instructions = parse_instructions(&raw)?;
    assert!(matches!(
        instructions[0],
        Instruction::IInc {
            offset: 0,
            index: 3,
            value: 10,
        }
    ));
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn opcode_iinc_negative() -> TestResult<()> {
    // iinc index=0, const=-5 (0xFB)
    let raw = classfile_with_code_only(&[0x84, 0, 0xFB]);
    let instructions = parse_instructions(&raw)?;
    assert!(matches!(
        instructions[0],
        Instruction::IInc {
            index: 0,
            value: -5,
            ..
        }
    ));
    Ok(())
}

// ---- invokeinterface ----

#[test]
fn opcode_invokeinterface() -> TestResult<()> {
    let mut extra_cp = Vec::new();
    // Need an InterfaceMethodRef. Build: Utf8 "run", Utf8 "()V",
    // NameAndType(8,9), InterfaceMethodRef(2,10)
    extra_cp.extend_from_slice(&utf8_entry_bytes("run"));
    extra_cp.extend_from_slice(&utf8_entry_bytes("()V"));
    extra_cp.push(12);
    extra_cp.extend_from_slice(&u2(8));
    extra_cp.extend_from_slice(&u2(9));
    extra_cp.push(11);
    extra_cp.extend_from_slice(&u2(2));
    extra_cp.extend_from_slice(&u2(10));
    // invokeinterface cp_index=11, count=1, reserved=0
    let mut code = vec![0xB9];
    code.extend_from_slice(&u2(11));
    code.push(1);
    code.push(0);
    let raw = classfile_with_code(&code, extra_cp, 4);
    let instructions = parse_instructions(&raw)?;
    assert!(matches!(
        instructions[0],
        Instruction::InvokeInterface(InvokeInterfaceInsn {
            offset: 0,
            index: CpIndex(11),
            count: 1,
            reserved: 0,
        })
    ));
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

// ---- invokedynamic ----

#[test]
fn opcode_invokedynamic() -> TestResult<()> {
    let mut extra_cp = Vec::new();
    extra_cp.extend_from_slice(&utf8_entry_bytes("dynCall"));
    extra_cp.extend_from_slice(&utf8_entry_bytes("()V"));
    extra_cp.push(12);
    extra_cp.extend_from_slice(&u2(8));
    extra_cp.extend_from_slice(&u2(9));
    // InvokeDynamic CP entry (tag 18)
    extra_cp.push(18);
    extra_cp.extend_from_slice(&u2(0)); // bootstrap_method_attr_index
    extra_cp.extend_from_slice(&u2(10)); // name_and_type_index
    // invokedynamic cp_index=11, reserved=0
    let mut code = vec![0xBA];
    code.extend_from_slice(&u2(11));
    code.extend_from_slice(&u2(0));
    let raw = classfile_with_code(&code, extra_cp, 4);
    let instructions = parse_instructions(&raw)?;
    assert!(matches!(
        instructions[0],
        Instruction::InvokeDynamic(InvokeDynamicInsn {
            offset: 0,
            index: CpIndex(11),
            reserved: 0,
        })
    ));
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

// ---- newarray ----

#[test]
fn opcode_newarray_all_types() -> TestResult<()> {
    let types: Vec<(u8, ArrayType)> = vec![
        (4, ArrayType::Boolean),
        (5, ArrayType::Char),
        (6, ArrayType::Float),
        (7, ArrayType::Double),
        (8, ArrayType::Byte),
        (9, ArrayType::Short),
        (10, ArrayType::Int),
        (11, ArrayType::Long),
    ];
    for &(atype_byte, expected_atype) in &types {
        let raw = classfile_with_code_only(&[0xBC, atype_byte]);
        let instructions = parse_instructions(&raw)?;
        match &instructions[0] {
            Instruction::NewArray(NewArrayInsn { offset, atype }) => {
                assert_eq!(*offset, 0);
                assert_eq!(*atype, expected_atype);
            }
            other => panic!("expected NewArray for type {atype_byte}, got {other:?}"),
        }
        let parsed = parse_class(&raw)?;
        assert_eq!(write_class(&parsed)?, raw);
    }
    Ok(())
}

// ---- multianewarray ----

#[test]
fn opcode_multianewarray() -> TestResult<()> {
    let mut extra_cp = Vec::new();
    extra_cp.extend_from_slice(&utf8_entry_bytes("[[I"));
    extra_cp.extend_from_slice(&class_entry_bytes(8));
    let mut code = vec![0xC5];
    code.extend_from_slice(&u2(9)); // class index
    code.push(2); // dimensions
    let raw = classfile_with_code(&code, extra_cp, 2);
    let instructions = parse_instructions(&raw)?;
    assert!(matches!(
        instructions[0],
        Instruction::MultiANewArray {
            offset: 0,
            index: ClassIndex(9),
            dimensions: 2,
        }
    ));
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

// ---- tableswitch ----

#[test]
fn opcode_tableswitch_padding_offsets() -> TestResult<()> {
    // Test at each alignment: instruction at offset 0, 1, 2, 3 mod 4
    for pad_prefix_len in 0..4u32 {
        let mut code = Vec::new();
        // nop padding to shift the tableswitch to the right offset
        code.extend(std::iter::repeat_n(0x00u8, pad_prefix_len as usize));
        code.push(0xAA); // tableswitch
        // padding to 4-byte alignment
        let padding = (4 - ((pad_prefix_len + 1) % 4)) % 4;
        code.extend(std::iter::repeat_n(0x00u8, padding as usize));
        // default offset
        code.extend_from_slice(&(8i32).to_be_bytes());
        // low = 0
        code.extend_from_slice(&(0i32).to_be_bytes());
        // high = 1
        code.extend_from_slice(&(1i32).to_be_bytes());
        // 2 offsets
        code.extend_from_slice(&(4i32).to_be_bytes());
        code.extend_from_slice(&(8i32).to_be_bytes());

        let raw = classfile_with_code_only(&code);
        let instructions = parse_instructions(&raw)?;
        let ts_insn = instructions
            .iter()
            .find(|i| matches!(i, Instruction::TableSwitch(_)))
            .expect("tableswitch not found");
        match ts_insn {
            Instruction::TableSwitch(ts) => {
                assert_eq!(ts.offset, pad_prefix_len);
                assert_eq!(ts.default_offset, 8);
                assert_eq!(ts.low, 0);
                assert_eq!(ts.high, 1);
                assert_eq!(ts.offsets, vec![4, 8]);
            }
            _ => unreachable!(),
        }
        let parsed = parse_class(&raw)?;
        assert_eq!(write_class(&parsed)?, raw);
    }
    Ok(())
}

// ---- lookupswitch ----

#[test]
fn opcode_lookupswitch_zero_pairs() -> TestResult<()> {
    // instruction at offset 0 → padding = 3
    let mut code = vec![0xAB];
    code.extend_from_slice(&[0, 0, 0]); // 3 bytes padding
    code.extend_from_slice(&(4i32).to_be_bytes()); // default
    code.extend_from_slice(&(0i32).to_be_bytes()); // npairs = 0
    let raw = classfile_with_code_only(&code);
    let instructions = parse_instructions(&raw)?;
    assert!(matches!(
        &instructions[0],
        Instruction::LookupSwitch(LookupSwitchInsn {
            offset: 0,
            default_offset: 4,
            pairs,
        }) if pairs.is_empty()
    ));
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn opcode_lookupswitch_one_pair() -> TestResult<()> {
    let mut code = vec![0xAB];
    code.extend_from_slice(&[0, 0, 0]);
    code.extend_from_slice(&(20i32).to_be_bytes());
    code.extend_from_slice(&(1i32).to_be_bytes()); // npairs = 1
    code.extend_from_slice(&(100i32).to_be_bytes()); // match
    code.extend_from_slice(&(10i32).to_be_bytes()); // offset
    let raw = classfile_with_code_only(&code);
    let instructions = parse_instructions(&raw)?;
    match &instructions[0] {
        Instruction::LookupSwitch(ls) => {
            assert_eq!(ls.default_offset, 20);
            assert_eq!(ls.pairs.len(), 1);
            assert_eq!(ls.pairs[0].match_value, 100);
            assert_eq!(ls.pairs[0].offset, 10);
        }
        other => panic!("expected LookupSwitch, got {other:?}"),
    }
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn opcode_lookupswitch_multiple_pairs() -> TestResult<()> {
    let mut code = vec![0xAB];
    code.extend_from_slice(&[0, 0, 0]);
    code.extend_from_slice(&(30i32).to_be_bytes());
    code.extend_from_slice(&(3i32).to_be_bytes()); // 3 pairs
    for i in 0..3i32 {
        code.extend_from_slice(&(i * 10).to_be_bytes());
        code.extend_from_slice(&((i + 1) * 5).to_be_bytes());
    }
    let raw = classfile_with_code_only(&code);
    let instructions = parse_instructions(&raw)?;
    match &instructions[0] {
        Instruction::LookupSwitch(ls) => {
            assert_eq!(ls.pairs.len(), 3);
            assert_eq!(
                ls.pairs[0],
                MatchOffsetPair {
                    match_value: 0,
                    offset: 5
                }
            );
            assert_eq!(
                ls.pairs[1],
                MatchOffsetPair {
                    match_value: 10,
                    offset: 10
                }
            );
            assert_eq!(
                ls.pairs[2],
                MatchOffsetPair {
                    match_value: 20,
                    offset: 15
                }
            );
        }
        other => panic!("expected LookupSwitch, got {other:?}"),
    }
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

// ---- wide ----

#[test]
fn opcode_wide_iload() -> TestResult<()> {
    let mut code = vec![0xC4, 0x15]; // wide iload
    code.extend_from_slice(&u2(300)); // index
    let raw = classfile_with_code_only(&code);
    let instructions = parse_instructions(&raw)?;
    assert!(matches!(
        &instructions[0],
        Instruction::Wide(WideInstruction {
            offset: 0,
            opcode: 0x15,
            index: 300,
            value: None,
        })
    ));
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn opcode_wide_istore() -> TestResult<()> {
    let mut code = vec![0xC4, 0x36]; // wide istore
    code.extend_from_slice(&u2(500));
    let raw = classfile_with_code_only(&code);
    let instructions = parse_instructions(&raw)?;
    assert!(matches!(
        &instructions[0],
        Instruction::Wide(WideInstruction {
            opcode: 0x36,
            index: 500,
            value: None,
            ..
        })
    ));
    Ok(())
}

#[test]
fn opcode_wide_all_load_store() -> TestResult<()> {
    let wide_opcodes = [
        0x15u8, 0x16, 0x17, 0x18, 0x19, // iload..aload
        0x36, 0x37, 0x38, 0x39, 0x3A, // istore..astore
        0xA9, // ret
    ];
    for &op in &wide_opcodes {
        let mut code = vec![0xC4, op];
        code.extend_from_slice(&u2(256));
        let raw = classfile_with_code_only(&code);
        let instructions = parse_instructions(&raw)?;
        match &instructions[0] {
            Instruction::Wide(w) => {
                assert_eq!(w.opcode, op);
                assert_eq!(w.index, 256);
                assert!(w.value.is_none());
            }
            other => panic!("expected Wide for 0x{op:02X}, got {other:?}"),
        }
        let parsed = parse_class(&raw)?;
        assert_eq!(write_class(&parsed)?, raw);
    }
    Ok(())
}

#[test]
fn opcode_wide_iinc() -> TestResult<()> {
    let mut code = vec![0xC4, 0x84]; // wide iinc
    code.extend_from_slice(&u2(300)); // index
    let value_bytes = (-500i16).to_be_bytes();
    code.extend_from_slice(&value_bytes); // value
    let raw = classfile_with_code_only(&code);
    let instructions = parse_instructions(&raw)?;
    assert!(matches!(
        &instructions[0],
        Instruction::Wide(WideInstruction {
            opcode: 0x84,
            index: 300,
            value: Some(-500),
            ..
        })
    ));
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

// =========================================================================
// 6c. Malformed Input Tests
// =========================================================================

#[test]
fn malformed_truncated_at_magic() {
    let err = parse_class(&[0xCA, 0xFE]).unwrap_err();
    assert!(matches!(err.kind, EngineErrorKind::UnexpectedEof { .. }));
}

#[test]
fn malformed_truncated_at_version() {
    let mut data = MAGIC.to_be_bytes().to_vec();
    data.extend_from_slice(&u2(0)); // minor only
    let err = parse_class(&data).unwrap_err();
    assert!(matches!(err.kind, EngineErrorKind::UnexpectedEof { .. }));
}

#[test]
fn malformed_truncated_at_cp() {
    let mut data = MAGIC.to_be_bytes().to_vec();
    data.extend_from_slice(&u2(0));
    data.extend_from_slice(&u2(52));
    data.extend_from_slice(&u2(10)); // cp_count = 10, but no entries follow
    let err = parse_class(&data).unwrap_err();
    assert!(matches!(err.kind, EngineErrorKind::UnexpectedEof { .. }));
}

#[test]
fn malformed_truncated_at_methods() {
    let mut raw = minimal_classfile();
    // Truncate: remove last 2 bytes (class_attrs_count) to simulate
    // truncation after methods_count
    raw.truncate(raw.len() - 2);
    // Re-set methods_count to 1 so the parser tries to read a method
    // Actually, easier: just cut a complete minimal classfile very short
    // Right after access_flags + this_class + super_class
    let data = &raw[..raw.len() - 4]; // cut off methods_count + class_attrs
    let err = parse_class(data).unwrap_err();
    assert!(matches!(err.kind, EngineErrorKind::UnexpectedEof { .. }));
}

#[test]
fn malformed_invalid_magic_zeros() {
    let mut data = minimal_classfile();
    data[0] = 0;
    data[1] = 0;
    data[2] = 0;
    data[3] = 0;
    let err = parse_class(&data).unwrap_err();
    assert!(matches!(
        err.kind,
        EngineErrorKind::InvalidMagic {
            found: 0x00000000,
            expected: 0xCAFEBABE,
        }
    ));
}

#[test]
fn malformed_invalid_magic_deadbeef() {
    let mut data = minimal_classfile();
    let magic = 0xDEADBEEFu32.to_be_bytes();
    data[0..4].copy_from_slice(&magic);
    let err = parse_class(&data).unwrap_err();
    assert!(matches!(
        err.kind,
        EngineErrorKind::InvalidMagic {
            found: 0xDEADBEEF,
            ..
        }
    ));
}

#[test]
fn malformed_invalid_cp_tags() {
    for bad_tag in [0u8, 2, 13, 14, 21, 255] {
        let mut entry = vec![bad_tag];
        // Add enough trailing bytes so it won't hit EOF before the tag error
        entry.extend_from_slice(&[0; 10]);
        let raw = minimal_classfile_with_options(MinimalClassfileOptions {
            extra_cp_bytes: entry,
            extra_cp_count: 1,
            ..MinimalClassfileOptions::default()
        });
        let err = parse_class(&raw).unwrap_err();
        assert!(
            matches!(err.kind, EngineErrorKind::InvalidConstantPoolTag { tag } if tag == bad_tag),
            "expected InvalidConstantPoolTag for tag {bad_tag}, got {:?}",
            err.kind,
        );
    }
}

#[test]
fn malformed_invalid_opcode() {
    for bad_opcode in [0xCBu8, 0xFE, 0xFF] {
        let raw = classfile_with_code_only(&[bad_opcode]);
        let err = parse_class(&raw).unwrap_err();
        assert!(
            matches!(err.kind, EngineErrorKind::InvalidOpcode { opcode } if opcode == bad_opcode),
            "expected InvalidOpcode for 0x{bad_opcode:02X}, got {:?}",
            err.kind,
        );
    }
}

#[test]
fn malformed_invalid_wide_opcode() {
    // wide + nop (0xC4, 0x00) should fail
    let raw = classfile_with_code_only(&[0xC4, 0x00, 0x00, 0x00]);
    let err = parse_class(&raw).unwrap_err();
    assert!(matches!(
        err.kind,
        EngineErrorKind::InvalidWideOpcode { opcode: 0x00 }
    ));
}

#[test]
fn malformed_invalid_wide_opcode_various() {
    // wide + pop (0x57), wide + iadd (0x60)
    for &bad_wide_op in &[0x57u8, 0x60, 0xB1] {
        let raw = classfile_with_code_only(&[0xC4, bad_wide_op, 0x00, 0x00]);
        let err = parse_class(&raw).unwrap_err();
        assert!(
            matches!(err.kind, EngineErrorKind::InvalidWideOpcode { opcode } if opcode == bad_wide_op),
            "expected InvalidWideOpcode for 0x{bad_wide_op:02X}, got {:?}",
            err.kind,
        );
    }
}

#[test]
fn malformed_invalid_newarray_type() {
    for bad_atype in [0u8, 3, 12, 255] {
        let raw = classfile_with_code_only(&[0xBC, bad_atype]);
        let err = parse_class(&raw).unwrap_err();
        assert!(
            matches!(err.kind, EngineErrorKind::InvalidArrayType { atype } if atype == bad_atype),
            "expected InvalidArrayType for {bad_atype}, got {:?}",
            err.kind,
        );
    }
}

// =========================================================================
// 6d. Boundary Tests
// =========================================================================

#[test]
fn boundary_empty_class() -> TestResult<()> {
    let raw = minimal_classfile();
    let parsed = parse_class(&raw)?;
    assert_eq!(parsed.interfaces.len(), 0);
    assert_eq!(parsed.fields.len(), 0);
    assert_eq!(parsed.methods.len(), 0);
    assert_eq!(parsed.attributes.len(), 0);
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn boundary_single_nop_code() -> TestResult<()> {
    let raw = classfile_with_code_only(&[0x00]);
    let instructions = parse_instructions(&raw)?;
    assert_eq!(instructions.len(), 1);
    assert!(matches!(
        instructions[0],
        Instruction::Simple {
            opcode: 0x00,
            offset: 0,
        }
    ));
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

// =========================================================================
// 6e. MethodHandle Reference Kind Coverage
// =========================================================================

#[test]
fn cp_method_handle_invalid_reference_kind_zero() {
    // reference_kind = 0 is validated at parse time → error
    let mut cp = Vec::new();
    cp.extend_from_slice(&utf8_entry_bytes("x"));
    cp.extend_from_slice(&utf8_entry_bytes("I"));
    cp.push(12);
    cp.extend_from_slice(&u2(5));
    cp.extend_from_slice(&u2(6));
    cp.push(9);
    cp.extend_from_slice(&u2(2));
    cp.extend_from_slice(&u2(7));
    // MethodHandle with ref_kind=0
    cp.push(15);
    cp.push(0);
    cp.extend_from_slice(&u2(8));
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: cp,
        extra_cp_count: 5,
        ..MinimalClassfileOptions::default()
    });
    let err = parse_class(&raw).unwrap_err();
    assert!(matches!(err.kind, EngineErrorKind::InvalidAttribute { .. }));
}

#[test]
fn cp_method_handle_reference_kind_10() {
    // reference_kind = 10 is out of range 1-9 → error
    let mut cp = Vec::new();
    cp.extend_from_slice(&utf8_entry_bytes("x"));
    cp.extend_from_slice(&utf8_entry_bytes("I"));
    cp.push(12);
    cp.extend_from_slice(&u2(5));
    cp.extend_from_slice(&u2(6));
    cp.push(9);
    cp.extend_from_slice(&u2(2));
    cp.extend_from_slice(&u2(7));
    // MethodHandle with ref_kind=10
    cp.push(15);
    cp.push(10);
    cp.extend_from_slice(&u2(8));
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: cp,
        extra_cp_count: 5,
        ..MinimalClassfileOptions::default()
    });
    let err = parse_class(&raw).unwrap_err();
    assert!(matches!(err.kind, EngineErrorKind::InvalidAttribute { .. }));
}

// =========================================================================
// 6f. newarray Type Coverage (individual tests per type)
// =========================================================================

#[test]
fn newarray_boolean() -> TestResult<()> {
    let raw = classfile_with_code_only(&[0xBC, 4]);
    let insns = parse_instructions(&raw)?;
    assert!(matches!(
        &insns[0],
        Instruction::NewArray(NewArrayInsn {
            atype: ArrayType::Boolean,
            ..
        })
    ));
    Ok(())
}

#[test]
fn newarray_char() -> TestResult<()> {
    let raw = classfile_with_code_only(&[0xBC, 5]);
    let insns = parse_instructions(&raw)?;
    assert!(matches!(
        &insns[0],
        Instruction::NewArray(NewArrayInsn {
            atype: ArrayType::Char,
            ..
        })
    ));
    Ok(())
}

#[test]
fn newarray_float() -> TestResult<()> {
    let raw = classfile_with_code_only(&[0xBC, 6]);
    let insns = parse_instructions(&raw)?;
    assert!(matches!(
        &insns[0],
        Instruction::NewArray(NewArrayInsn {
            atype: ArrayType::Float,
            ..
        })
    ));
    Ok(())
}

#[test]
fn newarray_double() -> TestResult<()> {
    let raw = classfile_with_code_only(&[0xBC, 7]);
    let insns = parse_instructions(&raw)?;
    assert!(matches!(
        &insns[0],
        Instruction::NewArray(NewArrayInsn {
            atype: ArrayType::Double,
            ..
        })
    ));
    Ok(())
}

#[test]
fn newarray_byte() -> TestResult<()> {
    let raw = classfile_with_code_only(&[0xBC, 8]);
    let insns = parse_instructions(&raw)?;
    assert!(matches!(
        &insns[0],
        Instruction::NewArray(NewArrayInsn {
            atype: ArrayType::Byte,
            ..
        })
    ));
    Ok(())
}

#[test]
fn newarray_short() -> TestResult<()> {
    let raw = classfile_with_code_only(&[0xBC, 9]);
    let insns = parse_instructions(&raw)?;
    assert!(matches!(
        &insns[0],
        Instruction::NewArray(NewArrayInsn {
            atype: ArrayType::Short,
            ..
        })
    ));
    Ok(())
}

#[test]
fn newarray_int() -> TestResult<()> {
    let raw = classfile_with_code_only(&[0xBC, 10]);
    let insns = parse_instructions(&raw)?;
    assert!(matches!(
        &insns[0],
        Instruction::NewArray(NewArrayInsn {
            atype: ArrayType::Int,
            ..
        })
    ));
    Ok(())
}

#[test]
fn newarray_long() -> TestResult<()> {
    let raw = classfile_with_code_only(&[0xBC, 11]);
    let insns = parse_instructions(&raw)?;
    assert!(matches!(
        &insns[0],
        Instruction::NewArray(NewArrayInsn {
            atype: ArrayType::Long,
            ..
        })
    ));
    Ok(())
}

// =========================================================================
// Additional round-trip coverage for mixed instruction sequences
// =========================================================================

#[test]
fn mixed_instructions_roundtrip() -> TestResult<()> {
    // A realistic snippet: iconst_0, istore_1, iload_1, bipush 10, if_icmplt +7,
    // iinc 1 1, goto -8, return
    let mut code = vec![
        0x03, // iconst_0
        0x3C, // istore_1
        0x1B, // iload_1
        0x10, 10,   // bipush 10
        0xA1, // if_icmplt
    ];
    code.extend_from_slice(&u2(7)); // branch offset
    code.extend_from_slice(&[0x84, 1, 1]); // iinc 1, 1
    code.push(0xA7); // goto
    code.extend_from_slice(&(-8i16).to_be_bytes()); // branch back
    code.push(0xB1); // return

    let raw = classfile_with_code_only(&code);
    let parsed = parse_class(&raw)?;
    let emitted = write_class(&parsed)?;
    assert_eq!(emitted, raw);

    let instructions = parse_instructions(&raw)?;
    assert_eq!(instructions.len(), 8);
    assert_eq!(instructions[0].opcode(), 0x03);
    assert_eq!(instructions[3].opcode(), 0x10);
    assert_eq!(instructions[4].opcode(), 0xA1);
    assert_eq!(instructions[5].opcode(), 0x84);
    assert_eq!(instructions[6].opcode(), 0xA7);
    assert_eq!(instructions[7].opcode(), 0xB1);
    Ok(())
}

#[test]
fn tableswitch_at_offset_1_roundtrip() -> TestResult<()> {
    // nop + tableswitch to test padding = 2
    let mut code = vec![0x00]; // nop at offset 0
    code.push(0xAA); // tableswitch at offset 1
    code.extend_from_slice(&[0, 0]); // 2 bytes padding (to align to 4)
    code.extend_from_slice(&(16i32).to_be_bytes()); // default
    code.extend_from_slice(&(1i32).to_be_bytes()); // low
    code.extend_from_slice(&(3i32).to_be_bytes()); // high
    code.extend_from_slice(&(4i32).to_be_bytes()); // offset for case 1
    code.extend_from_slice(&(8i32).to_be_bytes()); // offset for case 2
    code.extend_from_slice(&(12i32).to_be_bytes()); // offset for case 3
    let raw = classfile_with_code_only(&code);
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    let instructions = parse_instructions(&raw)?;
    match &instructions[1] {
        Instruction::TableSwitch(ts) => {
            assert_eq!(ts.offset, 1);
            assert_eq!(ts.low, 1);
            assert_eq!(ts.high, 3);
            assert_eq!(ts.offsets.len(), 3);
        }
        other => panic!("expected TableSwitch, got {other:?}"),
    }
    Ok(())
}

#[test]
fn empty_input_returns_eof_error() {
    let err = parse_class(&[]).unwrap_err();
    assert!(matches!(err.kind, EngineErrorKind::UnexpectedEof { .. }));
}

#[test]
#[allow(clippy::approx_constant)]
fn cp_all_17_types_in_one_classfile() -> TestResult<()> {
    // Stress test: pack all 17 CP entry types into one classfile and roundtrip.
    let mut cp = Vec::new();
    // Base CP: [1]=Utf8 "TestClass", [2]=Class(1), [3]=Utf8 "java/lang/Object", [4]=Class(3)
    // Extra entries start at index 5:

    // [5] Utf8 "hello"
    cp.extend_from_slice(&utf8_entry_bytes("hello"));
    // [6] Integer
    cp.push(3);
    cp.extend_from_slice(&u4(42));
    // [7] Float
    cp.push(4);
    cp.extend_from_slice(&u4(f32::to_bits(3.14_f32)));
    // [8] Long (takes 2 slots -> [8],[9])
    cp.push(5);
    cp.extend_from_slice(&u4(0));
    cp.extend_from_slice(&u4(100));
    // [10] Double (takes 2 slots -> [10],[11])
    cp.push(6);
    cp.extend_from_slice(&u4(0x40090000));
    cp.extend_from_slice(&u4(0x00000000));
    // [12] Class -> [5]
    cp.push(7);
    cp.extend_from_slice(&u2(5));
    // [13] String -> [5]
    cp.push(8);
    cp.extend_from_slice(&u2(5));
    // [14] Utf8 "x"
    cp.extend_from_slice(&utf8_entry_bytes("x"));
    // [15] Utf8 "I"
    cp.extend_from_slice(&utf8_entry_bytes("I"));
    // [16] NameAndType(14, 15)
    cp.push(12);
    cp.extend_from_slice(&u2(14));
    cp.extend_from_slice(&u2(15));
    // [17] FieldRef(2, 16)
    cp.push(9);
    cp.extend_from_slice(&u2(2));
    cp.extend_from_slice(&u2(16));
    // [18] MethodRef(4, 16)
    cp.push(10);
    cp.extend_from_slice(&u2(4));
    cp.extend_from_slice(&u2(16));
    // [19] InterfaceMethodRef(2, 16)
    cp.push(11);
    cp.extend_from_slice(&u2(2));
    cp.extend_from_slice(&u2(16));
    // [20] MethodHandle(1, 17)
    cp.push(15);
    cp.push(1);
    cp.extend_from_slice(&u2(17));
    // [21] MethodType -> [15] "I"
    cp.push(16);
    cp.extend_from_slice(&u2(15));
    // [22] Dynamic(0, 16)
    cp.push(17);
    cp.extend_from_slice(&u2(0));
    cp.extend_from_slice(&u2(16));
    // [23] InvokeDynamic(0, 16)
    cp.push(18);
    cp.extend_from_slice(&u2(0));
    cp.extend_from_slice(&u2(16));
    // [24] Utf8 "my.module"
    cp.extend_from_slice(&utf8_entry_bytes("my.module"));
    // [25] Module(24)
    cp.push(19);
    cp.extend_from_slice(&u2(24));
    // [26] Utf8 "my/pkg"
    cp.extend_from_slice(&utf8_entry_bytes("my/pkg"));
    // [27] Package(26)
    cp.push(20);
    cp.extend_from_slice(&u2(26));

    // Total extra CP count: slots 5-27, but Long takes 2 and Double takes 2
    // Slots used: 5,6,7,8,9(gap),10,11(gap),12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27
    // That's entries 5..=27 = 23 extra slots
    let extra_cp_count = 23u16;
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: cp,
        extra_cp_count,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;

    // Verify a few entries
    assert!(matches!(
        &parsed.constant_pool[5],
        Some(ConstantPoolEntry::Utf8(_))
    ));
    assert!(matches!(
        &parsed.constant_pool[6],
        Some(ConstantPoolEntry::Integer(_))
    ));
    assert!(matches!(
        &parsed.constant_pool[7],
        Some(ConstantPoolEntry::Float(_))
    ));
    assert!(matches!(
        &parsed.constant_pool[8],
        Some(ConstantPoolEntry::Long(_))
    ));
    assert!(parsed.constant_pool[9].is_none()); // gap
    assert!(matches!(
        &parsed.constant_pool[10],
        Some(ConstantPoolEntry::Double(_))
    ));
    assert!(parsed.constant_pool[11].is_none()); // gap
    assert!(matches!(
        &parsed.constant_pool[12],
        Some(ConstantPoolEntry::Class(_))
    ));
    assert!(matches!(
        &parsed.constant_pool[13],
        Some(ConstantPoolEntry::String(_))
    ));
    assert!(matches!(
        &parsed.constant_pool[17],
        Some(ConstantPoolEntry::FieldRef(_))
    ));
    assert!(matches!(
        &parsed.constant_pool[18],
        Some(ConstantPoolEntry::MethodRef(_))
    ));
    assert!(matches!(
        &parsed.constant_pool[19],
        Some(ConstantPoolEntry::InterfaceMethodRef(_))
    ));
    assert!(matches!(
        &parsed.constant_pool[20],
        Some(ConstantPoolEntry::MethodHandle(_))
    ));
    assert!(matches!(
        &parsed.constant_pool[21],
        Some(ConstantPoolEntry::MethodType(_))
    ));
    assert!(matches!(
        &parsed.constant_pool[22],
        Some(ConstantPoolEntry::Dynamic(_))
    ));
    assert!(matches!(
        &parsed.constant_pool[23],
        Some(ConstantPoolEntry::InvokeDynamic(_))
    ));
    assert!(matches!(
        &parsed.constant_pool[25],
        Some(ConstantPoolEntry::Module(_))
    ));
    assert!(matches!(
        &parsed.constant_pool[27],
        Some(ConstantPoolEntry::Package(_))
    ));

    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

// =========================================================================
// Additional: breakpoint opcode 0xCA parses as Simple
// =========================================================================

#[test]
fn opcode_breakpoint_parses_as_simple() -> TestResult<()> {
    let raw = classfile_with_code_only(&[0xCA]);
    let instructions = parse_instructions(&raw)?;
    assert!(matches!(
        instructions[0],
        Instruction::Simple {
            opcode: 0xCA,
            offset: 0,
        }
    ));
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}
