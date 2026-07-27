mod render;

use crate::source::Span;

pub use render::render;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub primary_span: Option<Span>,
    pub stack: Vec<StackFrame>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            primary_span: None,
            stack: Vec::new(),
        }
    }

    pub fn at_error(message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            primary_span: Some(span),
            stack: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct StackFrame {
    pub name: String,
    pub span: Span,
}
