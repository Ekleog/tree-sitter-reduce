use std::path::Path;

use anyhow::Context;
use tempfile::TempDir;

pub(crate) const WORKDIR: &str = "workdir";

pub(crate) fn clone_tempdir(root: &Path) -> anyhow::Result<TempDir> {
    let dir = tempfile::Builder::new()
        .prefix("tree-sitter-reduce-")
        .tempdir()
        .context("creating temporary directory")?;
    fs_extra::dir::copy(
        root,
        &dir,
        &fs_extra::dir::CopyOptions::default().content_only(true),
    )
    .with_context(|| format!("copying source from {root:?} to {:?}", dir.path()))?;
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
    fs_extra::dir::copy(
        root,
        &actual_path,
        &fs_extra::dir::CopyOptions::default().content_only(true),
    )
    .with_context(|| format!("copying source from {root:?} to {:?}", dir.path()))?;
    Ok(dir)
}
