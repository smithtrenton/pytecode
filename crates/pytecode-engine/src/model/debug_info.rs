#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DebugInfoState {
    #[default]
    Fresh,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugInfoPolicy {
    Preserve,
    Strip,
}

impl DebugInfoPolicy {
    pub fn should_strip(self) -> bool {
        matches!(self, Self::Strip)
    }
}
