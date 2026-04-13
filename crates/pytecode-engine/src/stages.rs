use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BenchmarkStage {
    JarRead,
    ClassParse,
    ModelLift,
    ModelLower,
    ClassWrite,
}

impl BenchmarkStage {
    pub const ALL: [Self; 5] = [
        Self::JarRead,
        Self::ClassParse,
        Self::ModelLift,
        Self::ModelLower,
        Self::ClassWrite,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::JarRead => "jar-read",
            Self::ClassParse => "class-parse",
            Self::ModelLift => "model-lift",
            Self::ModelLower => "model-lower",
            Self::ClassWrite => "class-write",
        }
    }
}

impl fmt::Display for BenchmarkStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub fn stage_names() -> Vec<String> {
    BenchmarkStage::ALL
        .iter()
        .map(|stage| stage.to_string())
        .collect()
}
