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
    }
});
