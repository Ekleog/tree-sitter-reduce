use std::{collections::VecDeque, fmt::Debug, hash::Hash, ops::Range};

use anyhow::Context;
use rand::{rngs::StdRng, Rng, SeedableRng};
use tree_sitter::TreeCursor;

use crate::{passes::DichotomyPass, JobStatus, TestResult};

pub struct TreeSitterReplace<F>
where
    F: Fn(&[u8], &tree_sitter::Node) -> Option<Vec<u8>>,
{
    /// Language to parse the input as
    pub language: tree_sitter::Language,

    /// Human-readable name of this pass
    pub name: String,

    /// Node matcher
    ///
    /// This is a function that takes as parameter the full file as bytes and
    /// a tree-sitter `Node`, and returns `Some(replace_with)` if this pass
    /// should try replacing this node by `replace_with`. The byte sequence
    /// represented by the node under test can be accessed with
    /// `&input[node.byte_range()]`
    pub node_matcher: F,

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

impl<F> Debug for TreeSitterReplace<F>
where
    F: Fn(&[u8], &tree_sitter::Node) -> Option<Vec<u8>>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tree-Sitter: {}", self.name)
    }
}

impl<F> Hash for TreeSitterReplace<F>
where
    F: Fn(&[u8], &tree_sitter::Node) -> Option<Vec<u8>>,
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.language.hash(state);
        self.name.hash(state);
        // self.node_matcher.hash(state);
        self.try_match_all_nodes.hash(state);
    }
}

#[derive(Clone, Debug)]
struct InterestingNode {
    bytes: Range<usize>,
    replace_with: Vec<u8>,
    children: InterestingNodeList,
}

#[derive(Clone, Debug)]
struct InterestingNodeList(VecDeque<Box<InterestingNode>>);

impl InterestingNodeList {
    fn count_bytes(&self) -> usize {
        self.0.iter().map(|n| n.bytes.len()).sum()
    }

    fn into_ranges(self) -> Vec<(Range<usize>, Vec<u8>)> {
        self.0
            .into_iter()
            .map(|n| (n.bytes, n.replace_with))
            .collect()
    }

    fn check_sorted(&self) -> bool {
        let mut cur = 0;
        for n in self.0.iter() {
            if cur > n.bytes.start {
                return false;
            }
            if n.bytes.start > n.bytes.end {
                return false;
            }
            cur = n.bytes.end;
        }
        true
    }

    fn try_remove_front(&mut self, mut size_to_remove: usize) -> usize {
        debug_assert!(
            self.check_sorted(),
            "Was unsorted before try_remove_front: {:?}",
            self.0
        );
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
        let item = self.0.pop_front().unwrap();
        removed += item.bytes.len() - item.children.count_bytes();
        let mut remaining_children = item.children;
        removed += remaining_children.try_remove_front(size_to_remove);
        remaining_children.0.append(&mut self.0);
        self.0 = remaining_children.0;
        debug_assert!(
            self.check_sorted(),
            "`try_remove_front` unsorted list {:?}",
            self.0
        );
        removed
    }

    fn try_remove_back(&mut self, mut size_to_remove: usize) -> usize {
        debug_assert!(
            self.check_sorted(),
            "Was unsorted before try_remove_back: {:?}",
            self.0
        );
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
        let item = self.0.pop_back().unwrap();
        removed += item.bytes.len() - item.children.count_bytes();
        let mut remaining_children = item.children;
        removed += remaining_children.try_remove_back(size_to_remove);
        self.0.append(&mut remaining_children.0);
        debug_assert!(
            self.check_sorted(),
            "`try_remove_back` unsorted list {:?}",
            self.0
        );
        removed
    }
}

impl<F> TreeSitterReplace<F>
where
    F: Fn(&[u8], &tree_sitter::Node) -> Option<Vec<u8>>,
{
    // TODO: refactor to use tree_sitter::QueryCursor::matches
    // Then the API can change to expose directly setting a tree-sitter query string and
    // then a function to know what to replace the returned captures list
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
            if !self.try_match_all_nodes && bytes.iter().all(u8::is_ascii_whitespace) {
                // Not-interesting node, just recurse
                self.collect_all_interesting(input, &mut *cursor, &mut *interesting);
            } else if let Some(replace_with) = (self.node_matcher)(&input, &node) {
                if self.try_match_all_nodes
                    || !replace_with.windows(bytes.len()).any(|b| b == bytes)
                {
                    // Interesting node, add to our list and recurse inside
                    let mut new_node = Box::new(InterestingNode {
                        bytes: node.byte_range(),
                        replace_with,
                        children: InterestingNodeList(VecDeque::new()),
                    });
                    self.collect_all_interesting(input, &mut *cursor, &mut new_node.children);
                    interesting.0.push_back(new_node);
                } else {
                    // Not-interesting node, just recurse
                    self.collect_all_interesting(input, &mut *cursor, &mut *interesting);
                }
            } else {
                // Not-interesting node, just recurse
                self.collect_all_interesting(input, &mut *cursor, &mut *interesting);
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
    F: Fn(&[u8], &tree_sitter::Node) -> Option<Vec<u8>>,
{
    // List of byte ranges to replace and the value to replace with
    type Attempt = Vec<(Range<usize>, Vec<u8>)>;

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
        if cur_bytes == 0 {
            return Ok(None);
        }
        attempts.push_back(interesting);
        'finished: loop {
            let mut attempt = attempts[attempts.len() - 1].clone();
            aim_at_bytes /= 2;
            if aim_at_bytes == 0 {
                break;
            }
            let mut removed_this_round = 0;
            while aim_at_bytes * 4 / 3 < cur_bytes {
                // allow slightly-too-big-for-dichotomy node sets, as we can't be precise
                // with what's being removed exactly
                let total_to_remove = cur_bytes - aim_at_bytes;
                let try_remove_now =
                    rng.gen_range(((total_to_remove + 1) / 2)..(total_to_remove + 1));
                let actually_removed = match rng.gen::<bool>() {
                    true => attempt.try_remove_front(try_remove_now),
                    false => attempt.try_remove_back(try_remove_now),
                };
                assert!(actually_removed != 0, "Tried removing {try_remove_now}B but failed to remove a single one! Current attempt is {attempt:?}");
                cur_bytes -= actually_removed;
                removed_this_round += actually_removed;
                debug_assert_eq!(
                    attempt.count_bytes(),
                    cur_bytes,
                    "`cur_bytes` cache diverged from real value: {cur_bytes} for {attempt:?}"
                );
                if cur_bytes == 0 {
                    break 'finished;
                }
            }
            assert_eq!(
                attempt.count_bytes(),
                cur_bytes,
                "`cur_bytes` cache diverged from real value"
            );
            if removed_this_round > 0 && cur_bytes > 0 {
                attempts.push_back(attempt);
            }
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

        let removed_size = attempt.iter().map(|(b, _)| b.len()).sum::<usize>();

        let mut new_data = Vec::with_capacity(file_contents.len() - removed_size);
        let mut file_cursor = 0;
        let mut replacement_size = 0;
        for (range, replace_with) in attempt.iter() {
            new_data.extend_from_slice(&file_contents[file_cursor..range.start]);
            new_data.extend_from_slice(&replace_with);
            replacement_size += replace_with.len();
            file_cursor = range.end;
        }
        new_data.extend_from_slice(&file_contents[file_cursor..]);

        std::fs::write(&path, new_data)
            .with_context(|| format!("writing file {path:?} with reduced data"))?;

        let attempt = format!(
            "{}: Replacing {removed_size}B with {replacement_size}B (ranges {attempt:?})",
            self.name,
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
