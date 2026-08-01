use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Arg, ArgAction, Command};

use crate::{
    Config, Interpreter,
    source::SourceMap,
    syntax::{lexer::lex, parser::parse},
};

pub fn run(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let arguments = legacy_run_arguments(arguments.into_iter().collect());
    let matches = match command().try_get_matches_from(arguments) {
        Ok(matches) => matches,
        Err(error) => {
            let informational = matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            );
            let _ = error.print();
            return if informational {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            };
        }
    };
    match matches.subcommand() {
        Some(("run", arguments)) => run_file(required_path(arguments, "path")),
        Some(("check", arguments)) => check_paths(paths(arguments)),
        Some(("fmt", arguments)) => format_paths(
            paths(arguments),
            arguments.get_flag("check"),
            *arguments.get_one::<usize>("indent").unwrap_or(&4),
        ),
        Some(("doc", arguments)) => document_paths(
            paths(arguments),
            arguments.get_one::<PathBuf>("output").cloned(),
            arguments
                .get_one::<String>("format")
                .map(String::as_str)
                .unwrap_or("html"),
        ),
        Some(("lsp", _)) => launch_lsp(),
        _ => ExitCode::from(2),
    }
}

fn command() -> Command {
    Command::new("pima")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Pima language runtime and development tools")
        .subcommand_required(true)
        .subcommand(
            Command::new("run")
                .about("Run a Pima program")
                .arg(path_arg("path").required(true)),
        )
        .subcommand(
            Command::new("check")
                .about("Check Pima source without executing it")
                .arg(paths_arg()),
        )
        .subcommand(
            Command::new("fmt")
                .about("Format Pima source without changing line boundaries")
                .arg(paths_arg())
                .arg(
                    Arg::new("check")
                        .long("check")
                        .action(ArgAction::SetTrue)
                        .help("Report files that would change"),
                )
                .arg(
                    Arg::new("indent")
                        .long("indent")
                        .value_name("SPACES")
                        .value_parser(clap::value_parser!(usize))
                        .default_value("4"),
                ),
        )
        .subcommand(
            Command::new("doc")
                .about("Generate HTML, Markdown, or JSON API documentation")
                .arg(paths_arg())
                .arg(
                    Arg::new("format")
                        .long("format")
                        .value_name("FORMAT")
                        .value_parser(["html", "markdown", "json"])
                        .default_value("html"),
                )
                .arg(
                    Arg::new("output")
                        .short('o')
                        .long("output")
                        .value_name("PATH")
                        .value_parser(clap::value_parser!(PathBuf)),
                ),
        )
        .subcommand(Command::new("lsp").about("Start the Pima language server"))
}

fn path_arg(name: &'static str) -> Arg {
    Arg::new(name).value_parser(clap::value_parser!(PathBuf))
}

fn paths_arg() -> Arg {
    path_arg("paths").num_args(0..).default_value(".")
}

fn legacy_run_arguments(mut arguments: Vec<OsString>) -> Vec<OsString> {
    let commands = ["run", "check", "fmt", "doc", "lsp", "help"];
    if arguments.len() > 1 {
        let first = arguments[1].to_string_lossy();
        if !first.starts_with('-') && !commands.contains(&first.as_ref()) {
            arguments.insert(1, OsString::from("run"));
        }
    }
    arguments
}

fn required_path(arguments: &clap::ArgMatches, name: &str) -> PathBuf {
    arguments
        .get_one::<PathBuf>(name)
        .cloned()
        .expect("required by clap")
}

fn paths(arguments: &clap::ArgMatches) -> Vec<PathBuf> {
    arguments
        .get_many::<PathBuf>("paths")
        .into_iter()
        .flatten()
        .cloned()
        .collect()
}

fn run_file(path: PathBuf) -> ExitCode {
    let mut interpreter = Interpreter::new(Config::default());
    let outcome = interpreter.run_file(path);
    for diagnostic in &outcome.diagnostics {
        eprintln!(
            "{}",
            crate::diagnostic::render(diagnostic, &interpreter.sources)
        );
    }
    success(outcome.is_success())
}

fn check_paths(paths: Vec<PathBuf>) -> ExitCode {
    let mut valid = true;
    let files = match expand(paths) {
        Ok(files) => files,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };
    for path in files {
        let Ok(source) = std::fs::read_to_string(&path) else {
            eprintln!("error: could not read {}", path.display());
            valid = false;
            continue;
        };
        let mut sources = SourceMap::default();
        let id = sources.add(path.display().to_string(), source.as_str());
        let result = lex(id, &source).and_then(|tokens| parse(&tokens));
        if let Err(diagnostics) = result {
            valid = false;
            for diagnostic in diagnostics {
                eprintln!("{}", crate::diagnostic::render(&diagnostic, &sources));
            }
        }
    }
    success(valid)
}

fn format_paths(paths: Vec<PathBuf>, check: bool, indent: usize) -> ExitCode {
    let mut clean = true;
    let files = match expand(paths) {
        Ok(files) => files,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };
    for path in files {
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("error: could not read {}: {error}", path.display());
                clean = false;
                continue;
            }
        };
        match crate::tooling::formatting::format(&source, indent) {
            Ok(formatted) if formatted == source => {}
            Ok(_) if check => {
                println!("would format {}", path.display());
                clean = false;
            }
            Ok(formatted) => {
                if let Err(error) = std::fs::write(&path, formatted) {
                    eprintln!("error: could not write {}: {error}", path.display());
                    clean = false;
                } else {
                    println!("formatted {}", path.display());
                }
            }
            Err(_) => {
                eprintln!("error: {} contains invalid Pima source", path.display());
                clean = false;
            }
        }
    }
    success(clean)
}

fn document_paths(paths: Vec<PathBuf>, output: Option<PathBuf>, format: &str) -> ExitCode {
    let files = match expand(paths) {
        Ok(files) => files,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };
    let documents = match files
        .iter()
        .map(|path| document_file(path))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(documents) => documents,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };
    match format {
        "html" => write_html_site(
            &documents,
            output.unwrap_or_else(|| PathBuf::from("target/pima-doc")),
        ),
        "markdown" => write_text_documents(&documents, output, "md", |document| {
            crate::tooling::documentation::markdown(document)
        }),
        "json" => write_json_documents(&documents, output),
        _ => unreachable!("format is validated by clap"),
    }
}

fn document_file(path: &Path) -> Result<crate::tooling::documentation::Documentation, String> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let mut sources = SourceMap::default();
    let id = sources.add(path.display().to_string(), source.as_str());
    let tokens = lex(id, &source).map_err(|_| format!("{} does not lex", path.display()))?;
    let module = parse(&tokens).map_err(|_| format!("{} does not parse", path.display()))?;
    Ok(crate::tooling::documentation::extract(
        path, &source, &module,
    ))
}

fn write_html_site(
    documents: &[crate::tooling::documentation::Documentation],
    destination: PathBuf,
) -> ExitCode {
    if let Err(error) = std::fs::create_dir_all(&destination) {
        eprintln!("error: could not create {}: {error}", destination.display());
        return ExitCode::FAILURE;
    }
    let modules = documents
        .iter()
        .map(|document| document.module.clone())
        .collect::<Vec<_>>();
    let mut files = vec![
        (
            destination.join("index.html"),
            crate::tooling::documentation::index_html(documents),
        ),
        (
            destination.join("style.css"),
            crate::tooling::documentation::STYLE.to_owned(),
        ),
    ];
    files.extend(documents.iter().map(|document| {
        (
            destination.join(&document.module).with_extension("html"),
            crate::tooling::documentation::html(document, &modules),
        )
    }));
    for (path, content) in files {
        if let Err(error) = std::fs::write(&path, content) {
            eprintln!("error: could not write {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    }
    println!("generated documentation in {}", destination.display());
    ExitCode::SUCCESS
}

fn write_text_documents(
    documents: &[crate::tooling::documentation::Documentation],
    output: Option<PathBuf>,
    extension: &str,
    render: impl Fn(&crate::tooling::documentation::Documentation) -> String,
) -> ExitCode {
    let Some(destination) = output else {
        for document in documents {
            print!("{}", render(document));
        }
        return ExitCode::SUCCESS;
    };
    if documents.len() == 1 && destination.extension().is_some() {
        return write_one(&destination, render(&documents[0]));
    }
    if let Err(error) = std::fs::create_dir_all(&destination) {
        eprintln!("error: could not create {}: {error}", destination.display());
        return ExitCode::FAILURE;
    }
    for document in documents {
        let path = destination.join(&document.module).with_extension(extension);
        if write_one(&path, render(document)) != ExitCode::SUCCESS {
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn write_json_documents(
    documents: &[crate::tooling::documentation::Documentation],
    output: Option<PathBuf>,
) -> ExitCode {
    let content = if documents.len() == 1 {
        crate::tooling::documentation::json(&documents[0])
    } else {
        let values = documents
            .iter()
            .map(|document| {
                serde_json::from_str(&crate::tooling::documentation::json(document))
                    .expect("generated JSON is valid")
            })
            .collect::<Vec<serde_json::Value>>();
        serde_json::to_string_pretty(&values).expect("documentation is serializable") + "\n"
    };
    match output {
        Some(path) => write_one(&path, content),
        None => {
            print!("{content}");
            ExitCode::SUCCESS
        }
    }
}

fn write_one(path: &Path, content: String) -> ExitCode {
    match std::fs::write(path, content) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: could not write {}: {error}", path.display());
            ExitCode::FAILURE
        }
    }
}

fn expand(paths: Vec<PathBuf>) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for path in paths {
        files.append(&mut crate::tooling::pima_files(&path)?);
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn launch_lsp() -> ExitCode {
    let executable = std::env::current_exe()
        .ok()
        .and_then(|path| {
            let extension = path.extension().map(|value| value.to_owned());
            let mut sibling = path.with_file_name("pima-language-server");
            if let Some(extension) = extension {
                sibling.set_extension(extension);
            }
            sibling.exists().then_some(sibling)
        })
        .unwrap_or_else(|| PathBuf::from("pima-language-server"));
    match std::process::Command::new(&executable).status() {
        Ok(status) => success(status.success()),
        Err(error) => {
            eprintln!("error: could not start {}: {error}", executable.display());
            ExitCode::FAILURE
        }
    }
}

fn success(value: bool) -> ExitCode {
    if value {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_file_invocation_becomes_run() {
        let arguments = legacy_run_arguments(vec!["pima".into(), "main.pima".into()]);
        assert_eq!(
            arguments,
            [
                OsString::from("pima"),
                OsString::from("run"),
                OsString::from("main.pima")
            ]
        );
    }

    #[test]
    fn clap_accepts_each_tool_command() {
        for arguments in [
            ["pima", "run", "main.pima"].as_slice(),
            &["pima", "check"],
            &["pima", "fmt", "--check"],
            &["pima", "doc"],
            &["pima", "lsp"],
        ] {
            assert!(command().try_get_matches_from(arguments).is_ok());
        }
    }
}
