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
//!
//! The scanner is a `try_fold` over the query: its state is the cursor, the
//! current consecutive streak and the accumulated score, so a failed match is
//! just `None` rather than a mutable flag and an early return.

use std::cmp::Reverse;

/// Characters that start a word in the things a launcher searches: a space
/// between words, the separators of a reverse-DNS id, a path.
fn is_word_start(haystack: &[char], position: usize) -> bool {
    position == 0
        || haystack
            .get(position - 1)
            .is_some_and(|previous| matches!(previous, ' ' | '-' | '_' | '.' | '/' | ':' | '('))
}

/// The match score (higher is better), or `None` when `needle` is not a
/// subsequence of `haystack`.
///
/// An empty query is not a filter: everything matches, equally.
#[must_use]
pub fn score(needle: &str, haystack: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(0);
    }

    // The C scorer subtracts a byte length; keep that for parity even though
    // the scan itself is over characters.
    let byte_length = haystack.len();
    let haystack: Vec<char> = haystack.chars().collect();

    let (_, _, total) = needle
        .chars()
        .map(|character| character.to_ascii_lowercase())
        .try_fold((0usize, 0i32, 0i32), |(cursor, streak, total), wanted| {
            let offset = haystack
                .get(cursor..)?
                .iter()
                .position(|character| character.to_ascii_lowercase() == wanted)?;
            let position = cursor + offset;
            let consecutive = offset == 0 && streak > 0;
            let streak_bonus = if consecutive { 6 + streak } else { 0 };
            let word_bonus = if is_word_start(&haystack, position) {
                14
            } else {
                0
            };

            Some((
                position + 1,
                if offset == 0 { streak + 1 } else { 1 },
                total + 10 + word_bonus + streak_bonus,
            ))
        })?;

    Some(total - (byte_length / 8) as i32)
}

/// The index of the best-scoring candidate, or `None` when none matches.
///
/// Ties keep the earlier candidate: the caller's order is the second key.
#[must_use]
pub fn best(needle: &str, candidates: &[&str]) -> Option<usize> {
    candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| score(needle, candidate).map(|score| (index, score)))
        .max_by_key(|&(index, score)| (score, Reverse(index)))
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::{best, score};

    #[test]
    fn an_empty_query_is_not_a_filter() {
        assert_eq!(score("", "Firefox"), Some(0));
        assert_eq!(score("", ""), Some(0));
        assert_eq!(best("", &["Files", "Firefox"]), Some(0));
        assert_eq!(best("", &[]), None);
    }

    #[test]
    fn case_is_ignored() {
        assert_eq!(score("FIRE", "firefox"), score("fire", "firefox"));
    }

    #[test]
    fn a_word_start_scores_better_than_a_middle_match() {
        assert!(score("f", "firefox") > score("f", "afire"));
    }

    #[test]
    fn consecutive_characters_score_better_than_scattered_ones() {
        assert!(score("abc", "xabc") > score("abc", "xaxbxc"));
    }

    #[test]
    fn a_missing_character_is_no_match() {
        assert_eq!(score("fire", "ffx"), None);
        assert_eq!(best("zzz", &["Files", "Firefox"]), None);
    }

    #[test]
    fn best_keeps_the_earlier_candidate_on_a_tie() {
        assert_eq!(best("f", &["afire", "afire"]), Some(0));
    }
}
