//! Parser and tree-walk interpreter for the Pima language.
//!
//! [`Interpreter`] is the embedding entry point. Call
//! [`Interpreter::run_source`] for in-memory source or
//! [`Interpreter::run_file`] for a file-backed program. Language failures are
//! returned as [`Diagnostic`] values rather than Rust panics.

pub mod cli;
pub mod diagnostic;
pub mod engine;
pub mod native;
pub mod runtime;
pub mod source;
pub mod syntax;

pub use diagnostic::Diagnostic;
pub use engine::{Config, Interpreter, RunOutcome};
pub use runtime::Value;
