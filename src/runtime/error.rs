use crate::{diagnostic::StackFrame, source::Span};

#[derive(Clone, Debug)]
pub struct ErrorMetadata {
    pub origin: Span,
    pub stack: Vec<StackFrame>,
}
