use std::{io::ErrorKind, path::PathBuf, sync::Arc};

use crate::runtime::Value;

use super::{Arity, NativeContext, NativeDefinition, NativeRegistry, NativeResult};

pub const READ_TEXT: &str = "read_text";
pub const WRITE_TEXT: &str = "write_text";

pub fn register(registry: &mut NativeRegistry) {
    registry.register(NativeDefinition {
        name: READ_TEXT,
        arity: Arity::Exact(1),
        call: read_text,
    });
    registry.register(NativeDefinition {
        name: WRITE_TEXT,
        arity: Arity::Exact(2),
        call: write_text,
    });
}

fn read_text(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let [Value::String(path)] = arguments else {
        return Err(context.typed_error(
            &["error", "type_error"],
            "read_text requires a string path".to_owned(),
        ));
    };
    let resolved = resolve_path(context, path);
    std::fs::read_to_string(&resolved)
        .map(|text| Value::String(Arc::from(text)))
        .map_err(|error| io_error(context, &resolved, error))
}

fn write_text(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let [Value::String(path), Value::String(text)] = arguments else {
        return Err(context.typed_error(
            &["error", "type_error"],
            "write_text requires a string path and string text".to_owned(),
        ));
    };
    let resolved = resolve_path(context, path);
    std::fs::write(&resolved, text.as_bytes())
        .map(|()| Value::Unit)
        .map_err(|error| io_error(context, &resolved, error))
}

fn resolve_path(context: &dyn NativeContext, path: &str) -> PathBuf {
    let path = std::path::Path::new(path);
    if path.is_absolute() {
        path.to_owned()
    } else {
        context.working_directory().join(path)
    }
}

fn io_error(
    context: &mut dyn NativeContext,
    path: &std::path::Path,
    error: std::io::Error,
) -> Value {
    let specific = match error.kind() {
        ErrorKind::NotFound => "file_not_found",
        ErrorKind::PermissionDenied => "permission_denied",
        ErrorKind::InvalidData => "invalid_encoding",
        ErrorKind::AlreadyExists => "already_exists",
        _ => "io_error",
    };
    let types = if specific == "io_error" {
        vec!["error", "io_error"]
    } else {
        vec!["error", "io_error", specific]
    };
    context.typed_error(
        &types,
        format!("I/O operation failed for `{}`: {error}", path.display()),
    )
}
