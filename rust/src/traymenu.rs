// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The `com.canonical.dbusmenu` layout reader, ported from the DBusMenu half of
//! `util/menutree.c`.
//!
//! A tray item's menu is a tree of `(id, properties, children)` nodes.  The
//! reader returns owned data first: no widget ever holds a pointer into a
//! `GVariant`, and malformed D-Bus data is refused instead of panicking the
//! shell.  Turning an id back into a click is a separate [`TrayMenuAction`].

use std::collections::HashMap;

use gtk::glib;
use gtk::glib::VariantDict;
use gtk4 as gtk;

use crate::chord::ChordStyle;

const MAX_DEPTH: usize = 32;

/// What choosing a row sends back to the menu's owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenuAction {
    pub service: String,
    pub path: String,
    pub id: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayMenuEntry {
    Separator,
    Item {
        label: String,
        enabled: bool,
        accel: Option<String>,
        action: TrayMenuAction,
        submenu: Vec<TrayMenuEntry>,
    },
}

/// Reads a context menu from a `GetLayout` reply.
///
/// A tray item's root is the context menu itself, so its children are the
/// rows.  An unknown or malformed reply is an empty menu, not an error the
/// caller has to carry into the UI.
pub fn parse_layout(
    reply: &glib::Variant,
    service: &str,
    path: &str,
    style: ChordStyle,
) -> Vec<TrayMenuEntry> {
    let Some(layout) = layout_node(reply) else {
        return Vec::new();
    };
    let Some((_, _, children)) = node_parts(&layout) else {
        return Vec::new();
    };

    parse_children(&children, service, path, style, 0)
}

fn layout_node(reply: &glib::Variant) -> Option<glib::Variant> {
    match reply.type_().as_str() {
        "(u(ia{sv}av))" if reply.n_children() == 2 => reply.try_child_value(1),
        "(ia{sv}av)" => Some(reply.clone()),
        _ => None,
    }
}

fn parse_children(
    children: &glib::Variant,
    service: &str,
    path: &str,
    style: ChordStyle,
    depth: usize,
) -> Vec<TrayMenuEntry> {
    if depth > MAX_DEPTH || children.type_().as_str() != "av" {
        return Vec::new();
    }

    let mut entries = Vec::new();

    for index in 0..children.n_children() {
        let Some(boxed) = children.try_child_value(index) else {
            continue;
        };
        let Some(node) = boxed.as_variant() else {
            continue;
        };
        let Some((id, props, kids)) = node_parts(&node) else {
            continue;
        };
        let Some(dict) = properties(&props) else {
            continue;
        };

        // An invisible node and its whole subtree are absent from the menu.
        if !dict
            .lookup::<bool>("visible")
            .ok()
            .flatten()
            .unwrap_or(true)
        {
            continue;
        }

        let kind = dict.lookup::<String>("type").ok().flatten();

        if kind.as_deref() == Some("separator") {
            entries.push(TrayMenuEntry::Separator);
            continue;
        }

        let label = dict
            .lookup::<String>("label")
            .ok()
            .flatten()
            .unwrap_or_default();
        let enabled = dict
            .lookup::<bool>("enabled")
            .ok()
            .flatten()
            .unwrap_or(true);
        let accel = shortcut_display(
            &dict
                .lookup::<Vec<Vec<String>>>("shortcut")
                .ok()
                .flatten()
                .unwrap_or_default(),
            style,
        );
        let submenu = parse_children(&kids, service, path, style, depth + 1);

        entries.push(TrayMenuEntry::Item {
            label,
            enabled,
            accel,
            action: TrayMenuAction {
                service: service.to_owned(),
                path: path.to_owned(),
                id,
            },
            submenu,
        });
    }

    entries
}

fn node_parts(node: &glib::Variant) -> Option<(i32, glib::Variant, glib::Variant)> {
    let node_type = glib::VariantTy::new("(ia{sv}av)").ok()?;

    if !node.is_type(node_type) || node.n_children() != 3 {
        return None;
    }

    Some((
        node.try_child_value(0)?.get::<i32>()?,
        node.try_child_value(1)?,
        node.try_child_value(2)?,
    ))
}

fn properties(props: &glib::Variant) -> Option<VariantDict> {
    let props_type = glib::VariantTy::new("a{sv}").ok()?;

    props
        .is_type(props_type)
        .then(|| VariantDict::new(Some(props)))
}

/// DBusMenu writes shortcut keys as X keysyms (`Control`, `Mod4`).  A row
/// shows them in the configured notation, not as protocol names.
fn shortcut_display(chords: &[Vec<String>], style: ChordStyle) -> Option<String> {
    let mut written = Vec::with_capacity(chords.len());

    for chord in chords {
        if chord.is_empty() {
            continue;
        }

        let keys: Vec<String> = chord.iter().map(|key| shortcut_part(key, style)).collect();

        written.push(keys.join("+"));
    }

    (!written.is_empty()).then(|| written.join(" "))
}

fn shortcut_part(name: &str, style: ChordStyle) -> String {
    let cua = style == ChordStyle::Cua;

    match name {
        "Control" | "Ctrl" => if cua { "Ctrl" } else { "C" }.to_owned(),
        "Shift" => if cua { "Shift" } else { "S" }.to_owned(),
        "Alt" | "Mod1" => if cua { "Alt" } else { "A" }.to_owned(),
        "Super" | "Meta" | "Mod4" => if cua { "Super" } else { "s" }.to_owned(),
        other => other.to_owned(),
    }
}

/// A compact constructor used by tests and by callers that want to build a
/// tree without a display.
pub fn item(label: &str, id: i32, service: &str, path: &str) -> TrayMenuEntry {
    TrayMenuEntry::Item {
        label: label.to_owned(),
        enabled: true,
        accel: None,
        action: TrayMenuAction {
            service: service.to_owned(),
            path: path.to_owned(),
            id,
        },
        submenu: Vec::new(),
    }
}

/// The variable map a `VariantDict` reads.  Kept public for tests that build a
/// layout by hand; the UI never needs it.
pub fn properties_map(entries: &[(&str, glib::Variant)]) -> HashMap<String, glib::Variant> {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::glib::prelude::ToVariant;

    fn node(
        id: i32,
        props: &[(&str, glib::Variant)],
        children: Vec<glib::Variant>,
    ) -> glib::Variant {
        (id, properties_map(props), children).to_variant()
    }

    fn child(id: i32, props: &[(&str, glib::Variant)]) -> glib::Variant {
        node(id, props, Vec::new())
    }

    fn layout(children: Vec<glib::Variant>) -> glib::Variant {
        (0i32, properties_map(&[]), children).to_variant()
    }

    fn text(value: &str) -> glib::Variant {
        value.to_variant()
    }

    #[test]
    fn rows_carry_labels_ids_and_actions() {
        let tree = layout(vec![
            child(
                4,
                &[("label", text("Open")), ("enabled", false.to_variant())],
            ),
            child(5, &[("type", text("separator"))]),
            child(6, &[("label", text("Quit"))]),
        ]);
        let rows = parse_layout(&tree, "org.example", "/MenuBar", ChordStyle::Cua);

        assert_eq!(rows.len(), 3);
        assert!(matches!(
            &rows[0],
            TrayMenuEntry::Item {
                label,
                enabled: false,
                action,
                ..
            } if label == "Open"
                && action.id == 4
                && action.service == "org.example"
                && action.path == "/MenuBar"
        ));
        assert_eq!(rows[1], TrayMenuEntry::Separator);
        assert!(matches!(
            &rows[2],
            TrayMenuEntry::Item { enabled: true, .. }
        ));
    }

    #[test]
    fn invisible_nodes_and_their_subtrees_are_skipped() {
        let hidden = node(
            2,
            &[("label", text("Hidden")), ("visible", false.to_variant())],
            vec![child(3, &[("label", text("Child"))])],
        );
        let tree = layout(vec![hidden, child(4, &[("label", text("Shown"))])]);
        let rows = parse_layout(&tree, "app", "/Menu", ChordStyle::Cua);

        assert_eq!(rows.len(), 1);
        assert!(matches!(&rows[0], TrayMenuEntry::Item { label, .. } if label == "Shown"));
    }

    #[test]
    fn submenus_are_owned_trees() {
        let parent = node(
            7,
            &[("label", text("More"))],
            vec![child(8, &[("label", text("Inner"))])],
        );
        let tree = layout(vec![parent]);

        let rows = parse_layout(&tree, "app", "/Menu", ChordStyle::Cua);

        assert!(matches!(
            &rows[0],
            TrayMenuEntry::Item { submenu, .. }
                if submenu.len() == 1
                    && matches!(&submenu[0], TrayMenuEntry::Item { label, .. } if label == "Inner")
        ));
    }

    #[test]
    fn shortcuts_are_rendered_in_the_requested_style() {
        let shortcut = vec![
            vec!["Control".to_owned(), "Shift".to_owned(), "p".to_owned()],
            vec!["Mod4".to_owned(), "q".to_owned()],
        ];
        let tree = layout(vec![child(
            9,
            &[
                ("label", text("Do it")),
                ("shortcut", shortcut.to_variant()),
            ],
        )]);

        let cua = parse_layout(&tree, "app", "/Menu", ChordStyle::Cua);
        let emacs = parse_layout(&tree, "app", "/Menu", ChordStyle::Emacs);

        assert!(
            matches!(&cua[0], TrayMenuEntry::Item { accel: Some(accel), .. } if accel == "Ctrl+Shift+p Super+q")
        );
        assert!(
            matches!(&emacs[0], TrayMenuEntry::Item { accel: Some(accel), .. } if accel == "C+S+p s+q"),
            "Emacs row was {:?}",
            emacs[0]
        );
    }

    #[test]
    fn a_wrapped_get_layout_reply_is_unwrapped() {
        let reply = (
            9u32,
            (
                0i32,
                properties_map(&[]),
                vec![child(1, &[("label", text("One"))])],
            ),
        )
            .to_variant();

        let rows = parse_layout(&reply, "app", "/Menu", ChordStyle::Cua);

        assert_eq!(rows, vec![item("One", 1, "app", "/Menu")]);
    }

    #[test]
    fn malformed_data_is_an_empty_menu() {
        assert!(parse_layout(&42i32.to_variant(), "app", "/Menu", ChordStyle::Cua).is_empty());
        assert!(
            parse_layout(
                &(0i32, 1i32, Vec::<glib::Variant>::new()).to_variant(),
                "app",
                "/Menu",
                ChordStyle::Cua
            )
            .is_empty()
        );
        assert!(
            parse_layout(
                &layout(vec![1i32.to_variant()]),
                "app",
                "/Menu",
                ChordStyle::Cua
            )
            .is_empty()
        );
    }

    #[test]
    fn omitted_properties_take_the_protocol_defaults() {
        let tree = layout(vec![child(11, &[])]);
        let rows = parse_layout(&tree, "app", "/Menu", ChordStyle::Cua);

        assert!(matches!(
            &rows[0],
            TrayMenuEntry::Item {
                label,
                enabled: true,
                accel: None,
                ..
            } if label.is_empty()
        ));
    }
}
