use std::{ops::Range, path::Path};

use anyhow::Context;
use rand::{rngs::StdRng, Rng, SeedableRng};
use rand_distr::{Distribution, Exp1};

use crate::Pass;

#[derive(Debug, Hash)]
pub struct RemoveLines {
    pub average: usize,
}

impl RemoveLines {
    fn read_file(&self, path: &Path) -> anyhow::Result<String> {
        std::fs::read_to_string(path).with_context(|| format!("reading file {path:?}"))
    }

    fn what_to_delete(
        &self,
        file: &str,
        random_seed: u64,
        recent_success_rate: u8,
    ) -> Option<Range<usize>> {
        let mut rng = StdRng::seed_from_u64(random_seed);
        let delete_lots: f32 = Exp1.sample(&mut rng); // average is 1
        let wanted_average = (f32::from(recent_success_rate) + 1.) * 20. / 256.;
        let num_dels = 1 + (delete_lots * wanted_average) as usize; // make avg somewhat related to success rate
        let num_lines = file.lines().count();
        match num_lines {
            0 => None,
            _ => Some({
                let delete_from = rng.gen_range(0..num_lines);
                delete_from..std::cmp::min(num_lines, delete_from + num_dels)
            }),
        }
    }
}

impl Pass for RemoveLines {
    fn reduce(
        &self,
        path: &Path,
        random_seed: u64,
        recent_success_rate: u8,
    ) -> anyhow::Result<bool> {
        let file = self.read_file(path)?;
        let to_delete = match self.what_to_delete(&file, random_seed, recent_success_rate) {
            Some(d) => d,
            None => return Ok(false),
        };

        let mut new_data = String::with_capacity(file.len());
        for (l, line) in file.lines().enumerate() {
            if !to_delete.contains(&l) {
                new_data.push_str(line);
                new_data.push('\n');
            }
        }

        std::fs::write(path, new_data)
            .with_context(|| format!("writing file {path:?} with reduced data"))?;
        Ok(true)
    }

    fn explain(
        &self,
        path: &Path,
        random_seed: u64,
        recent_success_rate: u8,
    ) -> anyhow::Result<String> {
        let to_delete =
            self.what_to_delete(&self.read_file(path)?, random_seed, recent_success_rate);
        Ok(format!("remove_lines({to_delete:?})"))
    }
}
