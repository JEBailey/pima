use std::{
    fs::OpenOptions,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::runtime::{PersistentList, Value};

use super::{Arity, NativeContext, NativeDefinition, NativeRegistry, NativeResult};

pub const EXPORTS: &[(&str, &str)] = &[
    ("read_text", "io.read_text"),
    ("read_lines", "io.read_lines"),
    ("read_bytes", "io.read_bytes"),
    ("write_text", "io.write_text"),
    ("append_text", "io.append_text"),
    ("write_bytes", "io.write_bytes"),
    ("append_bytes", "io.append_bytes"),
    ("exists?", "io.exists?"),
    ("file?", "io.file?"),
    ("directory?", "io.directory?"),
    ("create_directory", "io.create_directory"),
    ("list_directory", "io.list_directory"),
    ("copy_file", "io.copy_file"),
    ("move", "io.move"),
    ("remove_file", "io.remove_file"),
    ("remove_directory", "io.remove_directory"),
    ("join", "io.join"),
    ("parent", "io.parent"),
    ("file_name", "io.file_name"),
    ("extension", "io.extension"),
    ("canonicalize", "io.canonicalize"),
    ("current_directory", "io.current_directory"),
];

pub fn register(registry: &mut NativeRegistry) {
    register_native(registry, "io.read_text", Arity::Exact(1), read_text);
    register_native(registry, "io.read_lines", Arity::Exact(1), read_lines);
    register_native(registry, "io.read_bytes", Arity::Exact(1), read_bytes);
    register_native(registry, "io.write_text", Arity::Exact(2), write_text);
    register_native(registry, "io.append_text", Arity::Exact(2), append_text);
    register_native(registry, "io.write_bytes", Arity::Exact(2), write_bytes);
    register_native(registry, "io.append_bytes", Arity::Exact(2), append_bytes);
    register_native(registry, "io.exists?", Arity::Exact(1), exists);
    register_native(registry, "io.file?", Arity::Exact(1), is_file);
    register_native(registry, "io.directory?", Arity::Exact(1), is_directory);
    register_native(
        registry,
        "io.create_directory",
        Arity::Exact(1),
        create_directory,
    );
    register_native(
        registry,
        "io.list_directory",
        Arity::Exact(1),
        list_directory,
    );
    register_native(registry, "io.copy_file", Arity::Exact(2), copy_file);
    register_native(registry, "io.move", Arity::Exact(2), move_path);
    register_native(registry, "io.remove_file", Arity::Exact(1), remove_file);
    register_native(
        registry,
        "io.remove_directory",
        Arity::Exact(1),
        remove_directory,
    );
    register_native(registry, "io.join", Arity::AtLeast(1), join);
    register_native(registry, "io.parent", Arity::Exact(1), parent);
    register_native(registry, "io.file_name", Arity::Exact(1), file_name);
    register_native(registry, "io.extension", Arity::Exact(1), extension);
    register_native(registry, "io.canonicalize", Arity::Exact(1), canonicalize);
    register_native(
        registry,
        "io.current_directory",
        Arity::Exact(0),
        current_directory,
    );
}

fn register_native(
    registry: &mut NativeRegistry,
    name: &'static str,
    arity: Arity,
    call: super::NativeCall,
) {
    registry.register(NativeDefinition { name, arity, call });
}

fn read_text(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let path = string_path(context, arguments, "read_text")?;
    std::fs::read_to_string(&path)
        .map(|text| Value::String(Arc::from(text)))
        .map_err(|error| io_error(context, "read", &path, error))
}

fn read_lines(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let path = string_path(context, arguments, "read_lines")?;
    std::fs::read_to_string(&path)
        .map(|text| {
            Value::List(
                text.lines()
                    .map(|line| Value::String(Arc::from(line)))
                    .collect::<PersistentList>(),
            )
        })
        .map_err(|error| io_error(context, "read", &path, error))
}

fn read_bytes(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let path = string_path(context, arguments, "read_bytes")?;
    std::fs::read(&path)
        .map(|bytes| {
            Value::List(
                bytes
                    .into_iter()
                    .map(|byte| Value::Integer(i64::from(byte)))
                    .collect(),
            )
        })
        .map_err(|error| io_error(context, "read", &path, error))
}

fn write_text(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let (path, text) = path_and_text(context, arguments, "write_text")?;
    std::fs::write(&path, text.as_bytes())
        .map(|()| Value::Unit)
        .map_err(|error| io_error(context, "write", &path, error))
}

fn append_text(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let (path, text) = path_and_text(context, arguments, "append_text")?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map(|()| Value::Unit)
        .map_err(|error| io_error(context, "append", &path, error))
}

fn write_bytes(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let (path, bytes) = path_and_bytes(context, arguments, "write_bytes")?;
    std::fs::write(&path, bytes)
        .map(|()| Value::Unit)
        .map_err(|error| io_error(context, "write", &path, error))
}

fn append_bytes(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let (path, bytes) = path_and_bytes(context, arguments, "append_bytes")?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| file.write_all(&bytes))
        .map(|()| Value::Unit)
        .map_err(|error| io_error(context, "append", &path, error))
}

fn exists(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    metadata_predicate(context, arguments, "exists?", |_| true)
}

fn is_file(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    metadata_predicate(context, arguments, "file?", |metadata| metadata.is_file())
}

fn is_directory(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    metadata_predicate(context, arguments, "directory?", |metadata| {
        metadata.is_dir()
    })
}

fn metadata_predicate(
    context: &mut dyn NativeContext,
    arguments: &[Value],
    operation: &str,
    predicate: impl FnOnce(&std::fs::Metadata) -> bool,
) -> NativeResult {
    let path = string_path(context, arguments, operation)?;
    match std::fs::metadata(&path) {
        Ok(metadata) => Ok(Value::Boolean(predicate(&metadata))),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Value::Boolean(false)),
        Err(error) => Err(io_error(context, "inspect", &path, error)),
    }
}

fn create_directory(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let path = string_path(context, arguments, "create_directory")?;
    std::fs::create_dir_all(&path)
        .map(|()| Value::Unit)
        .map_err(|error| io_error(context, "create directory", &path, error))
}

fn list_directory(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let path = string_path(context, arguments, "list_directory")?;
    let entries =
        std::fs::read_dir(&path).map_err(|error| io_error(context, "list", &path, error))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| io_error(context, "list", &path, error))?;
        let name = entry.file_name().into_string().map_err(|_| {
            context.typed_error(
                &["error", "io_error", "invalid_encoding"],
                format!(
                    "directory `{}` contains a name that is not valid UTF-8",
                    path.display()
                ),
            )
        })?;
        names.push(name);
    }
    names.sort();
    Ok(Value::List(
        names
            .into_iter()
            .map(|name| Value::String(Arc::from(name)))
            .collect(),
    ))
}

fn copy_file(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let (source, destination) = two_paths(context, arguments, "copy_file")?;
    std::fs::copy(&source, &destination)
        .map(|_| Value::Unit)
        .map_err(|error| io_error_pair(context, "copy", &source, &destination, error))
}

fn move_path(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let (source, destination) = two_paths(context, arguments, "move")?;
    std::fs::rename(&source, &destination)
        .map(|()| Value::Unit)
        .map_err(|error| io_error_pair(context, "move", &source, &destination, error))
}

fn remove_file(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let path = string_path(context, arguments, "remove_file")?;
    std::fs::remove_file(&path)
        .map(|()| Value::Unit)
        .map_err(|error| io_error(context, "remove file", &path, error))
}

fn remove_directory(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let path = string_path(context, arguments, "remove_directory")?;
    std::fs::remove_dir(&path)
        .map(|()| Value::Unit)
        .map_err(|error| io_error(context, "remove directory", &path, error))
}

fn join(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let mut path = PathBuf::new();
    for argument in arguments {
        let Value::String(component) = argument else {
            return Err(type_error(context, "join requires string path components"));
        };
        path.push(component.as_ref());
    }
    path_string(context, path, "joined path is not valid UTF-8")
}

fn parent(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let Value::String(path) = &arguments[0] else {
        return Err(type_error(context, "parent requires a string path"));
    };
    match Path::new(path.as_ref()).parent() {
        Some(parent) => path_string(context, parent.to_owned(), "parent path is not valid UTF-8"),
        None => Ok(Value::Unit),
    }
}

fn file_name(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    path_component(context, arguments, "file_name", Path::file_name)
}

fn extension(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    path_component(context, arguments, "extension", Path::extension)
}

fn path_component(
    context: &mut dyn NativeContext,
    arguments: &[Value],
    operation: &str,
    component: impl FnOnce(&Path) -> Option<&std::ffi::OsStr>,
) -> NativeResult {
    let Value::String(path) = &arguments[0] else {
        return Err(type_error(
            context,
            &format!("{operation} requires a string path"),
        ));
    };
    match component(Path::new(path.as_ref())) {
        Some(value) => value
            .to_str()
            .map(|value| Value::String(Arc::from(value)))
            .ok_or_else(|| {
                context.typed_error(
                    &["error", "io_error", "invalid_encoding"],
                    format!("{operation} result is not valid UTF-8"),
                )
            }),
        None => Ok(Value::Unit),
    }
}

fn canonicalize(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let path = string_path(context, arguments, "canonicalize")?;
    std::fs::canonicalize(&path)
        .map_err(|error| io_error(context, "canonicalize", &path, error))
        .and_then(|path| path_string(context, path, "canonical path is not valid UTF-8"))
}

fn current_directory(context: &mut dyn NativeContext, _: &[Value]) -> NativeResult {
    let directory = context.working_directory().to_owned();
    path_string(context, directory, "working directory is not valid UTF-8")
}

fn string_path(
    context: &mut dyn NativeContext,
    arguments: &[Value],
    operation: &str,
) -> Result<PathBuf, Value> {
    let Value::String(path) = &arguments[0] else {
        return Err(type_error(
            context,
            &format!("{operation} requires a string path"),
        ));
    };
    Ok(resolve_path(context, path))
}

fn path_and_text<'a>(
    context: &mut dyn NativeContext,
    arguments: &'a [Value],
    operation: &str,
) -> Result<(PathBuf, &'a str), Value> {
    let [Value::String(path), Value::String(text)] = arguments else {
        return Err(type_error(
            context,
            &format!("{operation} requires a string path and string text"),
        ));
    };
    Ok((resolve_path(context, path), text))
}

fn path_and_bytes(
    context: &mut dyn NativeContext,
    arguments: &[Value],
    operation: &str,
) -> Result<(PathBuf, Vec<u8>), Value> {
    let [Value::String(path), Value::List(values)] = arguments else {
        return Err(type_error(
            context,
            &format!("{operation} requires a string path and list of bytes"),
        ));
    };
    let mut bytes = Vec::with_capacity(values.len());
    for value in values.iter() {
        let Value::Integer(byte @ 0..=255) = value else {
            return Err(context.typed_error(
                &["error", "value_error"],
                format!("{operation} requires byte integers from 0 through 255"),
            ));
        };
        bytes.push(*byte as u8);
    }
    Ok((resolve_path(context, path), bytes))
}

fn two_paths(
    context: &mut dyn NativeContext,
    arguments: &[Value],
    operation: &str,
) -> Result<(PathBuf, PathBuf), Value> {
    let [Value::String(source), Value::String(destination)] = arguments else {
        return Err(type_error(
            context,
            &format!("{operation} requires two string paths"),
        ));
    };
    Ok((
        resolve_path(context, source),
        resolve_path(context, destination),
    ))
}

fn resolve_path(context: &dyn NativeContext, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_owned()
    } else {
        context.working_directory().join(path)
    }
}

fn path_string(context: &mut dyn NativeContext, path: PathBuf, message: &str) -> NativeResult {
    path.into_os_string()
        .into_string()
        .map(|path| Value::String(Arc::from(path)))
        .map_err(|_| {
            context.typed_error(
                &["error", "io_error", "invalid_encoding"],
                message.to_owned(),
            )
        })
}

fn type_error(context: &mut dyn NativeContext, message: &str) -> Value {
    context.typed_error(&["error", "type_error"], message.to_owned())
}

fn io_error(
    context: &mut dyn NativeContext,
    operation: &str,
    path: &Path,
    error: std::io::Error,
) -> Value {
    let types = error_types(error.kind());
    context.typed_error(
        &types,
        format!(
            "could not {operation} `{}`: {error}",
            path.to_string_lossy()
        ),
    )
}

fn io_error_pair(
    context: &mut dyn NativeContext,
    operation: &str,
    source: &Path,
    destination: &Path,
    error: std::io::Error,
) -> Value {
    let types = error_types(error.kind());
    context.typed_error(
        &types,
        format!(
            "could not {operation} `{}` to `{}`: {error}",
            source.to_string_lossy(),
            destination.to_string_lossy()
        ),
    )
}

fn error_types(kind: ErrorKind) -> Vec<&'static str> {
    let specific = match kind {
        ErrorKind::NotFound => Some("file_not_found"),
        ErrorKind::PermissionDenied => Some("permission_denied"),
        ErrorKind::InvalidData => Some("invalid_encoding"),
        ErrorKind::AlreadyExists => Some("already_exists"),
        ErrorKind::InvalidInput => Some("invalid_input"),
        ErrorKind::UnexpectedEof => Some("unexpected_end_of_file"),
        ErrorKind::WriteZero => Some("write_failed"),
        ErrorKind::BrokenPipe => Some("broken_pipe"),
        ErrorKind::TimedOut => Some("timed_out"),
        ErrorKind::Interrupted => Some("interrupted"),
        ErrorKind::Unsupported => Some("unsupported_operation"),
        ErrorKind::OutOfMemory => Some("out_of_memory"),
        _ => None,
    };
    match specific {
        Some(specific) => vec!["error", "io_error", specific],
        None => vec!["error", "io_error"],
    }
}
