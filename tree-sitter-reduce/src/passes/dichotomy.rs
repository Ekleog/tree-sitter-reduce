use std::{fmt::Debug, hash::Hash, path::Path};

use anyhow::Context;

use crate::{Job, JobStatus, Pass, Test};

/// Helper trait to implement `Pass` for passes that make use of dichotomy
///
/// See the documentation for `Pass` for all the details; this documentation only
/// contains the differences to there.
pub trait DichotomyPass {
    type Attempt;

    /// List the attempts this pass should try
    ///
    /// Returns the list of attempts starting from the smallest (most likely to
    /// succeed but least reducing) and ending with the biggest (least likely to
    /// succeed but most interesting).
    ///
    /// The idea behind this order is that the pass should build the `Vec` while
    /// using the random seed to incrementally add elements to the `T` inside,
    /// and then cloning it into the result `Vec`.
    ///
    /// One example such result is, for a pass that would remove lines `x..y`,
    /// and assuming the file has for instance 16 lines:
    /// ```rust
    /// vec![3..4, 3..5, 1..5, 1..9, 0..16]
    /// ```
    fn list_attempts(
        &self,
        workdir: &Path,
        job: &Job,
        file_contents: &str,
        kill_trigger: &crossbeam_channel::Receiver<()>,
    ) -> anyhow::Result<Vec<Self::Attempt>>;

    /// Actually attempt the reduction suggested by `attempt`
    ///
    /// Note that the file currently at `workdir/job.path` could have been changed
    /// by previous attempts of this same pass. The pass should read the original
    /// file contents from the `file_contents` argument.
    fn attempt_reduce(
        &self,
        workdir: &Path,
        test: &dyn Test,
        attempt: Self::Attempt,
        attempt_number: usize,
        job: &Job,
        file_contents: &str,
        kill_trigger: &crossbeam_channel::Receiver<()>,
    ) -> anyhow::Result<JobStatus>;
}

impl<T> Pass for T
where
    T: Debug + DichotomyPass + Hash + Send + Sync,
{
    fn reduce(
        &self,
        workdir: &Path,
        test: &dyn Test,
        job: &Job,
        kill_trigger: &crossbeam_channel::Receiver<()>,
    ) -> anyhow::Result<JobStatus> {
        let path = workdir.join(&job.path);
        let file_contents =
            std::fs::read_to_string(&path).with_context(|| format!("reading file {path:?}"))?;
        let attempts = self.list_attempts(workdir, job, &file_contents, kill_trigger)?;
        if attempts.is_empty() {
            return Ok(JobStatus::PassFailed(String::from(
                "No option to choose from for {self:?}",
            )));
        }
        for (attempt_number, attempt) in attempts.into_iter().rev().enumerate() {
            match self.attempt_reduce(
                workdir,
                test,
                attempt,
                attempt_number,
                job,
                &file_contents,
                kill_trigger,
            )? {
                JobStatus::DidNotReduce => (), // go to next attempt
                res => return Ok(res),
            }
        }
        Ok(JobStatus::DidNotReduce)
    }
}
