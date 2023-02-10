use anyhow::Context;

use crate::{JobStatus, Pass, TestResult};

/// Trim end-of-line whitespace and discard empty lines
#[derive(Debug, Hash)]
pub struct DiscardWhitespace;

impl Pass for DiscardWhitespace {
    fn reduce(
        &self,
        workdir: &std::path::Path,
        test: &dyn crate::Test,
        job: &crate::Job,
        kill_trigger: &crossbeam_channel::Receiver<()>,
    ) -> anyhow::Result<crate::JobStatus> {
        let path = &workdir.join(&job.path);
        let file_contents =
            std::fs::read(path).with_context(|| format!("reading file {path:?}"))?;

        let mut remaining_contents = &file_contents[..];
        let mut new_file_contents = Vec::with_capacity(file_contents.len());
        while !remaining_contents.is_empty() {
            let end_of_line = remaining_contents
                .iter()
                .position(|&b| b == b'\n')
                .unwrap_or(remaining_contents.len());
            let line = &remaining_contents[..end_of_line];
            remaining_contents =
                &remaining_contents[std::cmp::min(end_of_line + 1, remaining_contents.len())..];
            if let Some(last_char) = line.iter().rposition(|b| !b.is_ascii_whitespace()) {
                new_file_contents.extend_from_slice(&line[..(last_char + 1)]);
                new_file_contents.push(b'\n');
            }
        }

        let attempt = "Discard whitespace";
        if new_file_contents == file_contents {
            return Ok(JobStatus::PassFailed(String::from(attempt)));
        }
        std::fs::write(path, new_file_contents)
            .with_context(|| format!("writing file {path:?}"))?;
        match test
            .test_interesting(workdir, kill_trigger, attempt, job.id(0))
            .context("running the test")?
        {
            TestResult::Interesting => Ok(JobStatus::Reduced(String::from(attempt))),
            TestResult::NotInteresting => Ok(JobStatus::DidNotReduce),
            TestResult::Interrupted => Ok(JobStatus::Interrupted),
        }
    }
}
