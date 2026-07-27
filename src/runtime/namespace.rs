use super::{EnvironmentId, SymbolId};

#[derive(Clone, Debug)]
pub struct Namespace {
    pub environment: EnvironmentId,
    pub types: Vec<SymbolId>,
}
