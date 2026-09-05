//! JVMS 4.2 names shared by descriptors and structural verification.

pub(crate) fn is_valid_internal_name(name: &str) -> bool {
    !name.contains(['.', ';', '[']) && !name.split('/').any(str::is_empty)
}

pub(crate) fn is_valid_unqualified_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['.', ';', '[', '/'])
}

pub(crate) fn is_valid_method_name(name: &str) -> bool {
    matches!(name, "<init>" | "<clinit>")
        || (is_valid_unqualified_name(name) && !name.contains(['<', '>']))
}
