use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use structopt::StructOpt;
use tree_sitter_reduce::{passes::generic::RemoveLines, ShellTest};

#[derive(Debug, structopt::StructOpt)]
struct Opt {
    /// Path to the test that validates whether the input is interesting
    ///
    /// The test must return `0` if it is interesting, and non-0 otherwise. If you
    /// think this is the wrong way around, remember that just because the compiler
    /// crashes doesn't mean that it crashes the way you want; a syntax error is a
    /// crash.
    ///
    /// Note that the test MUST NOT change the contents of its working directory
    /// in any way that could corrupt further runs! The working directory is NOT
    /// cleared between each run; this is on purpose seeing how incremental
    /// compilation makes each test much faster when dealing with large reproducers.
    test: PathBuf,

    #[structopt(flatten)]
    other_opts: tree_sitter_reduce::Opt,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    let test = opt
        .test
        .canonicalize()
        .with_context(|| format!("canonicalizing path {:?}", opt.test))?;
    // Rust testing needs no generic prep/cleanup
    let test = ShellTest::new(test).with_snapshot_cleanup(remove_target_dir);
    tree_sitter_reduce::run(
        opt.other_opts,
        list_files,
        test,
        // TODO: add more interesting passes
        &[Arc::new(RemoveLines)],
    )
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
            let path = file.path();
            let path = path.strip_prefix(root).with_context(|| {
                format!(
                    "Path {path:?} was found in folder {root:?} but seems to not be a sub-element"
                )
            })?;
            tracing::debug!("Found file to reduce: {path:?}");
            res.push(path.to_path_buf());
        }
    }
    Ok(res)
}

fn remove_target_dir(root: &Path) -> anyhow::Result<()> {
    let target_dir = root.join("target");
    if let Ok(_) = std::fs::metadata(target_dir) {
        std::fs::remove_dir_all(&root.join("target"))?;
    }
    Ok(())
}
