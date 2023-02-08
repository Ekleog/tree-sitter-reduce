use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use rand::{rngs::StdRng, SeedableRng};

use crate::{runner::Runner, util::init_env, Pass, Test};

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

    /// The path to which to save snapshots
    ///
    /// This is where you should look to check whether the reducer managed to reduce
    /// enough to your taste, or whether you should keep it running for a while longer.
    ///
    /// Inside, the reducer will write folders that are reduced copies of the root
    /// folder, each folder name being the timestamp of the snapshot.
    #[structopt(long)]
    snapshot_directory: PathBuf,

    /// At which frequency (in seconds) to snapshot the state of reduction
    ///
    /// Note that if no reduction happened, then no snapshot will be taken. This can
    /// thus be set to 0 in order to snapshot every single time a reduction is found,
    /// which can be helpful for debugging the reducer itself.
    ///
    /// By default, snapshots will be taken every 10 seconds, which should be fine
    /// for most use cases. But if you have little disk space, or try to minimize a
    /// huge directory (eg. Rust with a `target/` directory if it is not removed on
    /// snapshot), it could make sense to increase it.
    // TODO: allow customization (and disabling) of cleanup command for rsreduce
    // TODO: add a max_snapshots parameter to limit the number of kept snapshots for very long runs
    #[structopt(long, default_value = "10")]
    snapshot_interval: u64,

    /// Number of interestingness tests to run in parallel
    #[structopt(long, short, default_value = "4")]
    jobs: usize,

    /// Seed for the random number generation
    #[structopt(long)]
    random_seed: Option<u64>,

    /// Skip checking whether the provided target directory is interesting
    #[structopt(long)]
    do_not_validate_input: bool,

    /// Do not display the spinners with current job info
    #[structopt(long)]
    no_progress_bars: bool,
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
    let progress = init_env(opt.no_progress_bars)?;
    tracing::trace!("Received options {opt:#?}");

    // Handle the arguments
    let root = opt.canonicalized_root_path()?;
    let files = opt.files(filelist)?;
    let files = files.into_iter().collect::<HashSet<PathBuf>>();
    let seed = opt.random_seed.unwrap_or_else(rand::random);
    let snap_dir = opt.snapshot_directory;

    // Sanity-checks
    anyhow::ensure!(
        !passes.is_empty(),
        "Ill-configured runner: no passes are configured",
    );
    anyhow::ensure!(
        !files.is_empty(),
        "Cannot find any file to reduce in {root:?}",
    );
    {
        let testdir = snap_dir.join("test");
        std::fs::create_dir(&testdir).with_context(|| {
            format!("checking whether the snapshot directory {snap_dir:?} is writable")
        })?;
        std::fs::remove_dir(&testdir)
            .with_context(|| format!("removing test directory {testdir:?}"))?;
    }
    if opt.snapshot_interval > 300 {
        tracing::warn!("You set snapshot interval to more than 5 minutes.");
        tracing::warn!("This usually slows down the time to receive the results, without getting anything in return");
    }

    // Actually run
    tracing::info!("Initial seed is < {seed} >. It can be used for reproduction if running with a single worker thread");
    let rng = StdRng::seed_from_u64(seed);
    Runner::new(
        root,
        test,
        files,
        passes,
        snap_dir,
        Duration::from_secs(opt.snapshot_interval),
        rng,
        opt.jobs,
        progress,
        opt.do_not_validate_input,
    )?
    .run()
}
