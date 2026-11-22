#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DebugInfoState {
    #[default]
    Fresh,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DebugInfoPolicy {
    #[default]
    Preserve,
    Strip,
}

impl DebugInfoPolicy {
    pub fn should_strip(self) -> bool {
        matches!(self, Self::Strip)
    }
}
