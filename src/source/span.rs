#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceId(pub(crate) u32);

impl SourceId {
    pub(crate) const fn index(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub source: SourceId,
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(source: SourceId, start: usize, end: usize) -> Self {
        Self { source, start, end }
    }
}
