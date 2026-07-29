use std::{ffi::OsString, process::ExitCode};

use crate::{Config, Interpreter};

pub fn run(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let mut arguments = arguments.into_iter();
    let program = arguments.next().unwrap_or_else(|| OsString::from("pima"));
    let Some(path) = arguments.next() else {
        eprintln!(
            "usage: {} <file.pima>",
            std::path::Path::new(&program).display()
        );
        return ExitCode::from(2);
    };

    if arguments.next().is_some() {
        eprintln!("error: expected exactly one Pima source file");
        return ExitCode::from(2);
    }

    let mut interpreter = Interpreter::new(Config::default());
    let outcome = interpreter.run_file(path);
    for diagnostic in &outcome.diagnostics {
        eprintln!(
            "{}",
            crate::diagnostic::render(diagnostic, &interpreter.sources)
        );
    }

    if outcome.is_success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
