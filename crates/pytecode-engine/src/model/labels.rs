use crate::constants::TargetType;
use crate::indexes::Utf8Index;
use crate::raw::{ElementValuePairInfo, Instruction, TargetInfo, TypePathInfo};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

use super::operands::{
    FieldInsn, IIncInsn, InterfaceMethodInsn, InvokeDynamicInsn, LdcInsn, MethodInsn,
    MultiANewArrayInsn, TypeInsn, VarInsn,
};

static NEXT_LABEL_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct Label {
    id: u64,
    pub name: Option<String>,
}

impl Label {
    pub fn new() -> Self {
        Self {
            id: NEXT_LABEL_ID.fetch_add(1, Ordering::Relaxed),
            name: None,
        }
    }

    pub fn named(name: impl Into<String>) -> Self {
        Self {
            id: NEXT_LABEL_ID.fetch_add(1, Ordering::Relaxed),
            name: Some(name.into()),
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }
}

impl Default for Label {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for Label {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Label {}

impl Hash for Label {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInsn {
    pub opcode: u8,
    pub target: Label,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupSwitchInsn {
    pub default_target: Label,
    pub pairs: Vec<(i32, Label)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSwitchInsn {
    pub default_target: Label,
    pub low: i32,
    pub high: i32,
    pub targets: Vec<Label>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionHandler {
    pub start: Label,
    pub end: Label,
    pub handler: Label,
    pub catch_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineNumberEntry {
    pub label: Label,
    pub line_number: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalVariableEntry {
    pub start: Label,
    pub end: Label,
    pub name: String,
    pub descriptor: String,
    pub index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalVariableTypeEntry {
    pub start: Label,
    pub end: Label,
    pub name: String,
    pub signature: String,
    pub index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalvarTargetEntry {
    Range {
        start: Label,
        end: Label,
        index: u16,
    },
    NoLiveRange {
        index: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeTypeAnnotationTarget {
    Localvar {
        table: Vec<LocalvarTargetEntry>,
    },
    Catch {
        exception_table_index: usize,
    },
    Offset {
        target: Label,
    },
    TypeArgument {
        target: Label,
        type_argument_index: u8,
    },
    Other(TargetInfo),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeTypeAnnotation {
    pub target_type: TargetType,
    pub target: CodeTypeAnnotationTarget,
    pub target_path: TypePathInfo,
    pub type_index: Utf8Index,
    pub element_value_pairs: Vec<ElementValuePairInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeItem {
    Label(Label),
    Raw(Instruction),
    Field(FieldInsn),
    Method(MethodInsn),
    InterfaceMethod(InterfaceMethodInsn),
    Type(TypeInsn),
    Var(VarInsn),
    IInc(IIncInsn),
    Ldc(LdcInsn),
    InvokeDynamic(InvokeDynamicInsn),
    MultiANewArray(MultiANewArrayInsn),
    Branch(BranchInsn),
    LookupSwitch(LookupSwitchInsn),
    TableSwitch(TableSwitchInsn),
}
