#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 1_048_576 {
        return;
    }
    if let Ok(model) = pytecode_engine::model::ClassModel::from_bytes(data) {
        if let Ok(classfile) = model.to_classfile() {
            let _ = pytecode_engine::write_class(&classfile);
        }
    }
});
