use std::path::Path;

pub trait Pass: Send + Sync {
    /// Prepare the root path for this pass
    ///
    /// This will be called with as argument the `root` passed to `run`. It should
    /// make anything necessary to prepare the execution of the test on this pass.
    /// For instance, for rust tests it can mean copying the `target` directory if
    /// the `Cargo.toml` file was edited, so that it's possible to recover the
    /// incremental compilation situation if the test was not actually interesting.
    fn prepare(&self, root: &Path) -> anyhow::Result<()> {
        let _ = root;
        Ok(())
    }

    /// Edit the file at `path`, simplifying it
    ///
    /// Note that for proper operation this MUST BE DETERMINISTIC! For this reason, a
    /// `random_seed` argument is provided, which the pass can use to initialize an RNG.
    ///
    /// The `recent_success_rate` parameter is passed so that the passes can define how
    /// aggressive they want to be. Basically, the number will get closer to `u8::MAX`
    /// if recent passes have led to successful reductions, and closer to `0` if recent
    /// passes have failed to reduce the file size.
    fn reduce(&self, path: &Path, random_seed: u64, recent_success_rate: u8) -> anyhow::Result<()>;

    /// Cleanup the root path after this pass' test ran
    ///
    /// `root` is the path passed to `run`. `was_interesting` tells this function
    /// whether the run was actually interesting. For instance, a pass reducing
    /// `Cargo.toml` dependencies could use it to determine which of the `target`
    /// directories to keep.
    fn cleanup(&self, root: &Path, was_interesting: bool) -> anyhow::Result<()> {
        let _ = (root, was_interesting);
        Ok(())
    }
}
