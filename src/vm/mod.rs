mod analysis;
mod compiler;
mod ir;
mod machine;
mod native_context;
mod passes;

pub use crate::runtime::live_vm_cell_count as live_cell_count;
pub use compiler::{
    compile, compile_module, compile_module_with_globals, compile_module_with_globals_and_pipeline,
    compile_module_with_pipeline, compile_with_pipeline,
};
pub use ir::{Instruction, Primitive, Program, Register};
pub use machine::{Machine, VmError};
pub use passes::{CompilerPass, ControlFlowNormalization, PassPipeline};
