use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawClassStub {
    pub entry_name: String,
    pub bytes: Vec<u8>,
}
