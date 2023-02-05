use std::path::PathBuf;

use crate::{Pass, Test};

pub fn run(
    _root: PathBuf,
    _files: Vec<PathBuf>,
    _test: impl Test,
    _passes: &[&dyn Pass],
) -> anyhow::Result<()> {
    todo!()
}
