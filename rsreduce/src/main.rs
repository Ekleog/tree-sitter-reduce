use std::path::PathBuf;

use structopt::StructOpt;
use tree_sitter_reduce::ShellTest;

#[derive(Debug, structopt::StructOpt)]
struct Opt {
    /// Path to the test that validates whether the input is interesting
    ///
    /// The test must return `0` if it is interesting, and non-0 otherwise. If you
    /// think this is the wrong way around, remember that just because the compiler
    /// crashes doesn't mean that it crashes the way you want; a syntax error is a
    /// crash.
    test: PathBuf,

    /// Path to the root of the crate (or workspace)
    ///
    /// The interestingness test will be run in a copy this folder.
    root_path: PathBuf,

    /// If this option is passed, then only the file passed to it will be reduced
    ///
    /// Pass multiple times to reduce only a specific list of files in the root path.
    /// Paths are relative to the root path, by default all `.rs` files in the root
    /// path will be reduced
    // TODO: also support reducing the toml files, to remove external deps?
    #[structopt(long = "file")]
    only_files: Option<Vec<PathBuf>>,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    // Rust testing needs no generic prep/cleanup
    let test = ShellTest::new(opt.test);
    // TODO: remove unwrap below
    tree_sitter_reduce::run(opt.root_path, opt.only_files.unwrap(), test, &[])
}
