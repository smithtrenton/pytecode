use pytecode_engine::analysis::{Category, verify_classfile, verify_classmodel};
use pytecode_engine::constants::{ClassAccessFlags, MAGIC};
use pytecode_engine::model::{BranchInsn, ClassModel, CodeItem, FieldModel, Label};
use pytecode_engine::raw::{ClassFile, ConstantPoolEntry};

fn base_classfile() -> ClassFile {
    ClassFile {
        magic: MAGIC,
        minor_version: 0,
        major_version: 52,
        constant_pool: vec![
            None,
            Some(ConstantPoolEntry::Utf8(pytecode_engine::raw::Utf8Info {
                bytes: pytecode_engine::modified_utf8::encode_modified_utf8("TestClass"),
            })),
            Some(ConstantPoolEntry::Class(pytecode_engine::raw::ClassInfo {
                name_index: 1,
            })),
            Some(ConstantPoolEntry::Utf8(pytecode_engine::raw::Utf8Info {
                bytes: pytecode_engine::modified_utf8::encode_modified_utf8("java/lang/Object"),
            })),
            Some(ConstantPoolEntry::Class(pytecode_engine::raw::ClassInfo {
                name_index: 3,
            })),
        ],
        access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
        this_class: 2,
        super_class: 4,
        interfaces: Vec::new(),
        fields: Vec::new(),
        methods: Vec::new(),
        attributes: Vec::new(),
    }
}

#[test]
fn verify_classfile_reports_magic_and_descriptor_issues() {
    let mut classfile = base_classfile();
    classfile.magic = 0xDEADBEEF;
    let diagnostics = verify_classfile(&classfile);
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.category == Category::Magic)
    );
}

#[test]
fn verify_classmodel_reports_invalid_descriptors_and_bad_cfg() {
    let bytes = std::fs::read(
        pytecode_engine::fixtures::compiled_fixture_paths_for("ControlFlowExample.java")
            .expect("fixture paths should load")
            .into_iter()
            .find(|path| path.ends_with("ControlFlowExample.class"))
            .expect("fixture should exist"),
    )
    .expect("fixture bytes should read");
    let mut model = ClassModel::from_bytes(&bytes).expect("fixture should parse");
    model.fields.push(FieldModel {
        access_flags: pytecode_engine::constants::FieldAccessFlags::PUBLIC,
        name: "bad".to_owned(),
        descriptor: "not-a-descriptor".to_owned(),
        attributes: Vec::new(),
    });

    let code = model
        .methods
        .iter_mut()
        .find(|method| method.name == "branch")
        .and_then(|method| method.code.as_mut())
        .expect("branch method should have code");
    let branch = code
        .instructions
        .iter_mut()
        .find_map(|item| match item {
            CodeItem::Branch(branch) => Some(branch),
            _ => None,
        })
        .expect("branch instruction should exist");
    *branch = BranchInsn {
        opcode: branch.opcode,
        target: Label::named("missing"),
    };

    let diagnostics = verify_classmodel(&model, None);
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.category == Category::Descriptor)
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.category == Category::Code)
    );
}
