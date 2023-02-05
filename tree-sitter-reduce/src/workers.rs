use std::{path::Path, sync::Arc};

use tempfile::TempDir;

use crate::{
    job::{Job, JobResult, JobStatus},
    util::copy_to_tempdir,
    Test,
};

pub(crate) struct Worker {
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
    pub(crate) fn new(root: &Path, test: Arc<impl Test>) -> anyhow::Result<Self> {
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

    pub(crate) fn submit(&self, j: Job) {
        self.sender
            .try_send(j)
            .expect("Tried to send a job while the previous job was not done yet")
    }

    pub(crate) fn get_receiver(&self) -> &crossbeam_channel::Receiver<JobResult> {
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
