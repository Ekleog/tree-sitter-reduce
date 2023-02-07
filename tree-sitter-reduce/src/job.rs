use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::Pass;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobStatus {
    Reduced,
    DidNotReduce,
    PassFailed,
}

#[derive(Clone, Debug)]
pub(crate) struct Job {
    pub(crate) path: PathBuf,
    pub(crate) pass: Arc<dyn Pass>,
    pub(crate) seed: u64,
    pub(crate) recent_success_rate: u8,
    pub(crate) description: String,
}

pub(crate) struct JobResult {
    pub(crate) job: Job,
    pub(crate) res: anyhow::Result<JobStatus>,
}

impl Job {
    pub fn new(
        workdir: &Path,
        path: PathBuf,
        pass: Arc<dyn Pass>,
        seed: u64,
        recent_success_rate: u8,
    ) -> anyhow::Result<Job> {
        Ok(Job {
            description: pass.explain(&workdir, &path, seed, recent_success_rate)?,
            path,
            pass,
            seed,
            recent_success_rate,
        })
    }

    pub fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.path.hash(&mut hasher);
        self.pass.dyn_hash(&mut hasher);
        self.seed.hash(&mut hasher);
        self.recent_success_rate.hash(&mut hasher);
        hasher.finish()
    }

    pub fn explain(&self, workdir: &Path) -> anyhow::Result<String> {
        self.pass
            .explain(workdir, &self.path, self.seed, self.recent_success_rate)
    }
}
