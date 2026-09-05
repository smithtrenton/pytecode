#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 262_144 {
        return;
    }
    if let Ok(classfile) = pytecode_engine::parse_class(data) {
        if classfile.methods.len() > 128 || classfile.constant_pool.len() > 8192 {
            return;
        }
        let _ = pytecode_engine::analysis::verify_classfile(&classfile);
        // Keep reference names small before lifting/simulating cloned frames.
        if classfile.constant_pool.iter().flatten().any(|entry| {
            matches!(entry, pytecode_engine::raw::ConstantPoolEntry::Utf8(value) if value.bytes.len() > 512)
        }) {
            return;
        }
        if let Ok(model) = pytecode_engine::model::ClassModel::from_classfile(&classfile) {
            let instructions: usize = model
                .methods
                .iter()
                .filter_map(|method| method.code.as_ref())
                .map(|code| code.instructions.len())
                .sum();
            let handlers: usize = model
                .methods
                .iter()
                .filter_map(|method| method.code.as_ref())
                .map(|code| code.exception_handlers.len())
                .sum();
            let bounded_slots = model
                .methods
                .iter()
                .filter_map(|method| method.code.as_ref())
                .flat_map(|code| &code.instructions)
                .all(|item| match item {
                    pytecode_engine::model::CodeItem::Var(var) => var.slot < 256,
                    pytecode_engine::model::CodeItem::IInc(iinc) => iinc.slot < 256,
                    _ => true,
                });
            if instructions <= 512 && handlers <= 32 && bounded_slots {
                let _ = model.to_classfile_with_recomputed_frames(
                    pytecode_engine::model::DebugInfoPolicy::Preserve,
                    None,
                );
            }
        }
    }
});
