pub mod documentation;
pub mod formatting;

use std::path::{Path, PathBuf};

pub fn pima_files(path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect(path, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_file() {
        if path
            .extension()
            .is_some_and(|extension| extension == "pima")
        {
            files.push(path.to_owned());
        }
        return Ok(());
    }
    if !path.is_dir() {
        return Err(format!("{} does not exist", path.display()));
    }
    let entries = std::fs::read_dir(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("could not read directory entry: {error}"))?;
        let path = entry.path();
        if path
            .file_name()
            .is_some_and(|name| name == ".git" || name == "target")
        {
            continue;
        }
        collect(&path, files)?;
    }
    Ok(())
}
