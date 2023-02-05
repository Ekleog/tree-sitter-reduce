use std::{
    path::{Path, PathBuf},
    sync::Arc,
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
    test: impl Test,
    _passes: &[Arc<dyn Pass>],
) -> anyhow::Result<()> {
    // Handle the arguments
    opt.canonicalize_root_path()?;
    let _files = opt.files(filelist)?;
    let test = Arc::new(test);

    // Spawn the workers
    let mut workers = Vec::new();
    for _ in 0..opt.jobs {
        workers.push(Worker::new(&opt.root_path, test.clone()).context("spinning up worker")?);
    }
    todo!()
}

struct Job {
    path: PathBuf,
    pass: Arc<dyn Pass>,
    seed: u64,
    recent_success_rate: u8,
}

struct Worker {
    #[allow(dead_code)] // `dir` needs to be kept alive for the temporary directory to stay there
    dir: TempDir,
    sender: crossbeam_channel::Sender<Job>,
    receiver: crossbeam_channel::Receiver<anyhow::Result<bool>>,
}

struct WorkerThread<T> {
    dir: PathBuf,
    test: Arc<T>,
    receiver: crossbeam_channel::Receiver<Job>,
    sender: crossbeam_channel::Sender<anyhow::Result<bool>>,
}

impl Worker {
    fn new(root: &Path, test: Arc<impl Test>) -> anyhow::Result<Self> {
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
        let (sender, worker_receiver) = crossbeam_channel::bounded(1);
        let (worker_sender, receiver) = crossbeam_channel::bounded(1);

        // Finally, spawn a thread!
        std::thread::spawn({
            let dir = dir.path().to_path_buf();
            let test = test.clone();
            move || WorkerThread::new(dir, test, worker_receiver, worker_sender).run()
        });
        Ok(Worker {
            dir,
            receiver,
            sender,
        })
    }

    fn submit(&self, j: Job) {
        self.sender
            .try_send(j)
            .expect("Tried to send a job while the previous job was not done yet")
    }

    fn get_receiver(&self) -> &crossbeam_channel::Receiver<anyhow::Result<bool>> {
        &self.receiver
    }
}

impl<T: Test> WorkerThread<T> {
    fn new(
        dir: PathBuf,
        test: Arc<T>,
        receiver: crossbeam_channel::Receiver<Job>,
        sender: crossbeam_channel::Sender<anyhow::Result<bool>>,
    ) -> Self {
        Self {
            dir,
            test,
            receiver,
            sender,
        }
    }

    fn run(self) {
        for job in self.receiver.iter() {
            self.sender
                .try_send(self.run_job(job))
                .expect("Main thread submitted a job before reading the previous result");
        }
    }

    fn run_job(&self, job: Job) -> anyhow::Result<bool> {
        job.pass.prepare(&self.dir)?;
        job.pass
            .reduce(&job.path, job.seed, job.recent_success_rate)?;
        let res = self.test.test_interesting(&self.dir)?;
        job.pass.cleanup(&self.dir, res)?;
        Ok(res)
    }
}
