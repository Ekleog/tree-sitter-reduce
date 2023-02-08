use std::{ops::Range, path::Path};

use anyhow::Context;
// TODO: actually use dichotomy here
use rand::{rngs::StdRng, Rng, SeedableRng};

use crate::{
    job::{Job, JobStatus},
    passes::DichotomyPass,
    Test, TestResult,
};

#[derive(Debug, Hash)]
pub struct RemoveLines;

impl DichotomyPass for RemoveLines {
    type Attempt = Range<usize>;

    fn list_attempts(
        &self,
        _workdir: &Path,
        job: &Job,
        file_contents: &str,
        _kill_trigger: &crossbeam_channel::Receiver<()>,
    ) -> anyhow::Result<Vec<Self::Attempt>> {
        let mut rng = StdRng::seed_from_u64(job.random_seed);
        let num_lines = file_contents.lines().count();
        if num_lines == 0 {
            return Ok(Vec::new());
        }
        let mut res = Vec::with_capacity(num_lines.ilog2() as usize + 1);
        let mut start_at = rng.gen_range(0..num_lines);
        let mut len = 1;
        while len < num_lines {
            res.push(start_at..(start_at + len));
            start_at = start_at.saturating_sub(rng.gen_range(0..len));
            len += rng.gen_range(1..(2 * len));
        }
        res.push(0..num_lines);
        Ok(res)
    }

    fn attempt_reduce(
        &self,
        workdir: &Path,
        test: &dyn Test,
        attempt: Self::Attempt,
        attempt_number: usize,
        job: &Job,
        file_contents: &str,
        kill_trigger: &crossbeam_channel::Receiver<()>,
    ) -> anyhow::Result<JobStatus> {
        let path = workdir.join(&job.path);

        let mut new_data = String::with_capacity(file_contents.len());
        for (l, line) in file_contents.lines().enumerate() {
            if !attempt.contains(&l) {
                new_data.push_str(line);
                new_data.push('\n');
            }
        }

        std::fs::write(&path, new_data)
            .with_context(|| format!("writing file {path:?} with reduced data"))?;

        let attempt = format!("Remove lines {attempt:?} of file {:?}", job.path);
        match test
            .test_interesting(workdir, kill_trigger, &attempt, job.id(attempt_number))
            .context("running the test")?
        {
            TestResult::Interesting => Ok(JobStatus::Reduced(attempt)),
            TestResult::NotInteresting => Ok(JobStatus::DidNotReduce),
            TestResult::Interrupted => Ok(JobStatus::Interrupted),
        }
    }
}
