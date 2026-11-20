// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The `character` bar module: a card of characters, one click to the
//! clipboard.
//!
//! Typing an `é`, a `ß` or a shrug on a layout that has no key for it is a
//! chord nobody remembers.  This reading is the way round it: a click opens a
//! card of four sections -- accented letters from many languages, emoji, ASCII
//! smiles and kaomoji -- and a character on the card is on the clipboard the
//! moment it is clicked.  Nothing is inserted and no window is touched: the
//! clipboard is handed back to the reader, who pastes where they were.
//!
//! A configuration names its own commands with `#:on-click` and
//! `#:on-click-right`; when it does, the card is not attached and the readings
//! stay the module host's.

use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::{BarSettings, ModuleSpec};
use crate::barfactory::ModuleContext;
use crate::barmodule::{self, ModuleHost};
use crate::clipboard;
use crate::style;

pub const DEFAULT_INTERVAL_SECONDS: u32 = 0;
const DEFAULT_ICON: &str = "☺";
const DEFAULT_FORMAT: &str = "{icon}";
const DEFAULT_TOOLTIP: &str = "Characters: letters, emoji, smiles\nClick: the picker";
const PICKER_MAX_WIDTH: i32 = 452;
const PICKER_MAX_HEIGHT: i32 = 420;
const CHILDREN_PER_LINE: u32 = 10;

const PICKER_CSS: &str = "
.whelk-character-popover {
  padding: 6px;
}
.whelk-character-card {
  padding: 2px;
}
.whelk-character-heading {
  padding: 6px 4px 2px 4px;
  font-size: 0.82em;
  font-weight: 600;
  opacity: 0.62;
}
.whelk-character-char {
  padding: 1px 4px;
  min-width: 0;
  min-height: 0;
  font-size: 1.1em;
  border-radius: 4px;
}
";

/// One group of characters on the card.
pub struct Section {
    pub id: &'static str,
    pub title: &'static str,
    pub characters: &'static [&'static str],
}

/// The letters a layout with a few dead keys cannot produce, from the Latin
/// scripts, plus the ligatures and letters that are words of their own.
const ACCENTS: &[&str] = &[
    "à", "á", "â", "ã", "ä", "å", "ā", "ă", "ą", "æ", "ç", "ć", "č", "ĉ", "ċ", "ď", "đ", "ð", "è",
    "é", "ê", "ë", "ē", "ĕ", "ė", "ę", "ě", "ĝ", "ğ", "ġ", "ģ", "ĥ", "ħ", "ì", "í", "î", "ï", "ĩ",
    "ī", "ĭ", "į", "ı", "ĵ", "ķ", "ĺ", "ļ", "ľ", "ł", "ñ", "ń", "ň", "ņ", "ō", "ó", "ô", "õ", "ö",
    "ø", "ŏ", "ő", "œ", "ŕ", "ř", "ś", "š", "ş", "ß", "ť", "ţ", "ŧ", "ù", "ú", "û", "ü", "ũ", "ū",
    "ŭ", "ů", "ű", "ų", "ŵ", "ý", "ÿ", "ź", "ž", "ż", "þ", "À", "Á", "Â", "Ã", "Ä", "Å", "Ā", "Ă",
    "Ą", "Æ", "Ç", "Ć", "Č", "Ď", "Đ", "Ð", "È", "É", "Ê", "Ë", "Ē", "Ė", "Ę", "Ě", "Ğ", "Ģ", "Ì",
    "Í", "Î", "Ï", "Ĩ", "Ī", "Į", "İ", "Ķ", "Ĺ", "Ļ", "Ľ", "Ł", "Ñ", "Ń", "Ņ", "Ň", "Ò", "Ó", "Ô",
    "Õ", "Ö", "Ø", "Ō", "Ő", "Œ", "Ŕ", "Ř", "Ś", "Š", "Ş", "Ť", "Ţ", "Ŧ", "Ù", "Ú", "Û", "Ü", "Ũ",
    "Ū", "Ů", "Ű", "Ų", "Ŵ", "Ý", "Ÿ", "Ź", "Ž", "Ż", "Þ",
];

/// The ones that carry a conversation, not the whole table: a picker is a
/// card, not a keyboard.
const EMOJI: &[&str] = &[
    "😀", "😃", "😄", "😁", "😆", "😅", "😂", "🤣", "😊", "😇", "🙂", "😉", "😍", "🥰", "😘", "😋",
    "😛", "😜", "🤪", "😝", "🤑", "🤗", "🤭", "🤔", "🤐", "😐", "😑", "😶", "😏", "😒", "🙄", "😬",
    "😌", "😔", "😪", "😴", "😷", "🤒", "🤕", "🤢", "🤮", "🤧", "🥵", "🥶", "😵", "🤯", "🤠", "🥳",
    "😎", "🤓", "🧐", "😕", "😟", "🙁", "😮", "😯", "😲", "😳", "🥺", "😦", "😧", "😨", "😰", "😥",
    "😢", "😭", "😱", "😖", "😣", "😞", "😓", "😩", "😫", "😤", "😡", "🤬", "😈", "👿", "💀", "💩",
    "👻", "🙈", "🙉", "🙊", "💋", "💘", "💝", "💖", "💗", "💓", "💞", "💕", "💯", "🔥", "✨", "⭐",
    "🌟", "💫", "✅", "❌", "⚠️", "❗", "❓", "👍", "👎", "👌", "✌️", "🤞", "👋", "🙏", "💪", "🙌",
    "🎉", "🎊", "🎁", "☕", "🍎", "🍕", "🍻", "🚀", "🌙", "☀️", "🌈", "🐱", "🐶", "🐼", "🦊", "🌸",
    "🌻", "🌹", "🍀",
];

/// Typed on a keyboard with no input method: still readable twenty years on.
const SMILES: &[&str] = &[
    ":-)", ":)", ":]", "=)", ":-D", ":D", ";-)", ";D", ":-P", ":P", ":p", ":-/", ":/", ":-(", ":(",
    ":'(", ":-O", ":O", ":-|", ":|", ":-*", ":*", ":^)", "^_^", "^^", "-_-", "T_T", "o_O", "O_o",
    "8)", "B)", ">:(", "<3", "</3", ":3", "=D", "=P", ":S", ":?",
];

/// The same faces, drawn rather than typed, which is what makes them cute.
const KAWAII: &[&str] = &[
    "(◕‿◕)",
    "(｡♥‿♥｡)",
    "(´｡• ᵕ •｡`)",
    "(๑˃ᴗ˂)ﻭ",
    "ʕ•ᴥ•ʔ",
    "ʕっ•ᴥ•ʔっ",
    "(っ◔◡◔)っ",
    "( ˘ ³˘)♥",
    "uwu",
    "owo",
    "UwU",
    "OwO",
    ">w<",
    "^w^",
    "(・ω・)",
    "(=^･ω･^=)",
    "ヾ(≧▽≦*)o",
    "ヽ(•‿•)ノ",
    "(╯°□°)╯︵ ┻━┻",
    "( ͡° ͜ʖ ͡°)",
    "(*^▽^*)",
    "(づ｡◕‿‿◕｡)づ",
    "\\(^o^)/",
    "＼(^o^)／",
    "nya~",
    "=^..^=",
    "♡",
    "(๑•̀ㅂ•́)و✧",
];

/// The card's sections, in the order they are drawn.
pub const SECTIONS: &[Section] = &[
    Section {
        id: "accents",
        title: "Accented letters",
        characters: ACCENTS,
    },
    Section {
        id: "emoji",
        title: "Emoji",
        characters: EMOJI,
    },
    Section {
        id: "smiles",
        title: "ASCII smiles",
        characters: SMILES,
    },
    Section {
        id: "kawaii",
        title: "Kawaii",
        characters: KAWAII,
    },
];

pub fn sections() -> &'static [Section] {
    SECTIONS
}

pub fn section(id: &str) -> Option<&'static Section> {
    SECTIONS.iter().find(|section| section.id == id)
}

/// The sections `#:sections` names, in the catalogue's order.  A list that
/// names nothing known (or names nothing at all) shows every section rather
/// than an empty card.
pub fn chosen_sections(list: Option<&str>) -> Vec<&'static Section> {
    let everything = || SECTIONS.iter().collect::<Vec<_>>();
    let Some(list) = list.filter(|list| !list.trim().is_empty()) else {
        return everything();
    };
    let wanted = list
        .split(',')
        .map(|id| id.trim().to_ascii_lowercase())
        .filter(|id| !id.is_empty())
        .collect::<Vec<String>>();
    let chosen = SECTIONS
        .iter()
        .filter(|section| wanted.iter().any(|id| id == section.id))
        .collect::<Vec<_>>();

    if chosen.is_empty() {
        everything()
    } else {
        chosen
    }
}

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let module = module.clone();
    let (root, host) = ModuleHost::new(
        &module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    host.add_label();
    host.present_default(DEFAULT_FORMAT, DEFAULT_ICON, DEFAULT_TOOLTIP, &[]);

    if host.settings().actions.primary.is_none() {
        let sections = chosen_sections(module.text("sections").as_deref());

        attach_picker(&root, &context.bar, sections);
    }

    Some(root)
}

/// A click opens the card; a character on it is copied and the card closes, so
/// one click puts a character on the clipboard and the next paste has it.
fn attach_picker(root: &gtk::Box, bar: &BarSettings, sections: Vec<&'static Section>) {
    let popover = gtk::Popover::new();
    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let scroller = gtk::ScrolledWindow::new();

    popover.set_has_arrow(false);
    popover.add_css_class("whelk-character-popover");
    card.add_css_class("whelk-character-card");

    for section in sections {
        card.append(&section_card(section, &popover));
    }

    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_max_content_width(PICKER_MAX_WIDTH);
    scroller.set_max_content_height(PICKER_MAX_HEIGHT);
    scroller.set_propagate_natural_width(true);
    scroller.set_propagate_natural_height(true);
    scroller.set_child(Some(&card));
    popover.set_child(Some(&scroller));
    popover.set_parent(root);
    barmodule::watch_popover(&popover);
    attach_escape(&popover);

    let popover_weak = popover.downgrade();

    root.connect_destroy(move |_| {
        if let Some(popover) = popover_weak.upgrade() {
            popover.popdown();
            popover.unparent();
        }
    });

    if let Some(provider) = install_css(bar) {
        root.connect_destroy(move |_| {
            if let Some(display) = gtk::gdk::Display::default() {
                gtk::style_context_remove_provider_for_display(&display, &provider);
            }
        });
    }

    let gesture = gtk::GestureClick::new();
    let popover_clone = popover.clone();

    gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
    gesture.connect_pressed(move |gesture, _, _, _| {
        gesture.set_state(gtk::EventSequenceState::Claimed);
        barmodule::close_popovers(Some(&popover_clone));
        popover_clone.popup();
    });
    root.add_controller(gesture);
    root.set_cursor_from_name(Some("pointer"));
}

/// One heading and one flow of characters.  The flow wraps with the card, so a
/// narrow bar gets fewer characters per line rather than a wider card.
fn section_card(section: &'static Section, popover: &gtk::Popover) -> gtk::Widget {
    let block = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let heading = gtk::Label::new(Some(section.title));
    let flow = gtk::FlowBox::new();

    block.add_css_class("whelk-character-section");
    heading.add_css_class("whelk-character-heading");
    heading.set_halign(gtk::Align::Start);
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_max_children_per_line(CHILDREN_PER_LINE);
    flow.set_min_children_per_line(4);
    flow.set_column_spacing(2);
    flow.set_row_spacing(2);
    flow.set_homogeneous(true);

    for character in section.characters {
        flow.append(&character_button(character, popover));
    }

    block.append(&heading);
    block.append(&flow);
    block.upcast()
}

fn character_button(character: &'static str, popover: &gtk::Popover) -> gtk::Button {
    let button = gtk::Button::with_label(character);
    let popover = popover.downgrade();

    button.add_css_class("flat");
    button.add_css_class("whelk-character-char");
    button.set_tooltip_text(Some(&format!("Copy {character}")));
    button.connect_clicked(move |_| {
        if clipboard::copy_text(character)
            && let Some(popover) = popover.upgrade()
        {
            popover.popdown();
        }
    });

    button
}

/// Escape closes the card, whichever button has the focus: the reader who
/// opened it by accident does not have to find the pointer again.
fn attach_escape(popover: &gtk::Popover) {
    let keys = gtk::EventControllerKey::new();
    let weak = popover.downgrade();

    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(move |_, key, _, _| {
        if key != gtk::gdk::Key::Escape {
            return gtk::glib::Propagation::Proceed;
        }

        if let Some(popover) = weak.upgrade() {
            popover.popdown();
        }

        gtk::glib::Propagation::Stop
    });
    popover.add_controller(keys);
}

fn install_css(bar: &BarSettings) -> Option<gtk::CssProvider> {
    let display = gtk::gdk::Display::default()?;
    let mut css = PICKER_CSS.to_owned();

    if let Some(foreground) = bar
        .foreground
        .as_deref()
        .filter(|foreground| !foreground.is_empty())
    {
        css.push_str(&format!(
            ".whelk-character-popover {{ color: {foreground}; }}\n"
        ));
    }

    if let Some(background) = style::solid(bar.background.as_deref()) {
        css.push_str(&format!(
            ".whelk-character-popover {{ background-color: {background}; }}\n"
        ));
    }

    let provider = gtk::CssProvider::new();

    provider.load_from_string(&css);
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    Some(provider)
}
