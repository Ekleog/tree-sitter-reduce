use anyhow::Context;
use rand::{rngs::StdRng, Rng, SeedableRng};
use rand_distr::{Distribution, Exp1};

use crate::Pass;

#[derive(Debug)]
pub struct RemoveLines {
    pub average: usize,
}

impl Pass for RemoveLines {
    fn reduce(
        &self,
        path: &std::path::Path,
        random_seed: u64,
        recent_success_rate: u8,
    ) -> anyhow::Result<bool> {
        let file =
            std::fs::read_to_string(path).with_context(|| format!("reading file {path:?}"))?;

        let mut rng = StdRng::seed_from_u64(random_seed);
        let delete_lots: f32 = Exp1.sample(&mut rng); // average is 1
        let wanted_average = (f32::from(recent_success_rate) + 1.) * 20. / 256.;
        let num_dels = (delete_lots * wanted_average) as usize; // make avg somewhat related to success rate
        let delete_from = rng.gen_range(0..file.lines().count());

        let mut new_data = String::with_capacity(file.len());
        for (l, line) in file.lines().enumerate() {
            if l < delete_from || l >= delete_from + num_dels {
                new_data.push_str(line);
                new_data.push('\n');
            }
        }

        std::fs::write(path, new_data)
            .with_context(|| format!("writing file {path:?} with reduced data"))?;
        Ok(true)
    }
}
