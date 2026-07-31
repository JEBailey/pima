use std::{collections::HashMap, sync::Arc};

use crate::{
    diagnostic::Diagnostic,
    runtime::Value,
    syntax::ast::{NamespaceImportSelection, NodeKind},
};

use super::{Interpreter, ModuleIdentity, PreparedProgram, RunOutcome};

pub(super) fn run(interpreter: &mut Interpreter, prepared: PreparedProgram) -> RunOutcome {
    match execute_module(interpreter, prepared.module_index, true) {
        Ok(value) => RunOutcome {
            value: Some(value),
            diagnostics: Vec::new(),
        },
        Err(diagnostics) => RunOutcome {
            value: None,
            diagnostics,
        },
    }
}

fn execute_module(
    interpreter: &mut Interpreter,
    module_index: usize,
    session_root: bool,
) -> Result<Value, Vec<Diagnostic>> {
    let mut globals = interpreter.vm.standard_globals();
    if session_root {
        globals.extend(interpreter.vm_session_globals.clone());
    }
    let statements = interpreter.parsed_modules[module_index].statements.clone();
    let importer = interpreter
        .sources
        .get(interpreter.parsed_modules[module_index].source)
        .map(|source| source.name.to_string());
    for statement in statements {
        match interpreter.parsed_modules[module_index]
            .node(statement)
            .kind
            .clone()
        {
            NodeKind::Import { path, alias } => {
                let namespace = load_module(interpreter, &path, importer.as_deref())?;
                if let Some(alias) = alias {
                    insert_unique(&mut globals, alias, namespace)?;
                } else {
                    for (name, value) in interpreter.vm.namespace_globals(&namespace) {
                        globals.insert(name, value);
                    }
                }
            }
            NodeKind::NamespaceImport {
                path,
                selection,
                alias,
            } => {
                let mut value = globals.get(&path[0].text).cloned().ok_or_else(|| {
                    vec![Diagnostic::at_error(
                        format!("unbound import namespace `{}`", path[0].text),
                        path[0].span,
                    )]
                })?;
                for name in &path[1..] {
                    value = interpreter
                        .vm
                        .namespace_globals(&value)
                        .get(&name.text)
                        .cloned()
                        .ok_or_else(|| {
                            vec![Diagnostic::at_error(
                                format!("namespace has no member `{name}`"),
                                name.span,
                            )]
                        })?;
                }
                match selection {
                    NamespaceImportSelection::Wildcard(_) => {
                        for (name, value) in interpreter.vm.namespace_globals(&value) {
                            insert_unique(&mut globals, name, value)?;
                        }
                    }
                    NamespaceImportSelection::Member(member) => {
                        let member_value = interpreter
                            .vm
                            .namespace_globals(&value)
                            .get(&member.text)
                            .cloned()
                            .ok_or_else(|| {
                                vec![Diagnostic::at_error(
                                    format!("namespace has no member `{member}`"),
                                    member.span,
                                )]
                            })?;
                        let local = alias.map(|name| name.text).unwrap_or(member.text);
                        insert_unique(&mut globals, local, member_value)?;
                    }
                }
            }
            _ => {}
        }
    }
    let compiled = crate::vm::compile_module_with_globals(
        &interpreter.parsed_modules[module_index],
        module_index,
        globals,
    )?;
    let result = interpreter
        .vm
        .execute(&compiled)
        .map_err(|error| vec![interpreter.vm.diagnostic(error)])?;
    interpreter.vm_programs.insert(module_index, compiled);
    if session_root {
        let program = interpreter.vm_programs.get(&module_index).unwrap();
        interpreter.vm_session_globals = interpreter.vm.exported_globals(program);
    }
    Ok(result)
}

fn load_module(
    interpreter: &mut Interpreter,
    requested: &str,
    importer: Option<&str>,
) -> Result<Value, Vec<Diagnostic>> {
    let importer = importer
        .filter(|name| !name.starts_with('<') && !name.starts_with("/pima/"))
        .map(camino::Utf8Path::new);
    let identity = interpreter
        .module_loader
        .resolve(requested, importer)
        .map_err(|error| vec![Diagnostic::error(error.to_string())])?;
    if let Some(module_index) = interpreter.vm_module_indices.get(&identity).copied()
        && let Some(program) = interpreter.vm_programs.get(&module_index)
        && let Some(namespace) = interpreter.vm.exported_namespace(program)
    {
        return Ok(namespace);
    }
    if interpreter.vm_loading.contains(&identity) {
        let cycle = interpreter
            .vm_loading
            .iter()
            .chain(std::iter::once(&identity))
            .map(|identity| identity.path().to_string())
            .collect::<Vec<_>>()
            .join(" -> ");
        return Err(vec![Diagnostic::error(format!(
            "import cycle detected: {cycle}"
        ))]);
    }
    if let ModuleIdentity::Virtual(path) = &identity {
        let exports = match path.as_str() {
            "/pima/io" => Some(crate::native::io::EXPORTS),
            "/pima/tcp" => Some(crate::native::tcp::EXPORTS),
            _ => None,
        };
        if let Some(exports) = exports {
            return interpreter
                .vm
                .native_module(exports)
                .map_err(|diagnostic| vec![diagnostic]);
        }
    }
    let source = match &identity {
        ModuleIdentity::Virtual(path) if path.as_str() == "/pima/library/standard" => {
            include_str!("../../stdlib/standard.pima").to_owned()
        }
        ModuleIdentity::Virtual(path) => {
            return Err(vec![Diagnostic::error(format!(
                "virtual module `{path}` not found"
            ))]);
        }
        ModuleIdentity::File(path) => std::fs::read_to_string(path).map_err(|error| {
            vec![Diagnostic::error(format!(
                "could not read module `{path}`: {error}"
            ))]
        })?,
    };
    let source_id = interpreter
        .sources
        .add(identity.path().as_str(), source.clone());
    let tokens = crate::syntax::lexer::lex(source_id, &source)?;
    let module = crate::syntax::parser::parse(&tokens)?;
    let module_index = interpreter.parsed_modules.len();
    interpreter.parsed_modules.push(module);
    interpreter
        .vm_module_indices
        .insert(identity.clone(), module_index);
    interpreter.vm_loading.push(identity.clone());
    let result = execute_module(interpreter, module_index, false);
    interpreter
        .vm_loading
        .retain(|loading| loading != &identity);
    result?;
    let program = interpreter
        .vm_programs
        .get(&module_index)
        .expect("executed module must retain its program");
    interpreter
        .vm
        .exported_namespace(program)
        .ok_or_else(|| vec![Diagnostic::error("executed module did not publish exports")])
}

fn insert_unique(
    globals: &mut HashMap<Arc<str>, Value>,
    name: Arc<str>,
    value: Value,
) -> Result<(), Vec<Diagnostic>> {
    if globals.insert(name.clone(), value).is_some() {
        return Err(vec![Diagnostic::error(format!(
            "import collides with existing binding `{name}`"
        ))]);
    }
    Ok(())
}
