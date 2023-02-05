use std::path::{Path, PathBuf};

use crate::{Pass, Test};

#[derive(Debug, structopt::StructOpt)]
pub struct Opt {
    /// Path to the root of the crate (or workspace)
    ///
    /// The interestingness test will be run in a copy this folder.
    root_path: PathBuf,

    /// If this option is passed, then only the file passed to it will be reduced
    ///
    /// Pass multiple times to reduce only a specific list of files in the root path.
    /// Paths are relative to the root path, by default all `.rs` files in the root
    /// path will be reduced
    // TODO: also support reducing the toml files, to remove external deps?
    #[structopt(long = "file")]
    only_files: Option<Vec<PathBuf>>,

    /// Number of interestingness tests to run in parallel
    #[structopt(long, short, default_value = "4")]
    jobs: usize,
}

impl Opt {
    pub fn files(
        &self,
        default_list: impl Fn(&Path) -> anyhow::Result<Vec<PathBuf>>,
    ) -> anyhow::Result<Vec<PathBuf>> {
        match &self.only_files {
            Some(r) => Ok(r.clone()),
            None => default_list(&self.root_path),
        }
    }
}

pub fn run(
    opt: Opt,
    filelist: impl Fn(&Path) -> anyhow::Result<Vec<PathBuf>>,
    _test: impl Test,
    _passes: &[&dyn Pass],
) -> anyhow::Result<()> {
    let _files = opt.files(filelist)?;
    let mut workers = Vec::new();
    for _ in 0..opt.jobs {
        workers.push(Worker::new());
    }
    Ok(())
}

struct Worker {}

impl Worker {
    fn new() -> Self {
        todo!()
    }
}
