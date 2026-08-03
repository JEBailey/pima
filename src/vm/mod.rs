mod analysis;
mod compiler;
mod ir;
mod machine;
mod native_context;
mod passes;
mod verifier;

pub use crate::runtime::live_vm_cell_count as live_cell_count;
pub(crate) use compiler::compile_module_with_globals_and_source;
pub use compiler::{
    compile, compile_module, compile_module_with_globals, compile_module_with_globals_and_pipeline,
    compile_module_with_pipeline, compile_with_pipeline,
};
pub use ir::{CallArgument, Instruction, Primitive, Program, Register};
pub use machine::{Machine, VmError, VmMetrics};
pub use passes::{CompilerPass, ControlFlowNormalization, PassPipeline, RegisterAllocation};
