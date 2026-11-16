// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The `systemd-inhibit --list` reader, ported from `util/inhibitors.c`.
//!
//! "Why did my screen not blank?" has an answer on a systemd machine: the
//! inhibitor list.  The command prints who holds what, for how long, and why;
//! whelk puts that in a tooltip instead of leaving the question to guesswork.
//!
//! Parsing text rather than calling logind keeps this a few lines and makes it
//! testable without a system bus; the command is only ever a reading.

/// One row of the table: who holds the inhibitor, what it blocks, why, and
/// whether the block is enforced (`block`) or only advisory (`delay`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inhibitor {
    pub who: String,
    pub what: String,
    pub why: String,
    pub mode: String,
}

/// Parses `systemd-inhibit --list --no-pager` output.
///
/// Empty input, the header, and lines this reader does not understand produce
/// no rows: dropping a shape that changed is better than inventing a reason for
/// a lock that is not there.
pub fn parse(text: &str) -> Vec<Inhibitor> {
    let mut list = Vec::new();

    for line in text.split('\n') {
        if line.is_empty() || line.starts_with("WHO") {
            continue;
        }

        let columns = split_columns(line);

        // WHO UID USER PID COMM WHAT WHY MODE.  Anything narrower is a line
        // this reader does not understand.
        if columns.len() < 8 {
            continue;
        }

        list.push(Inhibitor {
            who: columns[0].to_owned(),
            what: columns[5].to_owned(),
            why: columns[6].to_owned(),
            mode: columns[7].to_owned(),
        });
    }

    list
}

/// One line per inhibitor, ready for a tooltip; `None` when nothing is held.
pub fn describe(list: &[Inhibitor]) -> Option<String> {
    if list.is_empty() {
        return None;
    }

    let mut out = format!(
        "{} inhibitor{}",
        list.len(),
        if list.len() == 1 { "" } else { "s" }
    );

    for inhibitor in list {
        out.push_str(&format!(
            "\n{} — {}: {} ({})",
            or_default(&inhibitor.what, "idle"),
            or_default(&inhibitor.who, "an application"),
            or_default(&inhibitor.why, "no reason given"),
            or_default(&inhibitor.mode, "block")
        ));
    }

    Some(out)
}

fn or_default<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() { fallback } else { value }
}

/// `systemd-inhibit --list` pads its columns, so a run of two or more spaces is
/// a column boundary.  A single space inside a name, a program or a reason is
/// part of that field.
fn split_columns(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut columns = Vec::new();
    let mut start = 0;
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b' ' || bytes[index] == b'\t' {
            let run_start = index;

            while index < bytes.len() && (bytes[index] == b' ' || bytes[index] == b'\t') {
                index += 1;
            }

            if index - run_start >= 2 {
                columns.push(line[start..run_start].trim());
                start = index;
            }
        } else {
            index += 1;
        }
    }

    columns.push(line[start..].trim());

    columns
}
