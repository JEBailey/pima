use camino::{Utf8Path, Utf8PathBuf};
use pima::engine::{ModuleIdentity, ModuleLoader};

fn workspace_root() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn resolves_virtual_pima_modules_without_touching_the_filesystem() {
    let loader = ModuleLoader::new(workspace_root());
    let identity = loader
        .resolve("/pima/library/standard", None)
        .expect("virtual path should resolve");

    assert_eq!(
        identity,
        ModuleIdentity::Virtual(Utf8PathBuf::from("/pima/library/standard"))
    );
}

#[test]
fn rejects_virtual_module_traversal() {
    let loader = ModuleLoader::new(workspace_root());
    let error = loader
        .resolve("/pima/../private", None)
        .expect_err("parent traversal must be rejected");

    assert!(error.to_string().contains("invalid virtual module path"));
}

#[test]
fn canonicalizes_relative_file_modules() {
    let root = workspace_root();
    let loader = ModuleLoader::new(root.clone());
    let identity = loader
        .resolve("Cargo.toml", None)
        .expect("workspace Cargo.toml should resolve");

    let ModuleIdentity::File(path) = identity else {
        panic!("expected filesystem module identity");
    };
    assert!(path.is_absolute());
    assert!(path.ends_with("Cargo.toml"));
}

#[test]
fn resolves_relative_to_the_importing_file() {
    let root = workspace_root();
    let loader = ModuleLoader::new(root.clone());
    let importer = root.join("examples/test.pima");
    let identity = loader
        .resolve("../Cargo.toml", Some(Utf8Path::new(importer.as_str())))
        .expect("relative import should use importing file directory");

    assert!(identity.path().ends_with("Cargo.toml"));
}
