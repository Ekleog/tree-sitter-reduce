use std::path::Path;

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
