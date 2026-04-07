use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawClassStub {
    pub entry_name: String,
    pub bytes: Vec<u8>,
}

pub fn write_raw_classes(classes: &[RawClassStub]) -> Vec<Vec<u8>> {
    classes.iter().map(|class| class.bytes.clone()).collect()
}
