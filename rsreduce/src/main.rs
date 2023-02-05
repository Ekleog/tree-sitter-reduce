use std::path::{Path, PathBuf};

use anyhow::Context;
use structopt::StructOpt;
use tree_sitter_reduce::ShellTest;

#[derive(Debug, structopt::StructOpt)]
struct Opt {
    /// Path to the test that validates whether the input is interesting
    ///
    /// The test must return `0` if it is interesting, and non-0 otherwise. If you
    /// think this is the wrong way around, remember that just because the compiler
    /// crashes doesn't mean that it crashes the way you want; a syntax error is a
    /// crash.
    test: PathBuf,

    #[structopt(flatten)]
    other_opts: tree_sitter_reduce::Opt,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    // Rust testing needs no generic prep/cleanup
    let test = ShellTest::new(opt.test);
    tree_sitter_reduce::run(opt.other_opts, list_files, test, &[]) // TODO: add passes
}

fn list_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    // TODO: Also support reducing the toml files, to remove external deps? This will
    // need additional infra in tree-sitter-reduce, to support a different selection
    // of passes per file type.
    let mut res = Vec::new();
    for file in walkdir::WalkDir::new(root) {
        let file =
            file.with_context(|| format!("walking directory {root:?} looking for rust files"))?;
        if file.file_type().is_file() && file.file_name().to_string_lossy().ends_with(".rs") {
            println!("Found file to reduce: {:?}", file.path()); // TODO: make optional based on verbosity
            res.push(file.path().to_path_buf());
        }
    }
    Ok(res)
}
