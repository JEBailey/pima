use std::path::{Path, PathBuf};

use pima::{Config, Interpreter, Value};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_owned()
}

#[test]
fn every_supported_example_executes_successfully() {
    let root = repository_root();
    let examples = root.join("examples");
    let mut paths = std::fs::read_dir(&examples)
        .expect("examples directory should be readable")
        .map(|entry| entry.expect("example entry should be readable").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "pima")
        })
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name != "java_support.pima")
        })
        .collect::<Vec<_>>();
    paths.sort();

    for path in paths {
        let mut interpreter = Interpreter::new(Config {
            working_directory: Some(examples.clone()),
        });
        let outcome = interpreter.run_file(&path);
        assert!(
            outcome.is_success(),
            "{} failed: {:?}",
            path.display(),
            outcome.diagnostics
        );
    }
}

#[test]
fn representative_programs_have_exact_results() {
    let cases = [
        ("[+ 20 22]\n", Value::Integer(42)),
        (
            "function add (:x) {\n    function inner (:y) { + x y }\n}\nval add_two [add 2]\n[add_two 5]\n",
            Value::Integer(7),
        ),
        (
            "val Counter {\n    var value 0\n    pub function next () { let value [+ value 1] }\n}\nval counter [new Counter]\n[counter.next]\n[counter.next]\n",
            Value::Integer(2),
        ),
        (
            "import \"/pima/library/standard\"\nval source (1 2)\nval changed [List.push source 0]\n[= source (1 2)]\n",
            Value::Boolean(true),
        ),
    ];

    for (source, expected) in cases {
        let mut interpreter = Interpreter::default();
        let outcome = interpreter.run_source("<conformance>", source);
        assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
        assert_eq!(outcome.value, Some(expected));
    }
}

#[test]
fn representative_failures_preserve_portable_error_types() {
    let source = "import \"/pima/library/standard\"\nval result [attempt {\n    List.head ()\n}]\n[Types.is? result :index_error]\n";
    let mut interpreter = Interpreter::default();
    let outcome = interpreter.run_source("<conformance>", source);

    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(Value::Boolean(true)));
}
