// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! C parity for `util/chord.c`: the two notations, the keys whose names are
//! not their glyphs, and the spelling GTK puts in an exported menubar.

use whelk_rs::chord::{Chord, ChordSequence, ChordStyle, display};

#[test]
fn emacs_notation() {
    let chord = Chord::parse("C-v").expect("C-v");
    assert!(chord.ctrl);
    assert!(!chord.alt);
    assert!(!chord.shift);
    assert_eq!(chord.key, "v");

    let chord = Chord::parse("M-F4").expect("M-F4");
    assert!(chord.alt);
    assert_eq!(chord.key, "F4");

    let chord = Chord::parse("C-M-x").expect("C-M-x");
    assert!(chord.ctrl);
    assert!(chord.alt);
    assert_eq!(chord.key, "x");

    let chord = Chord::parse("C-S-p").expect("C-S-p");
    assert!(chord.ctrl);
    assert!(chord.shift);
    assert_eq!(chord.key, "p");

    let chord = Chord::parse("s-p").expect("s-p");
    assert!(chord.super_key);
    assert_eq!(chord.key, "p");

    // The key may be a plus or a minus, which is why the prefixes are read
    // before the separators.
    let chord = Chord::parse("C-+").expect("C-+");
    assert!(chord.ctrl);
    assert_eq!(chord.key, "plus");

    let chord = Chord::parse("C--").expect("C--");
    assert!(chord.ctrl);
    assert_eq!(chord.key, "minus");
}

#[test]
fn cua_notation() {
    let chord = Chord::parse("Ctrl+Shift+P").expect("Ctrl+Shift+P");
    assert!(chord.ctrl);
    assert!(chord.shift);
    assert_eq!(chord.key, "p");

    let chord = Chord::parse("Alt+F4").expect("Alt+F4");
    assert!(chord.alt);
    assert_eq!(chord.key, "F4");

    let chord = Chord::parse("Ctrl++").expect("Ctrl++");
    assert!(chord.ctrl);
    assert_eq!(chord.key, "plus");

    let chord = Chord::parse("Ctrl+-").expect("Ctrl+-");
    assert!(chord.ctrl);
    assert_eq!(chord.key, "minus");

    let chord = Chord::parse("Ctrl+,").expect("Ctrl+,");
    assert_eq!(chord.key, "comma");
}

#[test]
fn bare_and_named_keys() {
    let chord = Chord::parse("F11").expect("F11");
    assert!(!chord.ctrl);
    assert_eq!(chord.key, "F11");

    assert_eq!(Chord::parse("Space").expect("Space").key, "space");
    assert_eq!(Chord::parse("RET").expect("RET").key, "Return");
    assert_eq!(Chord::parse("DEL").expect("DEL").key, "BackSpace");
    assert_eq!(
        Chord::parse("Page_Down").expect("Page_Down").key,
        "Page_Down"
    );
}

#[test]
fn refusals() {
    assert!(Chord::parse("").is_none());
    assert!(Chord::parse("C-").is_none());
    assert!(Chord::parse("Ctrl+NotAModifier+V").is_none());
}

#[test]
fn gtk_accelerators() {
    let chord = Chord::parse_gtk("<Primary>q").expect("<Primary>q");
    assert!(chord.ctrl);
    assert_eq!(chord.key, "q");

    let chord = Chord::parse_gtk("<Shift><Control>a").expect("<Shift><Control>a");
    assert!(chord.ctrl);
    assert!(chord.shift);
    assert_eq!(chord.key, "a");

    let chord = Chord::parse_gtk("<Alt>F4").expect("<Alt>F4");
    assert!(chord.alt);
    assert_eq!(chord.key, "F4");
}

#[test]
fn formatting() {
    let chord = Chord::parse("C-S-p").expect("C-S-p");

    assert_eq!(
        chord.format(ChordStyle::Cua).as_deref(),
        Some("Ctrl+Shift+P")
    );
    assert_eq!(chord.format(ChordStyle::Emacs).as_deref(), Some("C-S-p"));

    let chord = Chord::parse("Ctrl++").expect("Ctrl++");

    assert_eq!(chord.format(ChordStyle::Cua).as_deref(), Some("Ctrl++"));
    assert_eq!(chord.format(ChordStyle::Emacs).as_deref(), Some("C-+"));
}

#[test]
fn display_understands_both_notations() {
    // The same stored chord, written the way either kind of user reads it.
    assert_eq!(display("C-v", ChordStyle::Cua), "Ctrl+V");
    assert_eq!(display("Ctrl+V", ChordStyle::Emacs), "C-v");
    assert_eq!(display("<Primary>q", ChordStyle::Emacs), "C-q");
    assert_eq!(display("Numpad1", ChordStyle::Cua), "Numpad 1");

    // What is not a chord is not mangled.
    assert_eq!(display("No chord", ChordStyle::Cua), "No chord");
}

#[test]
fn commands() {
    let chord = Chord::parse("C-S-p").expect("C-S-p");

    assert_eq!(
        chord.command("wtype").as_deref(),
        Some("wtype -M ctrl shift -k p -m ctrl shift")
    );
    assert_eq!(
        chord.command("xdotool").as_deref(),
        Some("xdotool key ctrl+shift+p")
    );

    let chord = Chord::parse("F11").expect("F11");

    assert_eq!(chord.command("wtype").as_deref(), Some("wtype -k F11"));
    assert!(chord.command("").is_none());
}

#[test]
fn sequences() {
    let sequence = ChordSequence::parse("C-x C-s").expect("C-x C-s");

    assert_eq!(sequence.chords.len(), 2);
    assert!(sequence.chords[0].ctrl);
    assert_eq!(sequence.chords[0].key, "x");
    assert!(sequence.chords[1].ctrl);
    assert_eq!(sequence.chords[1].key, "s");

    assert_eq!(
        sequence.format(ChordStyle::Cua).as_deref(),
        Some("Ctrl+X Ctrl+S")
    );
    assert_eq!(
        sequence.format(ChordStyle::Emacs).as_deref(),
        Some("C-x C-s")
    );
    assert_eq!(
        sequence.command("wtype").as_deref(),
        Some("wtype -M ctrl -k x -m ctrl && wtype -M ctrl -k s -m ctrl")
    );
    assert_eq!(
        sequence.command("xdotool").as_deref(),
        Some("xdotool key ctrl+x && xdotool key ctrl+s")
    );

    // A sequence may end in a bare key, as C-x 2 does.
    let sequence = ChordSequence::parse("C-x 2").expect("C-x 2");

    assert_eq!(sequence.chords.len(), 2);
    assert!(!sequence.chords[1].ctrl);
    assert_eq!(sequence.chords[1].key, "2");

    // The display path understands the same sequence.
    assert_eq!(display("C-x C-s", ChordStyle::Cua), "Ctrl+X Ctrl+S");
    assert_eq!(display("Ctrl+X Ctrl+S", ChordStyle::Emacs), "C-x C-s");

    // One broken chord makes the whole sequence unusable.
    assert!(ChordSequence::parse("C-x C-").is_none());
    assert!(ChordSequence::parse("").is_none());
    assert!(ChordSequence::default().command("wtype").is_none());
}

#[test]
fn styles() {
    assert_eq!(ChordStyle::from_name(Some("emacs")), ChordStyle::Emacs);
    assert_eq!(ChordStyle::from_name(Some("Emacs")), ChordStyle::Emacs);
    assert_eq!(ChordStyle::from_name(Some("cua")), ChordStyle::Cua);
    assert_eq!(ChordStyle::from_name(None), ChordStyle::Cua);
    assert_eq!(ChordStyle::Emacs.name(), "emacs");
    assert_eq!(ChordStyle::Cua.name(), "cua");
}

#[test]
fn gtk_sequences() {
    let sequence = ChordSequence::parse_gtk("<Ctrl>x <Ctrl>s").expect("<Ctrl>x <Ctrl>s");

    assert_eq!(sequence.chords.len(), 2);
    assert!(sequence.chords[0].ctrl);
    assert_eq!(sequence.chords[0].key, "x");
    assert!(sequence.chords[1].ctrl);
    assert_eq!(sequence.chords[1].key, "s");

    assert_eq!(
        sequence.format(ChordStyle::Cua).as_deref(),
        Some("Ctrl+X Ctrl+S")
    );
    assert_eq!(
        sequence.command("wtype").as_deref(),
        Some("wtype -M ctrl -k x -m ctrl && wtype -M ctrl -k s -m ctrl")
    );

    let sequence = ChordSequence::parse_gtk("<Ctrl>colon").expect("<Ctrl>colon");

    assert_eq!(sequence.chords[0].key, "colon");

    assert!(ChordSequence::parse_gtk("<Ctrl> <bogus").is_none());
    assert!(ChordSequence::parse_gtk("").is_none());
}
