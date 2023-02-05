use std::{path::PathBuf, sync::Arc};

use anyhow::Context;
use rand::{rngs::StdRng, seq::SliceRandom, Rng};
use tempfile::TempDir;

use crate::{
    job::{Job, JobResult, JobStatus},
    util::copy_to_tempdir,
    workers::Worker,
    Pass, Test,
};

pub(crate) struct Runner<'a, T> {
    root: TempDir,
    test: Arc<T>,
    files: Vec<PathBuf>,
    passes: &'a [Arc<dyn Pass>],
    workers: Vec<Worker>,
    rng: StdRng,
}

impl<'a, T: Test> Runner<'a, T> {
    pub(crate) fn new(
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

    pub(crate) fn run(mut self) -> anyhow::Result<()> {
        loop {
            let next_job = self.make_job();
            self.wait_for_worker()?.submit(next_job);
            todo!() // Do regular snapshotting of current status
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
