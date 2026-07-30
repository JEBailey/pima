use crate::{
    source::Span,
    syntax::ast::{NodeId, Pattern},
};

use dumpster::{TraceWith, Visitor, unsync::Gc};

use super::{EnvironmentRef, SymbolId};

pub type FunctionRef = Gc<UserFunction>;

#[derive(Clone, Debug)]
pub struct UserFunction {
    pub name: SymbolId,
    pub parameter: Pattern,
    pub body: NodeId,
    pub body_module: usize, // Index into parsed_modules
    pub environment: EnvironmentRef,
    pub declaration_span: Span,
}

unsafe impl<V: Visitor> TraceWith<V> for UserFunction {
    fn accept(&self, visitor: &mut V) -> Result<(), ()> {
        self.environment.accept(visitor)
    }
}
