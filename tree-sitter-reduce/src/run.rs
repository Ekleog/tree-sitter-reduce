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
    #[structopt(long, required_unless("resume"))]
    root_path: Option<PathBuf>,

    /// Resume from a previous reducer run
    ///
    /// This only works if there are already snapshots in the snapshot directory, ie.
    /// a previous reducer run was interrupted.
    #[structopt(long)]
    resume: bool,

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
    /// for most use cases. But if you try to minimize a directory so huge that even
    /// copying it to the snapshots folder slows reduction down, it could make sense
    /// to increase it.
    // TODO: allow customization (and disabling) of cleanup command for rsreduce
    // TODO: add a max_snapshots parameter to limit the number of kept snapshots for very long runs
    #[structopt(long, default_value = "10")]
    snapshot_interval: u64,

    /// Maximum number of snapshots to keep
    ///
    /// If disk space in the snapshot directory is a limited resource, you may want
    /// to enable this option. Note that *IT MAY DELETE ANYTHING IN THE SNAPSHOTS
    /// FOLDER*. So if you enable it, make sure there is nothing in the snapshots
    /// directory before running!
    // TODO: add ability to resume reducing from a snapshot... maybe we should just
    // fail if the snapshots folder already contains data in it when started and the
    // resume flag was not passed? then we can also default this to like 5.
    #[structopt(long)]
    max_snapshots: Option<usize>,

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
    pub fn real_root_path(&mut self) -> anyhow::Result<PathBuf> {
        if !self.resume {
            let root = self
                .root_path
                .as_ref()
                .expect("Structopt should not let root_path be None if resume was not set");
            root.canonicalize()
                .with_context(|| format!("canonicalizing root path {root:?}"))
        } else {
            let snap_dir = &self.snapshot_directory;
            let mut snapshots = std::fs::read_dir(snap_dir)
                .with_context(|| format!("listing snapshot directory {snap_dir:?}"))?
                .collect::<Result<Vec<_>, _>>()
                .with_context(|| format!("listing snapshot directory {snap_dir:?}"))?;
            snapshots.sort_by_key(|s| s.file_name());
            match snapshots.pop() {
                None => anyhow::bail!("No snapshots found in snapshot directory {snap_dir:?}, but `--resume` was provided"),
                Some(snapshot) => {
                    let snap = snapshot.path();
                    snap.canonicalize().with_context(|| format!("canonicalizing snapshot path {snap:?}"))
                }
            }
        }
    }

    pub fn files(
        &self,
        real_root_path: &Path,
        default_list: impl Fn(&Path) -> anyhow::Result<Vec<PathBuf>>,
    ) -> anyhow::Result<Vec<PathBuf>> {
        match &self.only_files {
            Some(r) => Ok(r.clone()),
            None => default_list(&real_root_path),
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
    let root = opt.real_root_path()?;
    let files = opt.files(&root, filelist)?;
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
        if !opt.resume {
            if let Some(e) = std::fs::read_dir(&snap_dir)
                .with_context(|| format!("listing snapshot directory {snap_dir:?}"))?
                .next()
            {
                anyhow::bail!("Snapshot directory already has elements like {e:?}, but `--resume` was not passed");
            }
        }
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
    if opt.resume && opt.root_path.is_some() {
        tracing::warn!("You provided a root path but asked to resume. The root path will be ignored in favor of the latest snapshot");
    }
    if opt.resume && opt.do_not_validate_input {
        tracing::warn!("You asked to resume without validating the input. This is usually a bad idea, remember that a snapshot could be half-written before the program stopped.");
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
        opt.max_snapshots.unwrap_or(usize::MAX),
        rng,
        opt.jobs,
        progress,
        opt.do_not_validate_input,
    )?
    .run()
}
