// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Searching the home directory, ported from `util/filesearch.c`.
//!
//! The launcher's file mode answers "where is that thing I wrote".  Two rules
//! make it useful: every directory under the home is walked, so a file is found
//! wherever it lives, and the answer arrives in the order a person would rank
//! it -- the file whose own name matches first, then the shallower paths, with
//! directories behind files of the same name.
//!
//! The walk is a `find` that prints each entry's type with its path, so ranking
//! never has to stat a hundred thousand paths to know which are directories.

/// What the walk never enters: caches, repositories, package trees and
/// version-control internals, in volumes nobody wants to pay for on every
/// keystroke.
pub const IGNORED: [&str; 15] = [
    ".cache",
    ".cargo",
    ".git",
    ".hg",
    ".mozilla",
    ".npm",
    ".rustup",
    ".steam",
    ".svn",
    ".thunderbird",
    ".venv",
    ".wine",
    "node_modules",
    "__pycache__",
    "Trash",
];

pub fn ignored() -> &'static [&'static str] {
    &IGNORED
}

/// Scores one path for `query`; `-1` means it does not match at all.
///
/// A basename prefix beats a basename substring, which beats a whole path
/// component, which beats a substring anywhere in the path.  Each directory a
/// match hides under costs a point, and a directory is a place, not the thing
/// that was asked for.
pub fn score(query: &str, path: &str, is_directory: bool) -> i32 {
    if query.is_empty() {
        return 0;
    }

    if path.is_empty() {
        return -1;
    }

    let query_lower = query.to_lowercase();
    let path_lower = path.to_lowercase();
    let base_lower = path.rsplit('/').next().unwrap_or(path).to_lowercase();

    let mut score = if base_lower.starts_with(&query_lower) {
        100
    } else if base_lower.contains(&query_lower) {
        70
    } else if path_lower.split('/').any(|part| part == query_lower) {
        45
    } else if path_lower.contains(&query_lower) {
        30
    } else {
        return -1;
    };

    score -= path.matches('/').count() as i32;

    if is_directory {
        score -= 10;
    }

    score
}

/// Ranks the output of the walk (`find -printf '%y\\t%p\\n'`) and returns at
/// most `limit` paths.
pub fn rank(query: &str, output: &str, limit: u32) -> Vec<String> {
    rank_entries(query, output, limit)
        .into_iter()
        .map(|(path, _)| path)
        .collect()
}

/// [`rank`], keeping the type the walk reported.  A launcher row needs to know
/// whether it is offering a directory before it can paint the trailing slash.
pub fn rank_entries(query: &str, output: &str, limit: u32) -> Vec<(String, bool)> {
    if query.is_empty() {
        return Vec::new();
    }

    let mut ranked: Vec<(i32, String, bool)> = output
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let bytes = line.as_bytes();
            let (is_directory, path) = if bytes.get(1) == Some(&b'\t') {
                (bytes[0] == b'd', &line[2..])
            } else {
                (false, line)
            };
            let score = score(query, path, is_directory);

            (score >= 0).then(|| (score, path.to_owned(), is_directory))
        })
        .collect();

    ranked.sort_by(|(left_score, left_path, _), (right_score, right_path, _)| {
        right_score
            .cmp(left_score)
            .then(left_path.len().cmp(&right_path.len()))
            .then(left_path.cmp(right_path))
    });

    ranked
        .into_iter()
        .take(limit as usize)
        .map(|(_, path, is_directory)| (path, is_directory))
        .collect()
}

/// The `find` command the walk runs.  `-printf`, not `-print`: the type comes
/// back with the path so ranking knows which results are directories.
pub fn command(root: &str, max_depth: u32) -> String {
    let mut command = format!(
        "find {} -mindepth 1 -maxdepth {max_depth} \\(",
        shell_quote(root)
    );

    for (index, name) in IGNORED.iter().enumerate() {
        command.push_str(if index == 0 { " " } else { " -o " });
        command.push_str(&format!("-name {name}"));
    }

    command.push_str(r" \) -prune -o \( -type f -o -type d \) -printf '%y\t%p\n'");

    command
}

/// POSIX single-quote quoting, the same shape `g_shell_quote` produces.
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}
