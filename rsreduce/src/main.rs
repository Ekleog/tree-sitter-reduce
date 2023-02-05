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

    #[structopt(flatten)]
    other_opts: tree_sitter_reduce::Opt,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    // Rust testing needs no generic prep/cleanup
    let test = ShellTest::new(opt.test);
    // TODO: remove unwrap below
    tree_sitter_reduce::run(opt.other_opts, test, &[])
}
