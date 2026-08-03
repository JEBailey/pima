use std::sync::Arc;

use crate::{diagnostic::Diagnostic, runtime::Value, syntax::ast::Module};

use super::{Compiler, PassPipeline, Program};

/// Compile a module with the production pipeline and the default module id.
pub fn compile(module: &Module) -> Result<Program, Vec<Diagnostic>> {
    compile_module(module, 0)
}

pub fn compile_with_pipeline(
    module: &Module,
    pipeline: &PassPipeline,
) -> Result<Program, Vec<Diagnostic>> {
    compile_module_with_pipeline(module, 0, pipeline)
}

pub fn compile_module(module: &Module, module_index: usize) -> Result<Program, Vec<Diagnostic>> {
    compile_module_with_pipeline(module, module_index, &PassPipeline::standard())
}

pub fn compile_module_with_pipeline(
    module: &Module,
    module_index: usize,
    pipeline: &PassPipeline,
) -> Result<Program, Vec<Diagnostic>> {
    finish(Compiler::new(module, module_index), pipeline)
}

pub fn compile_module_with_globals(
    module: &Module,
    module_index: usize,
    globals: impl IntoIterator<Item = (Arc<str>, Value)>,
) -> Result<Program, Vec<Diagnostic>> {
    compile_module_with_globals_and_pipeline(
        module,
        module_index,
        globals,
        &PassPipeline::standard(),
    )
}

pub(crate) fn compile_module_with_globals_and_source(
    module: &Module,
    module_index: usize,
    globals: impl IntoIterator<Item = (Arc<str>, Value)>,
    source: Arc<str>,
) -> Result<Program, Vec<Diagnostic>> {
    let mut compiler = Compiler::new(module, module_index);
    compiler.source = Some(source);
    compiler.install_globals(globals);
    finish(compiler, &PassPipeline::standard())
}

pub fn compile_module_with_globals_and_pipeline(
    module: &Module,
    module_index: usize,
    globals: impl IntoIterator<Item = (Arc<str>, Value)>,
    pipeline: &PassPipeline,
) -> Result<Program, Vec<Diagnostic>> {
    let mut compiler = Compiler::new(module, module_index);
    compiler.install_globals(globals);
    finish(compiler, pipeline)
}

fn finish(compiler: Compiler<'_>, pipeline: &PassPipeline) -> Result<Program, Vec<Diagnostic>> {
    let mut program = compiler.compile()?;
    pipeline.run(&mut program)?;
    super::super::verifier::verify(&program)?;
    Ok(program)
}
