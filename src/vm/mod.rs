mod compiler;
mod ir;
mod machine;

pub use compiler::compile;
pub use ir::{Instruction, Primitive, Program, Register};
pub use machine::Machine;
