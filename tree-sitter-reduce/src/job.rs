use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;

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
    fn hash(&self, state: &mut DefaultHasher) {
        self.path.hash(state);
        self.pass.dyn_hash(state);
        self.seed.hash(state);
        self.recent_success_rate.hash(state);
    }
}

impl Job {
    pub fn explain(&self, workdir: &Path) -> anyhow::Result<String> {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        let hash = hasher.finish();
        let full_path = workdir.join(&self.path);
        Ok(format!(
            "{}@{:?}#{:x}",
            self.pass
                .explain(&full_path, self.seed, self.recent_success_rate)
                .with_context(|| format!("explaining pass for job {:?} in workdir {workdir:?}", self))?,
            &self.path,
            hash % 0xFFFF,
        ))
    }
}
