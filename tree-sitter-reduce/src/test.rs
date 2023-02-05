use std::path::{Path, PathBuf};

pub trait Test: 'static + Send + Sync {
    /// Run the test
    ///
    /// Returns `Err` in case an error made it impossible to know whether the current
    /// state is interesting. Returns `Ok(true)` if the current state is interesting,
    /// and `Ok(false)` if not.
    ///
    /// Note that if this returns `Err` then the current checkout will be considered
    /// broken and removed, so it should avoid doing so whenever possible.
    fn test_interesting(&self, root: &Path) -> anyhow::Result<bool>;
}

pub struct ShellTest<PrepFn, CleanFn> {
    prep: PrepFn,
    test: PathBuf,
    clean: CleanFn,
}

impl ShellTest<fn() -> anyhow::Result<()>, fn() -> anyhow::Result<()>> {
    pub fn new(test: PathBuf) -> Self {
        fn noop() -> anyhow::Result<()> {
            Ok(())
        }
        Self {
            prep: noop,
            test,
            clean: noop,
        }
    }
}

impl<PrepFn, CleanFn> ShellTest<PrepFn, CleanFn> {
    pub fn new_with_cleanup(test: PathBuf, prep: PrepFn, clean: CleanFn) -> Self {
        Self { prep, test, clean }
    }
}

impl<PrepFn, CleanFn> Test for ShellTest<PrepFn, CleanFn>
where
    PrepFn: 'static + Send + Sync + Fn() -> anyhow::Result<()>,
    CleanFn: 'static + Send + Sync + Fn() -> anyhow::Result<()>,
{
    fn test_interesting(&self, root: &Path) -> anyhow::Result<bool> {
        (self.prep)()?;
        let res = std::process::Command::new(&self.test)
            .current_dir(root)
            .output()?
            .status
            .success();
        (self.clean)()?;
        Ok(res)
    }
}
