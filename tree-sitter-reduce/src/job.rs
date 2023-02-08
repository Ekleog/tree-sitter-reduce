use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};

use crate::Pass;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JobStatus {
    /// Job did manage to reduce the input
    ///
    /// The `String` is a human-readable description of which changes the pass
    /// ended up performing on the input to get to the reduced input.
    Reduced(String),

    /// Job did not find a way to reduce the input
    DidNotReduce,

    /// Job failed to apply to the input
    ///
    /// The `String` is a human-readable description of why the pass could not apply.
    /// For example, `Could not remove functions in a file with no functions`.
    PassFailed(String),

    /// The pass run was interrupted
    Interrupted,
}

impl JobStatus {
    pub fn did_reduce(&self) -> bool {
        match self {
            JobStatus::Reduced(_) => true,
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Job {
    pub path: PathBuf,
    pub(crate) pass: Arc<dyn Pass>,
    pub random_seed: u64,
    pub recent_success_rate: u8,
}

pub(crate) struct JobResult {
    pub(crate) job: Job,
    pub(crate) res: anyhow::Result<JobStatus>,
}

impl Job {
    pub(crate) fn new(
        path: PathBuf,
        pass: Arc<dyn Pass>,
        random_seed: u64,
        recent_success_rate: u8,
    ) -> anyhow::Result<Job> {
        Ok(Job {
            path,
            pass,
            random_seed,
            recent_success_rate,
        })
    }

    /// Identifier of this job, supposed to be passed to `Test::test_interesting`
    ///
    /// The `attempt_number` is the number of the attempt for multi-attempt passes.
    pub fn id(&self, attempt_number: usize) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.path.hash(&mut hasher);
        self.pass.dyn_hash(&mut hasher);
        self.random_seed.hash(&mut hasher);
        self.recent_success_rate.hash(&mut hasher);
        attempt_number.hash(&mut hasher);
        hasher.finish()
    }
}
