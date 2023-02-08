use std::{collections::hash_map::DefaultHasher, fmt::Debug, hash::Hash, path::Path};

use crate::{
    job::{Job, JobStatus},
    Test,
};

pub trait Pass: Debug + DynHash + Send + Sync {
    /// Edit the file at `path`, simplifying it
    ///
    /// `kill_trigger` should be passed through to the `Test::test_interesting`.
    ///
    /// Note that for proper operation this MUST BE DETERMINISTIC! For this reason, a
    /// `random_seed` argument is provided, which the pass can use to initialize an RNG.
    /// Also, this path should not edit the other files in `workdir`, but only use it
    /// to run the test on them.
    ///
    /// The `recent_success_rate` parameter is passed so that the passes can define how
    /// aggressive they want to be. Basically, the number will get closer to `u8::MAX`
    /// if recent passes have led to successful reductions, and closer to `0` if recent
    /// passes have failed to reduce the file size.
    ///
    /// This function is expected to return `true` if it successfully reduced the input,
    /// and `false` if the current input cannot be handled by this pass. Returning errors
    /// should be reserved to situations where the pass crashed midways and the whole
    /// directory needs to be reset.
    ///
    /// Note that if the pass returns `PassFailed`, it is assumed that it did not touch
    /// the file, and so its contents does not need to be restored. Failure modes that do
    /// not allow the pass to clean up after itself should just result in `Err`.
    ///
    /// Also note that the pass is allowed to run the test multiple times, in order to
    /// implement things such as dichotomy.
    fn reduce(
        &self,
        workdir: &Path,
        test: &dyn Test,
        job: &Job,
        kill_trigger: &crossbeam_channel::Receiver<()>,
    ) -> anyhow::Result<JobStatus>;
}

pub trait DynHash {
    fn dyn_hash(&self, hasher: &mut DefaultHasher);
}

impl<T: Hash> DynHash for T {
    fn dyn_hash(&self, hasher: &mut DefaultHasher) {
        self.hash(hasher)
    }
}
