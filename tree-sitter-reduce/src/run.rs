use std::{
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
};

use anyhow::Context;
use tempfile::TempDir;

use crate::{Pass, Test};

#[derive(Debug, structopt::StructOpt)]
pub struct Opt {
    /// Path to the root of the crate (or workspace)
    ///
    /// The interestingness test will be run in a copy this folder. Note that copies
    /// can happen during the whole runtime of this program, so the folder should not
    /// be changed during that time.
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
    pub fn canonicalize_root_path(&mut self) -> anyhow::Result<()> {
        Ok(self.root_path = self
            .root_path
            .canonicalize()
            .with_context(|| "canonicalizing root path {root:?}")?)
    }

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
    mut opt: Opt,
    filelist: impl Fn(&Path) -> anyhow::Result<Vec<PathBuf>>,
    _test: impl Test,
    _passes: &[Arc<dyn Pass>],
) -> anyhow::Result<()> {
    opt.canonicalize_root_path()?;
    let _files = opt.files(filelist)?;
    let mut workers = Vec::new();
    for _ in 0..opt.jobs {
        workers.push(Worker::new(&opt.root_path).context("spinning up worker")?);
    }
    todo!()
}

struct Job {
    pass: Arc<dyn Pass>,
    seed: u64,
}

struct Worker {
    dir: TempDir,
    sender: mpsc::Sender<Job>,
    receiver: mpsc::Receiver<anyhow::Result<bool>>,
}

impl Worker {
    fn new(root: &Path) -> anyhow::Result<Self> {
        // First, copy the target into a directory
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

        // Then, prepare the communications channels
        let (sender, worker_receiver) = mpsc::channel();
        let (worker_sender, receiver) = mpsc::channel();

        // Finally, spawn a thread!
        std::thread::spawn({
            let dir = dir.path().to_path_buf();
            move || WorkerThread::new(worker_receiver, worker_sender).run()
        });
        Ok(Worker {
            dir,
            receiver,
            sender,
        })
    }
}

struct WorkerThread {
    receiver: mpsc::Receiver<Job>,
    sender: mpsc::Sender<anyhow::Result<bool>>,
}

impl WorkerThread {
    fn new(receiver: mpsc::Receiver<Job>, sender: mpsc::Sender<anyhow::Result<bool>>) -> Self {
        Self { receiver, sender }
    }

    fn run(&mut self) {
        todo!()
    }
}
