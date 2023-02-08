use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use anyhow::Context;
use indicatif::ProgressBar;
use tempfile::TempDir;

use crate::{
    job::{Job, JobResult, JobStatus},
    util::{clone_tempdir, TMPDIR, WORKDIR},
    Test, TestResult,
};

pub(crate) struct Worker {
    rootdir: TempDir,
    sender: crossbeam_channel::Sender<Job>,
    receiver: crossbeam_channel::Receiver<JobResult>,
    killer: crossbeam_channel::Sender<()>,
    job_running: Arc<AtomicBool>,
    progress: ProgressBar,
}

struct WorkerThread<T> {
    rootdir: PathBuf,
    test: T,
    receiver: crossbeam_channel::Receiver<Job>,
    sender: crossbeam_channel::Sender<JobResult>,
    kill_trigger: crossbeam_channel::Receiver<()>,
    job_running: Arc<AtomicBool>,
}

impl Worker {
    pub(crate) fn new(
        root: &Path,
        test: Arc<impl Test>,
        progress: ProgressBar,
    ) -> anyhow::Result<Self> {
        // Tick the progress bar every 100ms
        progress.enable_steady_tick(std::time::Duration::from_millis(100));

        // First, copy the target into a directory
        let rootdir = clone_tempdir(root)?;

        // Then, prepare the communications channels
        let (sender, worker_receiver) = crossbeam_channel::bounded(1);
        let (worker_sender, receiver) = crossbeam_channel::bounded(1);
        let (killer, kill_trigger) = crossbeam_channel::bounded(1);
        let job_running = Arc::new(AtomicBool::new(false));

        // Finally, spawn a thread!
        std::thread::spawn({
            let job_running = job_running.clone();
            let progress = progress.clone();
            let rootdir = rootdir.path().to_path_buf();
            let test = test.clone();
            move || {
                WorkerThread::new(
                    rootdir,
                    ReportingTest(test, progress),
                    worker_receiver,
                    worker_sender,
                    kill_trigger,
                    job_running,
                )
                .run()
            }
        });
        Ok(Worker {
            rootdir,
            receiver,
            sender,
            killer,
            job_running,
            progress,
        })
    }

    pub(crate) fn send_kill(&self) {
        self.killer
            .send(())
            .expect("Failed sending kill request to worker thread")
    }

    pub(crate) fn recover_bar(self) -> ProgressBar {
        // rootdir will be rm'd and worker will naturally die by dropping the sender
        assert!(
            !self.job_running.load(Ordering::Relaxed),
            "Tried to recover the bar of a worker that still has a job running"
        );
        self.progress.disable_steady_tick();
        self.progress
    }

    pub(crate) fn submit(&self, j: Job) -> anyhow::Result<()> {
        assert!(
            !self.job_running.swap(true, Ordering::Relaxed),
            "Tried to submit a job to a worker that still has a job running"
        );
        self.sender
            .try_send(j)
            .expect("Tried to send a job while the previous job was not done yet");
        Ok(())
    }

    pub(crate) fn get_receiver(&self) -> &crossbeam_channel::Receiver<JobResult> {
        &self.receiver
    }

    pub(crate) fn rootdir(&self) -> &Path {
        self.rootdir.path()
    }
}

impl<T: Test> WorkerThread<T> {
    fn new(
        rootdir: PathBuf,
        test: T,
        receiver: crossbeam_channel::Receiver<Job>,
        sender: crossbeam_channel::Sender<JobResult>,
        kill_trigger: crossbeam_channel::Receiver<()>,
        job_running: Arc<AtomicBool>,
    ) -> Self {
        Self {
            rootdir,
            test,
            receiver,
            sender,
            kill_trigger,
            job_running,
        }
    }

    fn rootdir(&self) -> &Path {
        &self.rootdir
    }

    fn workdir(&self) -> PathBuf {
        self.rootdir().join(WORKDIR)
    }

    fn tmpdir(&self) -> PathBuf {
        self.rootdir().join(TMPDIR)
    }

    fn run(self) {
        for job in self.receiver.iter() {
            let res = JobResult {
                res: self.run_job(job.clone()),
                job,
            };
            assert!(
                self.job_running.swap(false, Ordering::Relaxed),
                "Ran a job but job_running was not set to true"
            );
            self.sender
                .try_send(res)
                .expect("Main thread unable to receive job result");
        }
    }

    fn run_job(&self, job: Job) -> anyhow::Result<JobStatus> {
        let workdir = self.workdir();
        let tmpdir = self.tmpdir();
        let filepath = workdir.join(&job.path);
        let tmpfilepath = tmpdir.join(&job.path);

        if let Some(parent) = tmpfilepath.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("recursively creating directory {parent:?} before pass {job:?}")
            })?;
        }
        std::fs::copy(&filepath, &tmpfilepath)
            .with_context(|| format!("saving file {tmpfilepath:?} before pass {job:?}"))?;

        let res = job
            .pass
            .reduce(&workdir, &self.test, &job, &self.kill_trigger)
            .with_context(|| format!("reducing with pass {job:?}"))?;

        if !res.did_reduce() {
            std::fs::copy(&tmpfilepath, &filepath).with_context(|| {
                format!("restoring file {tmpfilepath:?} after failed pass {job:?}")
            })?;
        }
        std::fs::remove_file(&tmpfilepath).with_context(|| {
            format!("removing temporary file {tmpfilepath:?} after pass {job:?}")
        })?;

        Ok(res)
    }
}

struct ReportingTest<T>(Arc<T>, ProgressBar);

impl<T: Test> Test for ReportingTest<T> {
    fn test_interesting(
        &self,
        root: &Path,
        kill_trigger: &crossbeam_channel::Receiver<()>,
        attempt_name: &str,
        attempt_id: u64,
    ) -> anyhow::Result<TestResult> {
        self.1.set_prefix(format!("#{:04x}", attempt_id % 0xFFFF));
        self.1.set_message(String::from(attempt_name));
        let res = self
            .0
            .test_interesting(root, kill_trigger, attempt_name, attempt_id);
        self.1
            .set_message("Figuring out which pass to attempt next");
        res
    }

    fn cleanup_snapshot(&self, root: &Path) -> anyhow::Result<()> {
        self.0.cleanup_snapshot(root)
    }
}
