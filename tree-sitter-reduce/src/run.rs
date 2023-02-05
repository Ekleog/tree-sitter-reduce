use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};
use tempfile::TempDir;

use crate::{pass::JobStatus, Pass, Test};

#[derive(Debug, structopt::StructOpt)]
pub struct Opt {
    /// Path to the root of the crate (or workspace)
    ///
    /// The interestingness test will be run in a copy this folder. Note that copies
    /// will happen only during the startup of this program. So the folder can be
    /// changed after the program confirms it's running.
    root_path: PathBuf,

    /// If this option is passed, then only the file passed to it will be reduced
    ///
    /// Pass multiple times to reduce only a specific list of files in the root path.
    /// Paths are relative to the root path. By default all the files in the root path
    /// that this program knows how to reduce, will be reduced.
    #[structopt(long = "file")]
    only_files: Option<Vec<PathBuf>>,

    /// Number of interestingness tests to run in parallel
    #[structopt(long, short, default_value = "4")]
    jobs: usize,

    /// Seed for the random number generation
    #[structopt(long)]
    random_seed: Option<u64>,
}

impl Opt {
    pub fn canonicalized_root_path(&mut self) -> anyhow::Result<PathBuf> {
        self.root_path
            .canonicalize()
            .with_context(|| "canonicalizing root path {root:?}")
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
    passes: &[Arc<dyn Pass>],
) -> anyhow::Result<()> {
    // Handle the arguments
    let root = opt.canonicalized_root_path()?;
    let files = opt.files(filelist)?;
    let seed = opt.random_seed.unwrap_or_else(rand::random);

    // Sanity-checks
    anyhow::ensure!(
        !files.is_empty(),
        "Cannot find any file to reduce in {root:?}",
    );
    anyhow::ensure!(
        !passes.is_empty(),
        "Ill-configured runner: no passes are configured",
    );

    // Actually run
    println!("Initial seed is < {seed} >. It can be used for reproduction if running with a single worker thread");
    let rng = StdRng::seed_from_u64(seed);
    Runner::new(root, test, files, passes, rng, opt.jobs)?.run()
}

struct Runner<'a, T> {
    root: TempDir,
    test: Arc<T>,
    files: Vec<PathBuf>,
    passes: &'a [Arc<dyn Pass>],
    workers: Vec<Worker>,
    rng: StdRng,
}

impl<'a, T: Test> Runner<'a, T> {
    fn new(
        root: PathBuf,
        test: T,
        files: Vec<PathBuf>,
        passes: &'a [Arc<dyn Pass>],
        rng: StdRng,
        jobs: usize,
    ) -> anyhow::Result<Self> {
        let mut this = Runner {
            root: copy_to_tempdir(&root)?,
            test: Arc::new(test),
            files,
            passes,
            workers: Vec::with_capacity(jobs),
            rng,
        };

        println!("Finished copying target directory, running…");

        // Spawn the workers
        for _ in 0..jobs {
            this.spawn_worker()?;
        }

        Ok(this)
    }

    fn spawn_worker(&mut self) -> anyhow::Result<()> {
        let worker =
            Worker::new(self.root.path(), self.test.clone()).context("spinning up a worker")?;
        worker.submit(self.make_job());
        self.workers.push(worker);
        Ok(())
    }

    fn make_job(&mut self) -> Job {
        let path = self
            .root
            .path()
            .join(self.files.choose(&mut self.rng).unwrap());
        let pass = self.passes.choose(&mut self.rng).unwrap().clone();
        let seed = self.rng.gen();
        let recent_success_rate = 0; // TODO
        Job {
            path,
            pass,
            seed,
            recent_success_rate,
        }
    }

    fn run(mut self) -> anyhow::Result<()> {
        loop {
            let w = self.wait_for_worker()?;
            todo!()
        }
    }

    fn wait_for_worker(&mut self) -> anyhow::Result<&mut Worker> {
        loop {
            // Receive the first message from a worker
            let mut sel = crossbeam_channel::Select::new();
            for w in &self.workers {
                sel.recv(w.get_receiver());
            }
            let oper = sel.select();
            let w = oper.index();
            match oper
                .recv(self.workers[w].get_receiver())
                .expect("Workers should never disconnect first")
            {
                JobResult { job, res: Ok(res) } => {
                    self.handle_result(job, res);
                    return Ok(&mut self.workers[w]);
                }
                JobResult { job, res: Err(e) } => {
                    eprintln!(
                        "Worker died while processing a job! Starting a new worker…\nJob: {job:#?}\nError:\n---\n{e:#}\n---"
                    );
                    self.workers.swap_remove(w);
                    self.spawn_worker()?;
                }
            }
        }
    }

    fn handle_result(&mut self, job: Job, res: JobStatus) {
        todo!()
    }
}

#[derive(Debug)]
struct Job {
    path: PathBuf,
    pass: Arc<dyn Pass>,
    seed: u64,
    recent_success_rate: u8,
}

struct JobResult {
    job: Job,
    res: anyhow::Result<JobStatus>,
}

struct Worker {
    sender: crossbeam_channel::Sender<Job>,
    receiver: crossbeam_channel::Receiver<JobResult>,
}

struct WorkerThread<T> {
    dir: TempDir,
    test: Arc<T>,
    receiver: crossbeam_channel::Receiver<Job>,
    sender: crossbeam_channel::Sender<JobResult>,
}

impl Worker {
    fn new(root: &Path, test: Arc<impl Test>) -> anyhow::Result<Self> {
        // First, copy the target into a directory
        let dir = copy_to_tempdir(root)?;

        // Then, prepare the communications channels
        let (sender, worker_receiver) = crossbeam_channel::bounded(1);
        let (worker_sender, receiver) = crossbeam_channel::bounded(1);

        // Finally, spawn a thread!
        std::thread::spawn({
            let test = test.clone();
            move || WorkerThread::new(dir, test, worker_receiver, worker_sender).run()
        });
        Ok(Worker { receiver, sender })
    }

    fn submit(&self, j: Job) {
        self.sender
            .try_send(j)
            .expect("Tried to send a job while the previous job was not done yet")
    }

    fn get_receiver(&self) -> &crossbeam_channel::Receiver<JobResult> {
        &self.receiver
    }
}

impl<T: Test> WorkerThread<T> {
    fn new(
        dir: TempDir,
        test: Arc<T>,
        receiver: crossbeam_channel::Receiver<Job>,
        sender: crossbeam_channel::Sender<JobResult>,
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

    fn run_job(&self, job: Job) -> JobResult {
        match job.pass.prepare(self.dir.path()) {
            Ok(()) => (),
            Err(e) => return JobResult { job, res: Err(e) },
        };

        match job
            .pass
            .reduce(&job.path, job.seed, job.recent_success_rate)
        {
            Ok(true) => (),
            Ok(false) => {
                return JobResult {
                    job,
                    res: Ok(JobStatus::PassFailed),
                }
            }
            Err(e) => return JobResult { job, res: Err(e) },
        };

        let res = match self.test.test_interesting(self.dir.path()) {
            Ok(true) => JobStatus::Reduced,
            Ok(false) => JobStatus::DidNotReduce,
            Err(e) => return JobResult { job, res: Err(e) },
        };

        match job.pass.cleanup(self.dir.path(), res) {
            Ok(()) => (),
            Err(e) => return JobResult { job, res: Err(e) },
        };

        JobResult { job, res: Ok(res) }
    }
}

fn copy_to_tempdir(root: &Path) -> anyhow::Result<TempDir> {
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
