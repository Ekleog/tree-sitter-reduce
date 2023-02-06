use std::{path::PathBuf, sync::Arc};

use crate::Pass;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobStatus {
    Reduced,
    DidNotReduce,
    PassFailed,
}

#[derive(Debug)]
pub(crate) struct Job {
    pub(crate) path: PathBuf,
    pub(crate) pass: Arc<dyn Pass>,
    pub(crate) seed: u64,
    pub(crate) recent_success_rate: u8,
}

pub(crate) struct JobResult {
    pub(crate) job: Job,
    pub(crate) res: anyhow::Result<JobStatus>,
}
