use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use indicatif::ProgressBar;
use tempfile::TempDir;

use crate::{
    job::{Job, JobResult, JobStatus},
    util::{clone_tempdir, TMPDIR, WORKDIR},
    Test,
};

pub(crate) struct Worker {
    rootdir: TempDir,
    sender: crossbeam_channel::Sender<Job>,
    receiver: crossbeam_channel::Receiver<JobResult>,
    progress: ProgressBar,
}

struct WorkerThread<T> {
    rootdir: PathBuf,
    test: Arc<T>,
    receiver: crossbeam_channel::Receiver<Job>,
    sender: crossbeam_channel::Sender<JobResult>,
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

        // Finally, spawn a thread!
        std::thread::spawn({
            let rootdir = rootdir.path().to_path_buf();
            let test = test.clone();
            move || WorkerThread::new(rootdir, test, worker_receiver, worker_sender).run()
        });
        Ok(Worker {
            rootdir,
            receiver,
            sender,
            progress,
        })
    }

    pub(crate) fn kill(self) -> ProgressBar {
        // rootdir will be rm'd and worker will naturally die by dropping the sender
        self.progress.disable_steady_tick();
        self.progress
    }

    pub(crate) fn submit(&self, j: Job) -> anyhow::Result<()> {
        let workdir = self.workdir();
        self.progress
            .set_message(j.explain(&workdir).with_context(|| {
                format!("trying to use path {workdir:?} to describe job {j:?}")
            })?);
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

    pub(crate) fn workdir(&self) -> PathBuf {
        self.rootdir().join(WORKDIR)
    }
}

impl<T: Test> WorkerThread<T> {
    fn new(
        rootdir: PathBuf,
        test: Arc<T>,
        receiver: crossbeam_channel::Receiver<Job>,
        sender: crossbeam_channel::Sender<JobResult>,
    ) -> Self {
        Self {
            rootdir,
            test,
            receiver,
            sender,
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
            self.sender
                .try_send(JobResult {
                    res: self.run_job(job.clone()),
                    job,
                })
                .expect("Main thread submitted a job before reading the previous result");
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

        job.pass
            .prepare(&workdir)
            .with_context(|| format!("preparing for pass {job:?}"))?;

        if !job
            .pass
            .reduce(&filepath, job.seed, job.recent_success_rate)
            .with_context(|| format!("reducing with pass {job:?}"))?
        {
            return Ok(JobStatus::PassFailed);
        }

        let res = match self
            .test
            .test_interesting(&workdir)
            .with_context(|| format!("running test (after pass {job:?})"))?
        {
            true => JobStatus::Reduced,
            false => JobStatus::DidNotReduce,
        };

        job.pass
            .cleanup(&workdir, res)
            .with_context(|| format!("cleaning up after pass {job:?}"))?;

        if res != JobStatus::Reduced {
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
