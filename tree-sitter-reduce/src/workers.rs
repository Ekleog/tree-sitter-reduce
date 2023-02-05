use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use tempfile::TempDir;

use crate::{
    job::{Job, JobResult, JobStatus},
    util::{clone_tempdir, WORKDIR},
    Test,
};

pub(crate) struct Worker {
    dir: TempDir,
    sender: crossbeam_channel::Sender<Job>,
    receiver: crossbeam_channel::Receiver<JobResult>,
}

struct WorkerThread<T> {
    dir: PathBuf,
    test: Arc<T>,
    receiver: crossbeam_channel::Receiver<Job>,
    sender: crossbeam_channel::Sender<JobResult>,
}

impl Worker {
    pub(crate) fn new(root: &Path, test: Arc<impl Test>) -> anyhow::Result<Self> {
        // First, copy the target into a directory
        let dir = clone_tempdir(root)?;

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

    pub(crate) fn submit(&self, j: Job) {
        self.sender
            .try_send(j)
            .expect("Tried to send a job while the previous job was not done yet")
    }

    pub(crate) fn get_receiver(&self) -> &crossbeam_channel::Receiver<JobResult> {
        &self.receiver
    }

    pub(crate) fn dir(&self) -> &Path {
        self.dir.path()
    }
}

impl<T: Test> WorkerThread<T> {
    fn new(
        dir: PathBuf,
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
        let workdir = self.dir.join(WORKDIR);
        let filepath = workdir.join(&job.path);

        match job.pass.prepare(&workdir) {
            Ok(()) => (),
            Err(e) => {
                return JobResult {
                    res: Err(e).with_context(|| format!("preparing for pass {job:?}")),
                    job,
                }
            }
        };

        match job
            .pass
            .reduce(&filepath, job.seed, job.recent_success_rate)
        {
            Ok(true) => (),
            Ok(false) => {
                return JobResult {
                    job,
                    res: Ok(JobStatus::PassFailed),
                }
            }
            Err(e) => {
                return JobResult {
                    res: Err(e).with_context(|| format!("reducing with pass {job:?}")),
                    job,
                }
            }
        };

        let res = match self.test.test_interesting(&workdir) {
            Ok(true) => JobStatus::Reduced,
            Ok(false) => JobStatus::DidNotReduce,
            Err(e) => {
                return JobResult {
                    res: Err(e).with_context(|| format!("running test (after pass {job:?})")),
                    job,
                }
            }
        };

        match job.pass.cleanup(&workdir, res) {
            Ok(()) => (),
            Err(e) => {
                return JobResult {
                    res: Err(e).with_context(|| format!("cleaning up after pass {job:?}")),
                    job,
                }
            }
        };

        JobResult { job, res: Ok(res) }
    }
}
