use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use rand::{rngs::StdRng, SeedableRng};

use crate::{runner::Runner, Pass, Test};

#[derive(Debug, structopt::StructOpt)]
pub struct Opt {
    /// Path to the root of the crate (or workspace)
    ///
    /// The interestingness test will be run in a copy this folder. Note that copies
    /// will happen only during the startup of this program. So the folder can be
    /// changed after the program confirms it's running.
    root_path: PathBuf,

    /// If this option is passed, then only the file passed to it will be reduced
    ///
    /// Pass multiple times to reduce only a specific list of files in the root path.
    /// Paths are relative to the root path. By default all the files in the root path
    /// that this program knows how to reduce, will be reduced.
    #[structopt(long = "file")]
    only_files: Option<Vec<PathBuf>>,

    /// Number of interestingness tests to run in parallel
    #[structopt(long, short, default_value = "4")]
    jobs: usize,

    /// Seed for the random number generation
    #[structopt(long)]
    random_seed: Option<u64>,
}

impl Opt {
    pub fn canonicalized_root_path(&mut self) -> anyhow::Result<PathBuf> {
        self.root_path
            .canonicalize()
            .with_context(|| "canonicalizing root path {root:?}")
    }

    pub fn files(
        &self,
        default_list: impl Fn(&Path) -> anyhow::Result<Vec<PathBuf>>,
    ) -> anyhow::Result<Vec<PathBuf>> {
        match &self.only_files {
            Some(r) => Ok(r.clone()),
            None => default_list(&self.root_path),
        }
    }
}

pub fn run(
    mut opt: Opt,
    filelist: impl Fn(&Path) -> anyhow::Result<Vec<PathBuf>>,
    test: impl Test,
    passes: &[Arc<dyn Pass>],
) -> anyhow::Result<()> {
    // Handle the arguments
    let root = opt.canonicalized_root_path()?;
    let files = opt.files(filelist)?;
    let files = files.into_iter().collect::<HashSet<PathBuf>>();
    let seed = opt.random_seed.unwrap_or_else(rand::random);

    // Sanity-checks
    anyhow::ensure!(
        !files.is_empty(),
        "Cannot find any file to reduce in {root:?}",
    );
    anyhow::ensure!(
        !passes.is_empty(),
        "Ill-configured runner: no passes are configured",
    );

    // Actually run
    println!("Initial seed is < {seed} >. It can be used for reproduction if running with a single worker thread");
    let rng = StdRng::seed_from_u64(seed);
    Runner::new(root, test, files, passes, rng, opt.jobs)?.run()
}
