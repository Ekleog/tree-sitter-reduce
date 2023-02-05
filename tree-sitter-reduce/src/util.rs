use std::path::Path;

use anyhow::Context;
use tempfile::TempDir;

pub(crate) const WORKDIR: &str = "workdir";

pub(crate) fn copy_dir_contents(from: &Path, to: &Path) -> anyhow::Result<()> {
    fs_extra::dir::copy(
        from,
        to,
        &fs_extra::dir::CopyOptions::default().content_only(true),
    )
    .with_context(|| format!("copying directory from {from:?} to {to:?}"))
    .map(|_| ())
}

pub(crate) fn clone_tempdir(root: &Path) -> anyhow::Result<TempDir> {
    let dir = tempfile::Builder::new()
        .prefix("tree-sitter-reduce-")
        .tempdir()
        .context("creating temporary directory")?;
    copy_dir_contents(root, dir.path())?;
    Ok(dir)
}

pub(crate) fn copy_to_tempdir(root: &Path) -> anyhow::Result<TempDir> {
    let dir = tempfile::Builder::new()
        .prefix("tree-sitter-reduce-")
        .tempdir()
        .context("creating temporary directory")?;
    let actual_path = dir.path().join(WORKDIR);
    std::fs::create_dir(&actual_path)
        .context("creating directory nested under the temporary directory")?;
    copy_dir_contents(root, &actual_path)?;
    Ok(dir)
}
