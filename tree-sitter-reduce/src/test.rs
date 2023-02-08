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
    ///
    /// `attempt_name` is a human-readable message that describes what is being tried
    /// by running this test. `attempt_id` is a hash of the same.
    fn test_interesting(
        &self,
        root: &Path,
        attempt_name: &str,
        attempt_id: u64,
    ) -> anyhow::Result<bool>;

    /// Cleanup a snapshot folder
    ///
    /// Remove all auto-generated files unneeded to reproduce from a snapshot folder.
    /// This will be called after each snapshot, on the folder that the user will then
    /// read.
    fn cleanup_snapshot(&self, root: &Path) -> anyhow::Result<()>;
}

pub struct ShellTest<PrepFn, CleanFn, SnapCleanFn> {
    prep: PrepFn,
    test: PathBuf,
    clean: CleanFn,
    snap_clean: SnapCleanFn,
}

impl
    ShellTest<
        fn(&Path) -> anyhow::Result<()>,
        fn(&Path) -> anyhow::Result<()>,
        fn(&Path) -> anyhow::Result<()>,
    >
{
    pub fn new(test: PathBuf) -> Self {
        fn noop(_: &Path) -> anyhow::Result<()> {
            Ok(())
        }
        Self {
            prep: noop,
            test,
            clean: noop,
            snap_clean: noop,
        }
    }
}

impl<PrepFn, CleanFn, SnapCleanFn> ShellTest<PrepFn, CleanFn, SnapCleanFn> {
    pub fn with_fixture<NewPrep, NewClean>(
        self,
        prep: NewPrep,
        clean: NewClean,
    ) -> ShellTest<NewPrep, NewClean, SnapCleanFn> {
        ShellTest {
            prep,
            test: self.test,
            clean,
            snap_clean: self.snap_clean,
        }
    }

    pub fn with_snapshot_cleanup<NewSnap>(
        self,
        snap_clean: NewSnap,
    ) -> ShellTest<PrepFn, CleanFn, NewSnap> {
        ShellTest {
            prep: self.prep,
            test: self.test,
            clean: self.clean,
            snap_clean,
        }
    }
}

impl<PrepFn, CleanFn, SnapCleanFn> Test for ShellTest<PrepFn, CleanFn, SnapCleanFn>
where
    PrepFn: 'static + Send + Sync + Fn(&Path) -> anyhow::Result<()>,
    CleanFn: 'static + Send + Sync + Fn(&Path) -> anyhow::Result<()>,
    SnapCleanFn: 'static + Send + Sync + Fn(&Path) -> anyhow::Result<()>,
{
    fn test_interesting(
        &self,
        root: &Path,
        _attempt_name: &str,
        _attempt_id: u64,
    ) -> anyhow::Result<bool> {
        (self.prep)(root)?;
        let res = std::process::Command::new(&self.test)
            .current_dir(root)
            .output()?
            .status
            .success();
        (self.clean)(root)?;
        Ok(res)
    }

    fn cleanup_snapshot(&self, root: &Path) -> anyhow::Result<()> {
        (self.snap_clean)(root)
    }
}
