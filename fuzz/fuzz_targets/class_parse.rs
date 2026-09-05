#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 1_048_576 {
        return;
    }
    if let Ok(classfile) = pytecode_engine::parse_class(data) {
        if let Ok(bytes) = pytecode_engine::write_class(&classfile) {
            assert!(pytecode_engine::parse_class(&bytes).is_ok());
        }
    }
});
