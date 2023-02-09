# tree-sitter-reduce

`tree-sitter-reduce` is a library to build `creduce`-like binaries, in a way that attempts to make it really easy for any language that has a tree-sitter grammar.

In particular, `rsreduce` can currently be used to reduce full Rust crates, and [only requires minimal code](./rsreduce/src/main.rs).

If you are currently trying to reduce a big example in a not-yet-supported language, if you already some Rust chances are it would be faster to implement the necessary language-specific support then run `tree-sitter-reduce` than to minimize the example directly.

## Example usage: `rsreduce`

The first step when using `rsreduce` is to prepare a test driver, similar to `creduce` operation. Basically, you need to create a binary that, when run at the root of your crate and not depending on anything outside it, returns `0` if the bug reproduced and anything else if it did not. A typical example would be:
```bash
#!/bin/sh
(cargo build 2>&1 || true) | grep -q 'some error message'
```

You can then call `rsreduce` as follows:
```
$ cargo run --bin rsreduce -- \
    [path to your creduce-like test driver] \
    --root-path [path to the crate or workspace folder you want to reduce] \
    --snapshot-directory [path to an empty directory where snapshots will be saved]
```

Your test driver will then be run in a loop in copies of the folder you want to reduce, while minimizing the input. Note that the folder is reused across runs, to benefit of things like incremental compilation. If this is a problem for you, you can `rm target/` as part of your reproducer. However, it does also mean that your reproducer should not edit files in the working directory. If you really need to edit some files, you can do so in a temporary directory, similarly to `creduce` operation.

If you ever want to change some configuration, improve on your test runner, or just turn off your machine for a while, you can resume an interrupted run by replacing `--root-path [path]` with `--resume`. Then, `rsreduce` will resume reduction from the most recent snapshot in the snapshot directory.

By default, `rsreduce` writes at most one snapshot every 10 seconds, and keeps at most 10 of them, so if you want to investigate the current status while it is running you have at least 100 seconds to copy the snapshot and check it out. `rsreduce` does not read snapshots (except when resuming), so feel free to edit them. Also, `rsreduce` does not rely on the root path given to it staying constant, so you can continue development after spinning up an `rsreduce` instance.

See `rsreduce --help` for all the details about available command line options.
