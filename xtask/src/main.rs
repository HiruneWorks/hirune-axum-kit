mod checks;
mod deployment;
mod infrastructure;
mod production;
mod template;

use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("workspace root missing")?;
    std::env::set_current_dir(root)?;
    match std::env::args().nth(1).as_deref() {
        Some("init") => template::initialize(),
        Some("check") => checks::check(),
        Some("openapi") => checks::openapi(false),
        Some("test-db") => infrastructure::run_tests(&["identity", "jobs", "schema_consistency"]),
        Some("test-api") => infrastructure::run_tests(&["api", "oidc", "api_access"]),
        Some("test-process") => infrastructure::run_tests(&["process"]),
        Some("test-files") => infrastructure::run_tests(&["files"]),
        Some("test-content") => infrastructure::run_tests(&["content"]),
        Some("test-deployment") => deployment::verify(),
        Some("test-production") => production::verify(),
        Some("load-test") => infrastructure::run_load_test(),
        Some("template-check") => template::verify(root),
        _ => anyhow::bail!(
            "usage: cargo xtask <init|check|openapi|test-db|test-api|test-process|test-files|test-content|test-deployment|test-production|load-test|template-check>"
        ),
    }
}

fn run(command: &mut Command) -> Result<()> {
    let status = command
        .status()
        .context("could not start development command")?;
    ensure!(status.success(), "development command failed: {status}");
    Ok(())
}

fn rust_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            files.extend(rust_files(&path)?);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(files)
}
