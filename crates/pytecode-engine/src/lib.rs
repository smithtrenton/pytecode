//! Raw JVM classfile engine and phase scaffolding for the reduced Rust workspace.

pub mod analysis;
mod bytes;
pub mod constants;
pub mod descriptors;
pub mod error;
pub mod fixtures;
pub mod indexes;
pub mod model;
pub mod modified_utf8;
pub mod raw;
pub mod reader;
pub mod signatures;
pub mod stages;
pub mod transform;
pub mod writer;

pub use error::{EngineError, EngineErrorKind, Result};
pub use indexes::{
    BootstrapMethodIndex, ClassIndex, CpIndex, FieldRefIndex, MethodRefIndex, ModuleIndex,
    NameAndTypeIndex, PackageIndex, Utf8Index,
};
pub use reader::{ClassReader, parse_class, parse_class_bytes, parse_instructions};
pub use writer::{ClassWriter, write_class};
