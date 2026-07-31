mod compiler;
mod ir;
mod machine;
mod native_context;

pub use crate::runtime::live_vm_cell_count as live_cell_count;
pub use compiler::{compile, compile_module};
pub use ir::{Instruction, Primitive, Program, Register};
pub use machine::{Machine, VmError};
