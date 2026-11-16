// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The launcher's fuzzy search, ported from `util/fuzzy.c`.
//!
//! A query matches when its characters appear in order, so `ffx` finds
//! `Firefox` and `wterm` finds `WezTerm`.  The score decides the order, and it
//! is shaped by what people mean when they type: a match at a word start is
//! worth more than one in the middle, a run of consecutive characters more than
//! scattered ones, and a short name more than a long one.

/// Characters that start a word in the things a launcher searches: a space
/// between words, the separators of a reverse-DNS id, a path.
fn is_word_start(haystack: &[char], position: usize) -> bool {
    if position == 0 {
        return true;
    }

    matches!(
        haystack[position - 1],
        ' ' | '-' | '_' | '.' | '/' | ':' | '('
    )
}

/// The match score (higher is better), or `None` when `needle` is not a
/// subsequence of `haystack`.
///
/// An empty query is not a filter: everything matches, equally.
pub fn score(needle: &str, haystack: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(0);
    }

    // The C scorer subtracts a byte length; keep that for parity even though
    // the scan itself is over characters.
    let byte_length = haystack.len();
    let haystack: Vec<char> = haystack.chars().collect();
    let mut score = 0i32;
    let mut streak = 0i32;
    let mut cursor = 0usize;

    for wanted in needle
        .chars()
        .map(|character| character.to_ascii_lowercase())
    {
        let mut found = false;

        while cursor < haystack.len() {
            if haystack[cursor].to_ascii_lowercase() == wanted {
                found = true;
                break;
            }

            cursor += 1;
            streak = 0;
        }

        if !found {
            return None;
        }

        score += 10;

        if is_word_start(&haystack, cursor) {
            score += 14;
        }

        if streak > 0 {
            score += 6 + streak;
        }

        streak += 1;
        cursor += 1;
    }

    // A short name that matched is a better answer than a long one that merely
    // contains the same characters.
    score -= (byte_length / 8) as i32;

    Some(score)
}

/// The index of the best-scoring candidate, or `None` when none matches.
///
/// Ties keep the earlier candidate: the caller's order is the second key.
pub fn best(needle: &str, candidates: &[&str]) -> Option<usize> {
    let mut best_index = None;
    let mut best_score = -1;

    for (index, candidate) in candidates.iter().enumerate() {
        if let Some(score) = score(needle, candidate) {
            if score > best_score {
                best_score = score;
                best_index = Some(index);
            }
        }
    }

    best_index
}

#[cfg(test)]
mod tests {
    use super::{best, score};

    #[test]
    fn an_empty_query_is_not_a_filter() {
        assert_eq!(score("", "Firefox"), Some(0));
        assert_eq!(score("", ""), Some(0));
        assert_eq!(best("", &["Files", "Firefox"]), Some(0));
    }

    #[test]
    fn case_is_ignored() {
        assert_eq!(score("FIRE", "firefox"), score("fire", "firefox"));
    }
}
