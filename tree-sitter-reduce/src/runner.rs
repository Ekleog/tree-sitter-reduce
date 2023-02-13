use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Duration};

use anyhow::Context;
use fxhash::FxHashMap;
use indicatif::ProgressBar;
use kine::{
    icu::{cal::Iso, Cal},
    tz::Utc,
    Calendar,
};
use rand::{rngs::StdRng, seq::SliceRandom, Rng};
use tempfile::TempDir;

use crate::{
    job::{Job, JobResult, JobStatus},
    util::{copy_dir_contents, copy_to_tempdir, make_progress_bar, BAR_TICK_INTERVAL, WORKDIR},
    workers::Worker,
    Pass, Test, TestResult,
};

struct FileInfo {
    recent_success_rate: u8,
}

impl FileInfo {
    fn new() -> FileInfo {
        FileInfo {
            recent_success_rate: u8::MAX / 2,
        }
    }

    // (9 * self + MAX) / 10
    fn record_success(&mut self) {
        self.recent_success_rate =
            u8::try_from((self.recent_success_rate as u32 * 9 + u8::MAX as u32) / 10).unwrap();
    }

    // (9 * self + 0) / 10
    fn record_fail(&mut self) {
        self.recent_success_rate = u8::try_from(self.recent_success_rate as u32 * 9 / 10).unwrap();
    }
}

pub(crate) struct Runner<'a, T> {
    root: TempDir,
    test: Arc<T>,
    // FxHashMap because we want deterministic iteration order, for
    // random-based-on-printed-seed-only iteration order
    files: FxHashMap<PathBuf, FileInfo>,
    passes: &'a [Arc<dyn Pass>],
    snap_dir: PathBuf,
    snap_interval: Duration,
    max_snaps: usize,
    workers: Vec<Worker>,
    kill_trigger: crossbeam_channel::Receiver<()>,
    rng: StdRng,
}

struct WorkerIdx(usize);

impl<'a, T: Test> Runner<'a, T> {
    pub(crate) fn new(
        root: PathBuf,
        test: T,
        files: HashSet<PathBuf>,
        passes: &'a [Arc<dyn Pass>],
        snap_dir: PathBuf,
        snap_interval: Duration,
        max_snaps: usize,
        rng: StdRng,
        jobs: usize,
        progress: indicatif::MultiProgress,
        do_not_validate_input: bool,
    ) -> anyhow::Result<Self> {
        // Setup a ctrl-c handler that will kill us whenever
        let (killer, kill_trigger) = crossbeam_channel::bounded(3);
        ctrlc::set_handler(move || {
            killer
                .send(())
                .expect("User asked to kill 3 times the reducer before it had the time to notice")
        })
        .context("setting the interruption handler")?;

        // Copy the target directory to a tempdir
        let mut this = Runner {
            root: copy_to_tempdir(&root)?,
            test: Arc::new(test),
            files: files.into_iter().map(|f| (f, FileInfo::new())).collect(),
            passes,
            snap_dir,
            snap_interval,
            max_snaps,
            workers: Vec::with_capacity(jobs),
            kill_trigger,
            rng,
        };

        // Check that the provided test actually returns true on the initial input
        // TODO: Also clean up if killed here. This is before we spawn workers, so a bit
        // of refactoring will probably be needed.
        tracing::info!("Finished copying target directory {root:?}");
        if do_not_validate_input {
            tracing::warn!("Not validating the target directory. Note that validation does not usually make a reduction take significantly longer, but does avoid long useless waits due to malformed input.");
        } else {
            let bar = make_progress_bar();
            bar.enable_steady_tick(BAR_TICK_INTERVAL);
            bar.set_message("Checking that the provided target directory is interesting");
            let res = this.test.test_interesting(
                &this.root.path().join(WORKDIR),
                &this.kill_trigger,
                "Validate input interestingness",
                0,
            )?;
            match res {
                TestResult::Interesting => (),
                TestResult::NotInteresting => {
                    anyhow::bail!("Test did not find the provided target directory interesting")
                }
                TestResult::Interrupted => anyhow::bail!("Killed by user"),
            }
            bar.finish_and_clear();
            tracing::info!("The target directory was interesting, starting reducing…");
        }

        for _ in 0..jobs {
            this.spawn_worker(progress.add(make_progress_bar()))?;
        }

        Ok(this)
    }

    fn spawn_worker(&mut self, progress: ProgressBar) -> anyhow::Result<()> {
        let worker = Worker::new(self.root.path(), self.test.clone(), progress)
            .context("spinning up a worker")?;
        self.workers.push(worker);
        self.send_job_to(WorkerIdx(self.workers.len() - 1))?;
        Ok(())
    }

    fn send_job_to(&mut self, worker: WorkerIdx) -> anyhow::Result<()> {
        let (relpath, info) = self
            .files
            .iter()
            .skip(self.rng.gen_range(0..self.files.len()))
            .next()
            .unwrap();
        let pass = self.passes.choose(&mut self.rng).unwrap().clone();
        let seed = self.rng.gen();
        let recent_success_rate = info.recent_success_rate;
        let job = Job::new(relpath.clone(), pass, seed, recent_success_rate)?;
        self.workers[worker.0].submit(job)?;
        Ok(())
    }

    pub(crate) fn run(mut self) -> anyhow::Result<()> {
        let mut next_snap = std::time::Instant::now() + self.snap_interval;
        let mut did_reduce = false;
        loop {
            // Note: `snap_interval` can be equal to 0, so if we did not have this match
            // we would be busy-looping.
            let worker = match did_reduce {
                true => self.wait_for_worker(Some(next_snap))?,
                false => self.wait_for_worker(None)?,
            };
            if let Some((worker, pass_status)) = worker {
                did_reduce |= pass_status.did_reduce();
                self.send_job_to(worker)?;
            }
            if did_reduce && std::time::Instant::now() >= next_snap {
                // We have passed next snap time!
                self.snapshot()?;
                next_snap = std::time::Instant::now() + self.snap_interval;
                did_reduce = false;
            }
        }
    }

    /// Returns Some((worker id, job result)) if a worker finished, None otherwise
    fn wait_for_worker(
        &mut self,
        deadline: Option<std::time::Instant>,
    ) -> anyhow::Result<Option<(WorkerIdx, JobStatus)>> {
        loop {
            // Find the first worker with a message
            let mut sel = crossbeam_channel::Select::new();
            for w in &self.workers {
                sel.recv(w.get_receiver());
            }
            sel.recv(&self.kill_trigger);
            let oper = match deadline {
                None => sel.select(),
                Some(deadline) => match sel.select_deadline(deadline) {
                    Ok(oper) => oper,
                    Err(crossbeam_channel::SelectTimeoutError) => return Ok(None),
                },
            };
            let w = oper.index();

            // If the signal came from the kill trigger, handle it
            if w == self.workers.len() {
                oper.recv(&self.kill_trigger)
                    .expect("Kill trigger should never disconnect at all");
                for w in self.workers.drain(..) {
                    w.send_kill();
                }
                anyhow::bail!("Killed by the user");
            }

            // If not, read its message and act upon it
            let w = WorkerIdx(w);
            match oper
                .recv(self.workers[w.0].get_receiver())
                .expect("Workers should never disconnect first")
            {
                JobResult { job, res: Ok(res) } => {
                    match &res {
                        JobStatus::Reduced(desc) => {
                            tracing::info!("Job successfully reduced the input: {desc}")
                        }
                        JobStatus::DidNotReduce => {
                            tracing::trace!("Job did not reduce the input: {:?} {:?}", job.pass, job.path)
                        }
                        JobStatus::PassFailed(desc) => {
                            tracing::debug!("Job failed to handle the input: {desc}")
                        }
                        JobStatus::Interrupted => panic!("Got interrupted job result even though that should happen only after the runner itself is stopped"),
                    }
                    let w = self.handle_result(w, job, &res)?;
                    return Ok(Some((w, res)));
                }
                JobResult { job, res: Err(e) } => {
                    tracing::error!("Worker died while processing a job! Starting a new worker…\nJob: {job:?}\nError:\n---\n{e:?}\n---");
                    let worker = self.workers.swap_remove(w.0);
                    self.spawn_worker(worker.recover_bar())?;
                }
            }
        }
    }

    /// Note that while handling the result, the worker order can change
    ///
    /// So this returns the new WorkerIdx for `worker`.
    fn handle_result(
        &mut self,
        worker: WorkerIdx,
        job: Job,
        res: &JobStatus,
    ) -> anyhow::Result<WorkerIdx> {
        match res {
            JobStatus::Reduced(_) => {
                self.files.get_mut(&job.path).unwrap().record_success();
                return self.handle_reduction(worker, job);
            }
            JobStatus::DidNotReduce => {
                self.files.get_mut(&job.path).unwrap().record_fail();
            }
            // TODO: do something to avoid trying this pass again on the same file just after?
            JobStatus::PassFailed(_) => (),
            JobStatus::Interrupted => panic!("Got interrupted job result even though that should happen only after the runner itself is stopped"),
        }
        Ok(worker)
    }

    fn handle_reduction(&mut self, worker: WorkerIdx, _job: Job) -> anyhow::Result<WorkerIdx> {
        // TODO: try to intelligently merge successful reductions? that's what _job would be for
        tracing::trace!("Handling reduction");
        // Retrieve the worker's successful reduction to "current best" state
        let my_dir = self.root.path();
        let my_workdir = my_dir.join(WORKDIR);
        let workerdir = self.workers[worker.0].rootdir();
        std::fs::remove_dir_all(&my_workdir)
            .with_context(|| format!("removing \"current status\" path {my_workdir:?}"))?;
        fs_extra::dir::copy(
            workerdir,
            &my_dir,
            &fs_extra::dir::CopyOptions::default().content_only(true),
        )
        .with_context(|| {
            format!("copying successful reduction from {workerdir:?} to {my_dir:?}")
        })?;
        // Restart other workers so they actually take advantage of it
        tracing::trace!("Sending a kill message to all other workers");
        let mut workers_to_restart = self.workers.drain((worker.0 + 1)..).collect::<Vec<_>>();
        workers_to_restart.extend(self.workers.drain(..worker.0));
        for w in &workers_to_restart {
            w.send_kill();
        }
        tracing::trace!("Waiting for all other workers to reply to the kill message");
        while !workers_to_restart.is_empty() {
            let mut sel = crossbeam_channel::Select::new();
            for w in workers_to_restart.iter() {
                sel.recv(w.get_receiver());
            }
            sel.recv(&self.kill_trigger);
            let oper = sel.select();
            let w = oper.index();
            if w == workers_to_restart.len() {
                let _ = oper.recv(&self.kill_trigger);
                anyhow::bail!("Killed by user");
            }
            let _ = oper.recv(workers_to_restart[w].get_receiver());
            let worker = workers_to_restart.swap_remove(w);
            self.spawn_worker(worker.recover_bar())
                .context("restarting workers after one of them found a successful reduction")?;
        }
        tracing::trace!("All workers replied to the kill message and were restarted");
        Ok(WorkerIdx(0)) // We removed all workers then respawned new ones
    }

    fn snapshot(&self) -> anyhow::Result<()> {
        let now = Cal::new(Iso, Utc).now();
        let now = now.icu();
        let snap_dir = self.snap_dir.join(format!(
            "{:04}-{:02}-{:02}-{:02}-{:02}-{:02}-{:03}",
            now.date.year().number,
            now.date.month().ordinal,
            now.date.day_of_month().0,
            now.time.hour.number(),
            now.time.minute.number(),
            now.time.second.number(),
            now.time.nanosecond.number() / 1_000_000,
        ));
        let workdir = self.root.path().join(WORKDIR);
        std::fs::create_dir(&snap_dir)
            .with_context(|| format!("creating snapshot directory {snap_dir:?}"))?;
        copy_dir_contents(&workdir, &snap_dir)?;
        self.test
            .cleanup_snapshot(&snap_dir)
            .with_context(|| format!("cleaning up snapshot {snap_dir:?}"))?;
        tracing::info!("Wrote a reduced snapshot in {snap_dir:?}");
        self.remove_old_snapshots()?;
        tracing::trace!("Removed old snapshots from {:?}", self.snap_dir);
        Ok(())
    }

    fn remove_old_snapshots(&self) -> anyhow::Result<()> {
        let mut snapshots = std::fs::read_dir(&self.snap_dir)
            .with_context(|| format!("listing snapshots in {:?}", self.snap_dir))?
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| format!("listing snapshots in {:?}", self.snap_dir))?;
        if snapshots.len() <= self.max_snaps {
            return Ok(());
        }
        snapshots.sort_by_key(|s| s.file_name());
        snapshots.truncate(snapshots.len() - self.max_snaps);
        tracing::trace!("Too many snapshots, removing {snapshots:?}");
        for s in snapshots {
            let path = s.path();
            std::fs::remove_dir_all(&path)
                .with_context(|| format!("removing expired snapshot {path:?}"))?;
        }
        Ok(())
    }
}
