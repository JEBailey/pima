use crate::{source::Span, syntax::ast::BlockId as AstBlockId};

use super::{EnvironmentId, SymbolId};

#[derive(Clone, Debug)]
pub struct UserFunction {
    pub name: SymbolId,
    pub parameters: Vec<SymbolId>,
    pub body: AstBlockId,
    pub body_module: usize, // Index into parsed_modules
    pub environment: EnvironmentId,
    pub declaration_span: Span,
}
