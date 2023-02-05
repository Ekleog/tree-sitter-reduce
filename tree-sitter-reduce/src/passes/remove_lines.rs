use std::{
    fs::File,
    io::{BufRead, BufReader},
};

use anyhow::Context;
use rand::{rngs::StdRng, SeedableRng};
use rand_distr::{Distribution, Exp1};

use crate::Pass;

#[derive(Debug)]
pub struct RemoveLines;

impl Pass for RemoveLines {
    fn reduce(
        &self,
        path: &std::path::Path,
        random_seed: u64,
        recent_success_rate: u8,
    ) -> anyhow::Result<bool> {
        let file =
            BufReader::new(File::open(path).with_context(|| format!("opening file {path:?}"))?);
        let mut rng = StdRng::seed_from_u64(random_seed);
        let linecount = file.lines().count();
        for _ in 1..1000 {
            let num_lines_to_delete: f32 = Exp1.sample(&mut rng);
            println!("got value {num_lines_to_delete}");
        }

        todo!()
    }
}
