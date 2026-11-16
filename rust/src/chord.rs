// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Key chords, ported from `util/chord.c`.
//!
//! A chord is a key and the modifiers held with it, and it is written two ways
//! on a desktop.  CUA writes `Ctrl+Shift+P`; Emacs writes `C-S-p`, and its
//! documentation and its users write `M-F4`, `C-x`, `s-p` -- one letter and a
//! dash per modifier, in any order.  The shell reads both, because the menu
//! dictionary is configuration and a person who lives in Emacs should not have
//! to translate their own muscle memory to add a menu entry.
//!
//! What a chord is stored as is not what it is shown as: the dictionary may
//! write `C-v`, the menu may display `Ctrl+V` or `C-v`, and what gets typed
//! into the focused window is the same key either way.  Parsing produces the
//! key name `wtype` and `xdotool` understand (`v`, `F4`, `Return`, `plus`), not
//! the character on the key cap.

/// How a chord is written for a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordStyle {
    /// `Ctrl+Shift+P`, `Alt+F4`.
    Cua,
    /// `C-S-p`, `M-F4`.
    Emacs,
}

impl ChordStyle {
    /// A style from its configuration spelling: `"emacs"`, or CUA for
    /// anything else.
    pub fn from_name(name: Option<&str>) -> Self {
        match name {
            Some(name) if name.eq_ignore_ascii_case("emacs") => Self::Emacs,
            _ => Self::Cua,
        }
    }

    /// `"emacs"` or `"cua"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Emacs => "emacs",
            Self::Cua => "cua",
        }
    }
}

/// One key and the modifiers held with it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The Super/Windows/Command key; `super` is a Rust keyword.
    pub super_key: bool,
    /// The keysym name to type, not the character on the key cap.
    pub key: String,
}

/// An Emacs key sequence such as `C-x C-s`.  Most chords are one chord long;
/// Emacs is not, and its prefixes are the reason this type exists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChordSequence {
    pub chords: Vec<Chord>,
}

// ---------------------------------------------------------------------------
// The key names
// ---------------------------------------------------------------------------

/// Menus write "Return" or "RET", CUA writes "Delete", Emacs writes "DEL" for
/// the backspace key, and wtype wants a keysym name.  One table, read
/// case-insensitively, is the whole translation.
const KEY_ALIASES: &[(&str, &str)] = &[
    ("ret", "Return"),
    ("return", "Return"),
    ("enter", "Return"),
    ("tab", "Tab"),
    ("spc", "space"),
    ("space", "space"),
    ("esc", "Escape"),
    ("escape", "Escape"),
    ("del", "BackSpace"),
    ("bs", "BackSpace"),
    ("backspace", "BackSpace"),
    ("delete", "Delete"),
    ("ins", "Insert"),
    ("insert", "Insert"),
    ("home", "Home"),
    ("end", "End"),
    ("pgup", "Page_Up"),
    ("prior", "Page_Up"),
    ("pageup", "Page_Up"),
    ("page_up", "Page_Up"),
    ("pgdn", "Page_Down"),
    ("next", "Page_Down"),
    ("pagedown", "Page_Down"),
    ("page_down", "Page_Down"),
    ("left", "Left"),
    ("right", "Right"),
    ("up", "Up"),
    ("down", "Down"),
    ("plus", "plus"),
    ("+", "plus"),
    ("minus", "minus"),
    ("-", "minus"),
    ("equal", "equal"),
    ("=", "equal"),
    ("comma", "comma"),
    (",", "comma"),
    ("period", "period"),
    (".", "period"),
    ("slash", "slash"),
    ("/", "slash"),
    ("semicolon", "semicolon"),
    (";", "semicolon"),
    ("apostrophe", "apostrophe"),
    ("'", "apostrophe"),
    ("backslash", "backslash"),
    ("\\", "backslash"),
    ("bracketleft", "bracketleft"),
    ("[", "bracketleft"),
    ("bracketright", "bracketright"),
    ("]", "bracketright"),
    ("grave", "grave"),
    ("`", "grave"),
];

/// The display of a keysym, which is not the keysym: a menu says "+", not
/// "plus", and "Page Up", not "Page_Up".
const KEY_DISPLAYS: &[(&str, &str)] = &[
    ("plus", "+"),
    ("minus", "-"),
    ("equal", "="),
    ("comma", ","),
    ("period", "."),
    ("slash", "/"),
    ("semicolon", ";"),
    ("apostrophe", "'"),
    ("backslash", "\\"),
    ("bracketleft", "["),
    ("bracketright", "]"),
    ("grave", "`"),
    ("space", "Space"),
    ("BackSpace", "Backspace"),
    ("Page_Up", "Page Up"),
    ("Page_Down", "Page Down"),
    ("colon", ":"),
    ("exclam", "!"),
    ("quotedbl", "\""),
    ("numbersign", "#"),
    ("dollar", "$"),
    ("percent", "%"),
    ("ampersand", "&"),
    ("parenleft", "("),
    ("parenright", ")"),
    ("asterisk", "*"),
    ("less", "<"),
    ("greater", ">"),
    ("question", "?"),
    ("at", "@"),
    ("asciicircum", "^"),
    ("underscore", "_"),
    ("braceleft", "{"),
    ("bar", "|"),
    ("braceright", "}"),
    ("asciitilde", "~"),
];

fn lookup_alias(written: &str) -> Option<&'static str> {
    KEY_ALIASES
        .iter()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(written))
        .map(|(_, key)| *key)
}

/// A single character as the keysym name that types it.
fn symbol_key(symbol: char) -> Option<&'static str> {
    Some(match symbol {
        '+' => "plus",
        '-' => "minus",
        '=' => "equal",
        ',' => "comma",
        '.' => "period",
        '/' => "slash",
        ';' => "semicolon",
        '\'' => "apostrophe",
        '\\' => "backslash",
        '[' => "bracketleft",
        ']' => "bracketright",
        '`' => "grave",
        ' ' => "space",
        '!' => "exclam",
        '"' => "quotedbl",
        '#' => "numbersign",
        '$' => "dollar",
        '%' => "percent",
        '&' => "ampersand",
        '(' => "parenleft",
        ')' => "parenright",
        '*' => "asterisk",
        ':' => "colon",
        '<' => "less",
        '>' => "greater",
        '?' => "question",
        '@' => "at",
        '^' => "asciicircum",
        '_' => "underscore",
        '{' => "braceleft",
        '|' => "bar",
        '}' => "braceright",
        '~' => "asciitilde",
        _ => return None,
    })
}

/// The keysym name for what a person wrote, or `None` when it cannot be typed.
fn canonical_key(written: &str) -> Option<String> {
    let trimmed = written.trim();

    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return None;
    }

    if let Some(alias) = lookup_alias(trimmed) {
        return Some(alias.to_owned());
    }

    let mut characters = trimmed.chars();
    let first = characters.next()?;

    // A letter is typed as the letter: shift is a modifier, not a capital.
    if characters.next().is_none() && first.is_ascii_alphabetic() {
        return Some(first.to_ascii_lowercase().to_string());
    }

    if trimmed.chars().count() == 1 {
        return Some(symbol_key(first).map_or_else(|| trimmed.to_owned(), str::to_owned));
    }

    // F1 to F35: wtype and xdotool spell them with a capital F.
    let bytes = trimmed.as_bytes();

    if bytes.len() >= 2
        && (bytes[0] == b'F' || bytes[0] == b'f')
        && bytes[1].is_ascii_digit()
        && bytes[1..].iter().all(u8::is_ascii_digit)
    {
        return Some(format!("F{}", &trimmed[1..]));
    }

    // Numpad keys: "KP_1" is the keysym, "Numpad 1" what a menu says.
    if trimmed.len() > 6 && trimmed[..6].eq_ignore_ascii_case("numpad") {
        return Some(format!("KP_{}", &trimmed[6..]));
    }

    Some(trimmed.to_owned())
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn is_modifier_name(name: &str) -> Option<fn(&mut Chord)> {
    if name.eq_ignore_ascii_case("ctrl")
        || name.eq_ignore_ascii_case("control")
        || name.eq_ignore_ascii_case("primary")
    {
        return Some(|chord| chord.ctrl = true);
    }

    if name.eq_ignore_ascii_case("alt")
        || name.eq_ignore_ascii_case("meta")
        || name.eq_ignore_ascii_case("option")
    {
        return Some(|chord| chord.alt = true);
    }

    if name.eq_ignore_ascii_case("shift") {
        return Some(|chord| chord.shift = true);
    }

    if name.eq_ignore_ascii_case("super")
        || name.eq_ignore_ascii_case("win")
        || name.eq_ignore_ascii_case("cmd")
        || name.eq_ignore_ascii_case("command")
    {
        return Some(|chord| chord.super_key = true);
    }

    None
}

/// The last `+` separates the key: `Ctrl+Shift++` is Ctrl+Shift and the plus
/// key, and `Ctrl+-` is Ctrl and the minus key.
fn parse_cua(text: &str) -> Option<Chord> {
    let mut head = text;
    let key;

    if head.ends_with('+') {
        key = "+";
        head = &head[..head.len() - 1];

        // "Ctrl++": the separator before the key is the second-last plus.
        if head.ends_with('+') {
            head = &head[..head.len() - 1];
        }
    } else {
        let last = head.rfind('+')?;

        if last == 0 {
            return None;
        }

        key = &head[last + 1..];
        head = &head[..last];
    }

    let mut chord = Chord::default();

    for part in head.split('+') {
        let part = part.trim();

        if part.is_empty() {
            continue;
        }

        let set = is_modifier_name(part)?;

        set(&mut chord);
    }

    let key = canonical_key(key)?;

    Some(Chord { key, ..chord })
}

/// Emacs writes one letter and a dash per modifier, in any order.  The key is
/// whatever follows the last prefix, so `C--` is C and the minus key.
fn parse_emacs(text: &str) -> Option<Chord> {
    let mut rest = text;
    let mut chord = Chord::default();
    let mut prefixed = false;

    loop {
        let mut characters = rest.chars();
        let Some(first) = characters.next() else {
            break;
        };
        let Some(second) = characters.next() else {
            break;
        };

        if second != '-' {
            break;
        }

        match first {
            'C' => chord.ctrl = true,
            'M' => chord.alt = true,
            'S' => chord.shift = true,
            's' => chord.super_key = true,
            _ => break,
        }

        prefixed = true;
        rest = &rest[first.len_utf8() + 1..];
    }

    if !prefixed {
        return None;
    }

    chord.key = canonical_key(rest)?;

    Some(chord)
}

impl Chord {
    /// Parses Emacs notation (`C-v`, `M-F4`), CUA notation (`Ctrl+V`,
    /// `Alt+F4`) or a bare key (`F11`, `Space`).
    ///
    /// A key name it does not know is still accepted -- wtype knows more
    /// keysyms than this table does -- but a chord with no key, or a key with
    /// whitespace in it, is not.
    pub fn parse(text: &str) -> Option<Self> {
        let trimmed = text.trim();

        if trimmed.is_empty() {
            return None;
        }

        // Emacs's prefixes are unmistakable, and its key may be a plus:
        // "C-+" is Ctrl and the plus key, not a CUA chord.
        if let Some(chord) = parse_emacs(trimmed) {
            return Some(chord);
        }

        // It looked like Emacs notation and had no key behind the prefixes:
        // "C-" is a typo, not a key name.
        let bytes = trimmed.as_bytes();

        if bytes.len() >= 2 && bytes[1] == b'-' && matches!(bytes[0], b'C' | b'M' | b'S' | b's') {
            return None;
        }

        // A plus is CUA's separator.
        if trimmed.contains('+') {
            return parse_cua(trimmed);
        }

        // A bare key: "F11", "Space", "Delete".
        Some(Chord {
            key: canonical_key(trimmed)?,
            ..Chord::default()
        })
    }

    /// Reads a GTK accelerator as a menu model writes it: `<Primary>q`,
    /// `<Shift><Control>a`, `<Alt>F4`.
    pub fn parse_gtk(text: &str) -> Option<Self> {
        let mut chord = Chord::default();

        // Orbit's corpus uses a lone `<` for the less-than key.  In a GTK
        // accelerator `<` starts a modifier, but there is no closing `>` here
        // and treating it as a broken modifier would make five real menu
        // entries dead.
        if text == "<" {
            chord.key = canonical_key(text)?;

            return Some(chord);
        }

        let mut rest = text;

        while rest.starts_with('<') {
            let close = rest.find('>')?;
            let name = &rest[1..close];

            if name.eq_ignore_ascii_case("primary")
                || name.eq_ignore_ascii_case("control")
                || name.eq_ignore_ascii_case("ctrl")
            {
                chord.ctrl = true;
            } else if name.eq_ignore_ascii_case("shift") {
                chord.shift = true;
            } else if name.eq_ignore_ascii_case("alt") || name.eq_ignore_ascii_case("meta") {
                chord.alt = true;
            } else if name.eq_ignore_ascii_case("super") || name.eq_ignore_ascii_case("mod4") {
                chord.super_key = true;
            }

            rest = &rest[close + 1..];
        }

        chord.key = canonical_key(rest)?;

        Some(chord)
    }

    /// `"ctrl alt shift super"`, or `None` when nothing is held.  The order
    /// wtype and xdotool are given.
    pub fn modifiers(&self) -> Option<String> {
        let mut modifiers = Vec::new();

        if self.ctrl {
            modifiers.push("ctrl");
        }
        if self.alt {
            modifiers.push("alt");
        }
        if self.shift {
            modifiers.push("shift");
        }
        if self.super_key {
            modifiers.push("super");
        }

        if modifiers.is_empty() {
            None
        } else {
            Some(modifiers.join(" "))
        }
    }

    /// The chord as a menubar shows it: `Ctrl+Shift+P` or `C-S-p`.
    pub fn format(&self, style: ChordStyle) -> Option<String> {
        if self.key.is_empty() {
            return None;
        }

        let key = display_key(&self.key, style);
        let mut text = String::new();

        if style == ChordStyle::Emacs {
            if self.ctrl {
                text.push_str("C-");
            }
            if self.alt {
                text.push_str("M-");
            }
            if self.shift {
                text.push_str("S-");
            }
            if self.super_key {
                text.push_str("s-");
            }

            text.push_str(&key);

            return Some(text);
        }

        if self.ctrl {
            text.push_str("Ctrl+");
        }
        if self.alt {
            text.push_str("Alt+");
        }
        if self.shift {
            text.push_str("Shift+");
        }
        if self.super_key {
            text.push_str("Super+");
        }

        text.push_str(&key);

        Some(text)
    }

    /// The command that types this chord into the focused window, or `None`
    /// when there is no tool.
    pub fn command(&self, tool: &str) -> Option<String> {
        if tool.is_empty() || self.key.is_empty() {
            return None;
        }

        let modifiers = self.modifiers();

        // wtype presses and releases; xdotool is told the whole chord at once.
        if tool.ends_with("xdotool") {
            let mut typed = String::new();

            if let Some(modifiers) = &modifiers {
                typed.push_str(&modifiers.replace(' ', "+"));
                typed.push('+');
            }

            typed.push_str(&self.key);

            return Some(format!("{tool} key {typed}"));
        }

        match &modifiers {
            None => Some(format!("{tool} -k {}", self.key)),
            Some(modifiers) => Some(format!(
                "{tool} -M {modifiers} -k {} -m {modifiers}",
                self.key
            )),
        }
    }
}

impl ChordSequence {
    /// Parses one chord, or several separated by whitespace, in Emacs notation
    /// (`C-x C-s`) or CUA notation (`Ctrl+X Ctrl+S`).
    pub fn parse(text: &str) -> Option<Self> {
        parse_sequence(text, Chord::parse)
    }

    /// Parses one or more GTK accelerators separated by whitespace, each
    /// written the way a menu model writes one.
    pub fn parse_gtk(text: &str) -> Option<Self> {
        parse_sequence(text, Chord::parse_gtk)
    }

    /// The sequence as a menubar shows it: `Ctrl+X Ctrl+S` or `C-x C-s`.
    pub fn format(&self, style: ChordStyle) -> Option<String> {
        if self.chords.is_empty() {
            return None;
        }

        let formatted: Option<Vec<String>> = self
            .chords
            .iter()
            .map(|chord| chord.format(style))
            .collect();

        formatted.map(|chords| chords.join(" "))
    }

    /// One command that types every chord in order, or `None` when there is no
    /// tool or a chord has no key.
    pub fn command(&self, tool: &str) -> Option<String> {
        if tool.is_empty() || self.chords.is_empty() {
            return None;
        }

        let commands: Option<Vec<String>> = self
            .chords
            .iter()
            .map(|chord| chord.command(tool))
            .collect();

        commands.map(|commands| commands.join(" && "))
    }
}

fn parse_sequence(text: &str, parse: fn(&str) -> Option<Chord>) -> Option<ChordSequence> {
    let chords: Option<Vec<Chord>> = text.split_whitespace().map(parse).collect();

    match chords {
        Some(chords) if !chords.is_empty() => Some(ChordSequence { chords }),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Writing a chord back
// ---------------------------------------------------------------------------

fn display_key(key: &str, style: ChordStyle) -> String {
    if let Some((_, written)) = KEY_DISPLAYS.iter().find(|(name, _)| *name == key) {
        return (*written).to_owned();
    }

    let mut characters = key.chars();

    if let (Some(first), None) = (characters.next(), characters.next())
        && first.is_ascii_alphabetic()
    {
        return match style {
            ChordStyle::Cua => first.to_ascii_uppercase().to_string(),
            ChordStyle::Emacs => first.to_ascii_lowercase().to_string(),
        };
    }

    if let Some(rest) = key.strip_prefix("KP_")
        && !rest.is_empty()
    {
        return format!("Numpad {rest}");
    }

    key.to_owned()
}

/// A chord sequence in any notation this module reads, written back in
/// `style`.  Text that is not a sequence is returned unchanged: a menu entry's
/// printed chord is never worse than what was written for it.
pub fn display(text: &str, style: ChordStyle) -> String {
    if text.is_empty() {
        return String::new();
    }

    // A GTK accelerator starts with a modifier token and is one chord, not a
    // sequence.  Try it first: otherwise "<Primary>q" is an unknown bare key
    // that happens to contain no space.
    if text.starts_with('<')
        && let Some(chord) = Chord::parse_gtk(text)
        && let Some(formatted) = chord.format(style)
    {
        return formatted;
    }

    if let Some(sequence) = ChordSequence::parse(text)
        && let Some(formatted) = sequence.format(style)
    {
        return formatted;
    }

    text.to_owned()
}

#[cfg(test)]
mod tests {
    use super::{Chord, ChordStyle, canonical_key, display};

    #[test]
    fn a_single_plus_is_the_plus_key() {
        assert_eq!(canonical_key("+").as_deref(), Some("plus"));
        assert_eq!(canonical_key("<").as_deref(), Some("less"));
    }

    #[test]
    fn display_keeps_text_that_is_not_a_chord() {
        assert_eq!(display("No chord", ChordStyle::Cua), "No chord");
    }

    #[test]
    fn style_names_round_trip() {
        assert_eq!(ChordStyle::from_name(Some("emacs")), ChordStyle::Emacs);
        assert_eq!(ChordStyle::from_name(Some("Emacs")), ChordStyle::Emacs);
        assert_eq!(ChordStyle::from_name(Some("cua")), ChordStyle::Cua);
        assert_eq!(ChordStyle::from_name(None), ChordStyle::Cua);
        assert_eq!(ChordStyle::Emacs.name(), "emacs");
        assert_eq!(ChordStyle::Cua.name(), "cua");
    }

    #[test]
    fn an_empty_chord_cannot_be_typed() {
        assert!(Chord::default().format(ChordStyle::Cua).is_none());
        assert!(Chord::default().command("wtype").is_none());
    }
}
