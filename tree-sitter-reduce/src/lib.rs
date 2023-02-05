use std::path::{Path, PathBuf};

pub trait Pass {
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
    fn reduce(&self, path: &Path, random_seed: u64) -> anyhow::Result<()>;

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

pub trait Test {
    /// Run the test
    ///
    /// Returns `Err` in case an error made it impossible to know whether the current
    /// state is interesting. Returns `Ok(true)` if the current state is interesting,
    /// and `Ok(false)` if not.
    fn test_interesting(&self, root: &Path) -> anyhow::Result<bool>;
}

pub struct ShellTest;

impl Test for ShellTest {
    fn test_interesting(&self, _root: &Path) -> anyhow::Result<bool> {
        todo!()
    }
}

pub fn run(
    _root: PathBuf,
    _files: Vec<PathBuf>,
    _test: impl Test,
    _passes: &[&dyn Pass],
) -> anyhow::Result<()> {
    todo!()
}
