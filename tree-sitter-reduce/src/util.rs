use std::{path::Path, time::Duration};

use anyhow::Context;
use tempfile::TempDir;

pub(crate) const WORKDIR: &str = "workdir";
pub(crate) const TMPDIR: &str = "tmpdir";

pub(crate) fn copy_dir_contents(from: &Path, to: &Path) -> anyhow::Result<()> {
    fs_extra::dir::copy(
        from,
        to,
        &fs_extra::dir::CopyOptions::default().content_only(true),
    )
    .with_context(|| format!("copying directory from {from:?} to {to:?}"))
    .map(|_| ())
}

pub(crate) fn clone_tempdir(root: &Path) -> anyhow::Result<TempDir> {
    let dir = tempfile::Builder::new()
        .prefix("tree-sitter-reduce-")
        .tempdir()
        .context("creating temporary directory")?;
    copy_dir_contents(root, dir.path())?;
    Ok(dir)
}

pub(crate) fn copy_to_tempdir(root: &Path) -> anyhow::Result<TempDir> {
    let dir = tempfile::Builder::new()
        .prefix("tree-sitter-reduce-")
        .tempdir()
        .context("creating temporary directory")?;
    let actual_path = dir.path().join(WORKDIR);
    std::fs::create_dir(&actual_path)
        .context("creating workdir nested under the temporary directory")?;
    copy_dir_contents(root, &actual_path)?;
    std::fs::create_dir(&dir.path().join(TMPDIR))
        .context("creating tempdir nested under the temporary directory")?;
    Ok(dir)
}

pub(crate) fn init_env() -> anyhow::Result<indicatif::MultiProgress> {
    // Setup the progress bar
    let progress = indicatif::MultiProgress::new();

    // Setup tracing
    let format = tracing_subscriber::fmt::format().with_target(false);
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .event_format(format)
        .with_writer(IndicatifWriter::new({
            let progress = progress.clone();
            move |buffer: &[u8]| progress.println(String::from_utf8_lossy(buffer))
        }))
        .init();

    Ok(progress)
}

pub(crate) fn make_progress_bar() -> indicatif::ProgressBar {
    let progress_style =
        indicatif::ProgressStyle::with_template("{prefix:.grey} {spinner:.blue} {wide_msg}")
            .expect("Failed to build progress bar style")
            .tick_strings(&[
                "⢀⠀", "⡀⠀", "⠄⠀", "⢂⠀", "⡂⠀", "⠅⠀", "⢃⠀", "⡃⠀", "⠍⠀", "⢋⠀", "⡋⠀", "⠍⠁", "⢋⠁", "⡋⠁",
                "⠍⠉", "⠋⠉", "⠋⠉", "⠉⠙", "⠉⠙", "⠉⠩", "⠈⢙", "⠈⡙", "⢈⠩", "⡀⢙", "⠄⡙", "⢂⠩", "⡂⢘", "⠅⡘",
                "⢃⠨", "⡃⢐", "⠍⡐", "⢋⠠", "⡋⢀", "⠍⡁", "⢋⠁", "⡋⠁", "⠍⠉", "⠋⠉", "⠋⠉", "⠉⠙", "⠉⠙", "⠉⠩",
                "⠈⢙", "⠈⡙", "⠈⠩", "⠀⢙", "⠀⡙", "⠀⠩", "⠀⢘", "⠀⡘", "⠀⠨", "⠀⢐", "⠀⡐", "⠀⠠", "⠀⢀", "⠀⡀",
            ]);
    let bar = indicatif::ProgressBar::new_spinner();
    bar.set_style(progress_style);
    bar
}

pub(crate) const BAR_TICK_INTERVAL: Duration = Duration::from_millis(100);

// Used to make tracing work well with indicatif
#[derive(Clone)]
struct IndicatifWriter<F>(F);

impl<F> IndicatifWriter<F> {
    pub fn new(f: F) -> Self {
        Self(f)
    }
}

impl<F: Fn(&[u8]) -> std::io::Result<()>> std::io::Write for IndicatifWriter<F> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        (self.0)(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<F: Clone + Fn(&[u8]) -> std::io::Result<()>> tracing_subscriber::fmt::MakeWriter<'_>
    for IndicatifWriter<F>
{
    type Writer = Self;

    fn make_writer(&self) -> Self::Writer {
        self.clone()
    }
}
