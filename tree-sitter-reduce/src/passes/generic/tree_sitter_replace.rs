use std::{collections::VecDeque, ops::Range};

use anyhow::Context;
use rand::{rngs::StdRng, Rng, SeedableRng};
use tree_sitter::TreeCursor;

use crate::{passes::DichotomyPass, JobStatus, TestResult};

pub struct TreeSitterReplace<F> {
    /// Language to parse the input as
    pub language: tree_sitter::Language,

    /// Node matcher
    ///
    /// This is a function that takes as parameter the full file as bytes and
    /// a tree-sitter `Node`, and returns `true` if this pass should try
    /// replacing this node by `replace_with`. The byte sequence represented
    /// by the node under test can be accessed with `&input[node.byte_range()]`
    ///
    /// Note that this function is expected to be fast, so for performance
    /// reasons, even if `try_match_all_nodes` is not set it will actually be
    /// run on all nodes and its result will be ignored on nodes for which
    /// `try_match_all_nodes` say they should be ignored.
    ///
    /// If this is a problem for you, you should set `try_match_all_nodes` to
    /// `true` and reimplement its behavior straight in `node_matcher`.
    pub node_matcher: F,

    /// Attempt replacing nodes matched by `node_matcher` by this byte sequence
    pub replace_with: Vec<u8>,

    /// If false (the default), try to match only the nodes that look like they
    /// could cause a reduction
    ///
    /// In practice, this means nodes that are only whitespace or that are a
    /// substring of `replace_with` will get ignored and never matched. This is
    /// an effort to avoid the pass from actually infinitely growing the input
    /// if repeated.
    ///
    /// However, `node_matcher` should implement more proper validation, as
    /// when coupled with other passes this pass could still lead to unchecked
    /// input growth (eg. a pass doing A -> BB and a pass doing B -> AA)
    pub try_match_all_nodes: bool,
}

#[derive(Clone)]
struct InterestingNode {
    bytes: Range<usize>,
    children: InterestingNodeList,
}

#[derive(Clone)]
struct InterestingNodeList(VecDeque<Box<InterestingNode>>);

impl InterestingNodeList {
    fn count_bytes(&self) -> usize {
        self.0.iter().map(|n| n.bytes.len()).sum()
    }

    fn into_ranges(self) -> Vec<Range<usize>> {
        self.0.into_iter().map(|n| n.bytes).collect()
    }

    fn try_remove_front(&mut self, mut size_to_remove: usize) -> usize {
        let mut removed = 0;
        loop {
            if self.0.is_empty() {
                return removed;
            }
            let item_size = self.0.front_mut().unwrap().bytes.len();
            if item_size > size_to_remove {
                break;
            }
            removed += item_size;
            size_to_remove -= item_size;
            self.0.pop_front();
        }
        let mut remaining_children = self.0.pop_front().unwrap().children;
        removed += remaining_children.try_remove_front(size_to_remove);
        remaining_children.0.append(&mut self.0);
        self.0 = remaining_children.0;
        removed
    }

    fn try_remove_back(&mut self, mut size_to_remove: usize) -> usize {
        let mut removed = 0;
        loop {
            if self.0.is_empty() {
                return removed;
            }
            let item_size = self.0.back_mut().unwrap().bytes.len();
            if item_size > size_to_remove {
                break;
            }
            removed += item_size;
            size_to_remove -= item_size;
            self.0.pop_back();
        }
        let mut remaining_children = self.0.pop_back().unwrap().children;
        removed += remaining_children.try_remove_front(size_to_remove);
        self.0.append(&mut remaining_children.0);
        removed
    }
}

impl<F> TreeSitterReplace<F>
where
    F: Fn(&[u8], &tree_sitter::Node) -> bool,
{
    fn collect_all_interesting(
        &self,
        input: &[u8],
        cursor: &mut TreeCursor,
        interesting: &mut InterestingNodeList,
    ) {
        if !cursor.goto_first_child() {
            return;
        }
        loop {
            let node = cursor.node();
            let bytes = &input[node.byte_range()];
            let node_is_interesting = (self.node_matcher)(input, &node)
                && (self.try_match_all_nodes
                    || (!bytes.iter().all(u8::is_ascii_whitespace)
                        && !self.replace_with.windows(bytes.len()).any(|b| b == bytes)));
            if !node_is_interesting {
                // Not-interesting node, just recurse
                self.collect_all_interesting(input, &mut *cursor, &mut *interesting);
            } else {
                // Interesting node, add to our list and recurse inside
                let mut new_node = Box::new(InterestingNode {
                    bytes: node.byte_range(),
                    children: InterestingNodeList(VecDeque::new()),
                });
                self.collect_all_interesting(input, &mut *cursor, &mut new_node.children);
                interesting.0.push_back(new_node);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        assert!(
            cursor.goto_parent(),
            "Failed to send the cursor back to the parent"
        );
    }
}

impl<F> DichotomyPass for TreeSitterReplace<F>
where
    F: Fn(&[u8], &tree_sitter::Node) -> bool,
{
    type Attempt = Vec<Range<usize>>; // List of byte ranges to replace

    type Parsed = Vec<u8>;

    fn list_attempts(
        &self,
        workdir: &std::path::Path,
        job: &crate::Job,
        _kill_trigger: &crossbeam_channel::Receiver<()>,
    ) -> anyhow::Result<Option<(Self::Parsed, VecDeque<Self::Attempt>)>> {
        // Load the file
        let path = workdir.join(&job.path);
        let file_contents =
            std::fs::read(&path).with_context(|| format!("reading file {path:?}"))?;

        // Parse the file
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(self.language)
            .expect("Failed to make a parser with configured language");
        let tree = match parser.parse(&file_contents, None) {
            Some(t) => t,
            None => return Ok(None),
        };

        // Collect all interesting nodes as per matcher
        let mut cursor = tree.walk();
        let mut interesting = InterestingNodeList(VecDeque::new());
        self.collect_all_interesting(&file_contents, &mut cursor, &mut interesting);

        // Select the byte ranges to replace
        let mut rng = StdRng::seed_from_u64(job.random_seed);
        let mut attempts = VecDeque::new();
        let mut aim_at_bytes = interesting.count_bytes();
        let mut cur_bytes = aim_at_bytes;
        attempts.push_back(interesting);
        loop {
            let mut attempt = attempts[attempts.len() - 1].clone();
            aim_at_bytes /= 2;
            if aim_at_bytes == 0 {
                break;
            }
            while aim_at_bytes * 4 / 3 < cur_bytes {
                // allow slightly-too-big-for-dichotomy node sets, as we can't be precise
                // with what's being removed exactly
                let try_remove_now = rng.gen_range(1..(cur_bytes - aim_at_bytes));
                let actually_removed = match rng.gen::<bool>() {
                    true => attempt.try_remove_front(try_remove_now),
                    false => attempt.try_remove_back(try_remove_now),
                };
                cur_bytes -= actually_removed;
            }
            attempts.push_back(attempt);
        }

        Ok(Some((
            file_contents,
            attempts.into_iter().map(|a| a.into_ranges()).collect(),
        )))
    }

    fn attempt_reduce(
        &self,
        workdir: &std::path::Path,
        test: &dyn crate::Test,
        attempt: Self::Attempt,
        attempt_number: usize,
        job: &crate::Job,
        file_contents: &Self::Parsed,
        kill_trigger: &crossbeam_channel::Receiver<()>,
    ) -> anyhow::Result<crate::JobStatus> {
        let path = workdir.join(&job.path);

        let removed_size = attempt.iter().map(Range::len).sum::<usize>();
        let replacement_size = attempt.len() * self.replace_with.len();

        let mut new_data =
            Vec::with_capacity(file_contents.len() - removed_size + replacement_size);
        let mut file_cursor = 0;
        for r in attempt.iter() {
            new_data.extend_from_slice(&file_contents[file_cursor..r.start]);
            new_data.extend_from_slice(&self.replace_with);
            file_cursor = r.end;
        }
        new_data.extend_from_slice(&file_contents[file_cursor..]);

        std::fs::write(&path, new_data)
            .with_context(|| format!("writing file {path:?} with reduced data"))?;

        let attempt = format!(
            "Replacing {removed_size}B with {replacement_size}B of {:?} (ranges {attempt:?})",
            String::from_utf8_lossy(&self.replace_with),
        );

        match test
            .test_interesting(workdir, kill_trigger, &attempt, job.id(attempt_number))
            .context("running the test")?
        {
            TestResult::Interesting => Ok(JobStatus::Reduced(attempt)),
            TestResult::NotInteresting => Ok(JobStatus::DidNotReduce),
            TestResult::Interrupted => Ok(JobStatus::Interrupted),
        }
    }
}
