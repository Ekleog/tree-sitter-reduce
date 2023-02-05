use std::path::{PathBuf, Path};

pub trait Pass {
    /// Edit the file at `path`, simplifying it
    ///
    /// Note that for proper operation this MUST BE DETERMINISTIC! For this reason, a
    /// `random_seed` argument is provided, which the pass can use to initialize an RNG.
    fn run(&self, path: &Path, random_seed: u64);
}

pub fn run(
    _files: Vec<PathBuf>,
    _cleanup: impl FnMut() -> anyhow::Result<()>,
    _passes: &[&dyn Pass],
) -> anyhow::Result<()> {
    todo!()
}
