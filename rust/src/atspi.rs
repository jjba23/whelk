// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The global menu's AT-SPI producer, ported from `util/atspimenus.c`.
//!
//! AT-SPI is not a menu protocol.  GTK, Qt and Electron expose their menus as
//! accessible objects: a `MENU_BAR` whose children are `MENU`s, whose children
//! are `MENU_ITEM`s with names, state and key bindings.  Reading that tree is
//! the last fallback before the dictionary and Orbit corpus, and the only
//! producer that normally returns the application's translated labels.
//!
//! The accessibility bus is slow, so the walk runs on a worker thread and
//! delivers owned [`Menu`] values to the main loop.  The worker also follows
//! `org.a11y.atspi.Event.Object` changes and re-reads after a short debounce.
//! No GTK type crosses the thread boundary.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use gtk::gio;
use gtk::glib;
use gtk::glib::prelude::*;
use gtk4 as gtk;

use crate::chord::{self, ChordStyle};
use crate::menubar::{Menu, MenuAction, MenuEntry};

pub const ATSPI_TIMEOUT_MS: i32 = 250;
pub const MAX_DEPTH: usize = 9;
pub const MAX_NODES: usize = 600;
pub const MAX_CHILDREN: usize = 120;
const CHANGE_DELAY: Duration = Duration::from_millis(150);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

const ROOT_SERVICE: &str = "org.a11y.atspi.Registry";
const ROOT_PATH: &str = "/org/a11y/atspi/accessible/root";
const REGISTRY_PATH: &str = "/org/a11y/atspi/registry";
const EVENT_OBJECT: &str = "org.a11y.atspi.Event.Object";
const EVENT_REGISTRATION: &str = "Object:";

// The wire values from atspi-constants.h.  Keeping libatspi out of the build
// leaves the bus as the API.
const ROLE_MENU: u32 = 33;
const ROLE_MENU_BAR: u32 = 34;
const ROLE_MENU_ITEM: u32 = 35;
const ROLE_CHECK_MENU_ITEM: u32 = 8;
const ROLE_RADIO_MENU_ITEM: u32 = 45;
const ROLE_SEPARATOR: u32 = 50;
const STATE_ENABLED: u32 = 8;
const STATE_SENSITIVE: u32 = 24;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeRef {
    pub service: String,
    pub path: String,
}

impl NodeRef {
    fn new(service: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            path: path.into(),
        }
    }
}

/// The bus operations the tree walk needs.  The trait is what lets the walk be
/// tested without an accessibility bus; production uses [`AtspiBus`].
trait AccessibleTree {
    fn role(&self, node: &NodeRef) -> Option<u32>;
    fn name(&self, node: &NodeRef) -> Option<String>;
    fn children(&self, node: &NodeRef) -> Vec<NodeRef>;
    fn states(&self, node: &NodeRef) -> Option<Vec<u32>>;
    fn key_binding(&self, node: &NodeRef) -> Option<String>;
    fn peer_pid(&self, candidate: &NodeRef) -> i32;

    fn state(&self, node: &NodeRef, bit: u32) -> bool {
        let Some(words) = self.states(node) else {
            return true; // no state to say otherwise
        };
        let word = (bit / 32) as usize;

        words
            .get(word)
            .is_some_and(|value| value & (1u32 << (bit % 32)) != 0)
    }
}

/// The real accessibility bus.  `GetConnectionUnixProcessID` must be asked on
/// the same bus that owns the accessibility name.
struct AtspiBus {
    a11y: gio::DBusConnection,
}

impl AtspiBus {
    fn connect() -> Option<Self> {
        let session = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).ok()?;
        let address_type = glib::VariantTy::new("(s)").ok()?;
        let reply = session
            .call_sync(
                Some("org.a11y.Bus"),
                "/org/a11y/bus",
                "org.a11y.Bus",
                "GetAddress",
                None,
                Some(address_type),
                gio::DBusCallFlags::NO_AUTO_START,
                ATSPI_TIMEOUT_MS,
                None::<&gio::Cancellable>,
            )
            .ok()?;
        let address = reply.child_value(0).get::<String>()?;

        if address.is_empty() {
            return None;
        }

        let a11y = gio::DBusConnection::for_address_sync(
            &address,
            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
            None,
            None::<&gio::Cancellable>,
        )
        .ok()?;

        Some(Self { a11y })
    }

    fn call(
        &self,
        node: &NodeRef,
        interface: &str,
        method: &str,
        parameters: Option<&glib::Variant>,
        reply_type: &str,
    ) -> Option<glib::Variant> {
        let reply_type = glib::VariantTy::new(reply_type).ok()?;

        self.a11y
            .call_sync(
                Some(&node.service),
                &node.path,
                interface,
                method,
                parameters,
                Some(reply_type),
                gio::DBusCallFlags::NO_AUTO_START,
                ATSPI_TIMEOUT_MS,
                None::<&gio::Cancellable>,
            )
            .ok()
    }
}

impl AccessibleTree for AtspiBus {
    fn role(&self, node: &NodeRef) -> Option<u32> {
        let reply = self.call(node, "org.a11y.atspi.Accessible", "GetRole", None, "(u)")?;

        reply.child_value(0).get::<u32>()
    }

    fn name(&self, node: &NodeRef) -> Option<String> {
        let reply = self.call(
            node,
            "org.freedesktop.DBus.Properties",
            "Get",
            Some(&("org.a11y.atspi.Accessible", "Name").to_variant()),
            "(v)",
        )?;
        let value = reply.child_value(0).get::<glib::Variant>()?;

        value.get::<String>()
    }

    fn children(&self, node: &NodeRef) -> Vec<NodeRef> {
        let Some(reply) = self.call(
            node,
            "org.a11y.atspi.Accessible",
            "GetChildren",
            None,
            "(a(so))",
        ) else {
            return Vec::new();
        };
        let children = reply.child_value(0);
        let count = children.n_children().min(MAX_CHILDREN);
        let mut result = Vec::new();

        for index in 0..count {
            let Some(entry) = children.try_child_value(index) else {
                continue;
            };
            let Some(service) = entry
                .try_child_value(0)
                .and_then(|value| value.get::<String>())
            else {
                continue;
            };
            let Some(path_value) = entry.try_child_value(1) else {
                continue;
            };
            let Some(path) = path_value.get::<String>().or_else(|| {
                path_value
                    .get::<glib::variant::ObjectPath>()
                    .map(String::from)
            }) else {
                continue;
            };

            if !service.is_empty() && glib::Variant::is_object_path(&path) {
                result.push(NodeRef::new(service, path));
            }
        }

        result
    }

    fn states(&self, node: &NodeRef) -> Option<Vec<u32>> {
        let reply = self.call(node, "org.a11y.atspi.Accessible", "GetState", None, "(au)")?;
        let words = reply.child_value(0);
        let count = words.n_children().min(2);
        let mut result = Vec::with_capacity(count);

        for index in 0..count {
            result.push(words.child_value(index).get::<u32>()?);
        }

        Some(result)
    }

    fn key_binding(&self, node: &NodeRef) -> Option<String> {
        let reply = self.call(
            node,
            "org.a11y.atspi.Action",
            "GetKeyBinding",
            Some(&(0i32,).to_variant()),
            "(s)",
        )?;
        let binding = reply.child_value(0).get::<String>()?;

        (!binding.is_empty()).then_some(binding)
    }

    fn peer_pid(&self, candidate: &NodeRef) -> i32 {
        let reply_type = glib::VariantTy::new("(u)").ok();

        self.a11y
            .call_sync(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "GetConnectionUnixProcessID",
                Some(&(candidate.service.as_str(),).to_variant()),
                reply_type,
                gio::DBusCallFlags::NO_AUTO_START,
                ATSPI_TIMEOUT_MS,
                None::<&gio::Cancellable>,
            )
            .ok()
            .and_then(|reply| reply.child_value(0).get::<u32>())
            .map_or(0, |pid| pid as i32)
    }
}

// ---------------------------------------------------------------------------
// The tree
// ---------------------------------------------------------------------------

fn names_match(candidate: Option<&str>, wanted: &str) -> bool {
    let Some(candidate) = candidate.filter(|candidate| !candidate.is_empty()) else {
        return false;
    };

    if wanted.is_empty() {
        return false;
    }

    if candidate.eq_ignore_ascii_case(wanted) {
        return true;
    }

    // A desktop entry says "Firefox"; some toolkits say "Mozilla Firefox".
    candidate
        .to_ascii_lowercase()
        .contains(&wanted.to_ascii_lowercase())
        || wanted
            .to_ascii_lowercase()
            .contains(&candidate.to_ascii_lowercase())
}

fn application(tree: &dyn AccessibleTree, app_name: &str, pid: i32) -> Option<NodeRef> {
    let root = NodeRef::new(ROOT_SERVICE, ROOT_PATH);
    let children = tree.children(&root);
    let mut by_name = None;
    let mut chosen = None;

    for candidate in &children {
        if pid > 0 && tree.peer_pid(candidate) == pid {
            chosen = Some(candidate.clone());
            break;
        }

        if by_name.is_none()
            && !app_name.is_empty()
            && names_match(tree.name(candidate).as_deref(), app_name)
        {
            by_name = Some(candidate.clone());
        }
    }

    chosen.or(by_name).or_else(|| {
        (pid <= 0 && app_name.is_empty())
            .then(|| children.first().cloned())
            .flatten()
    })
}

fn find_menu_bar(
    tree: &dyn AccessibleTree,
    node: &NodeRef,
    depth: usize,
    budget: &mut usize,
) -> Option<NodeRef> {
    if *budget == 0 || depth > MAX_DEPTH {
        return None;
    }

    *budget -= 1;

    if tree.role(node) == Some(ROLE_MENU_BAR) {
        return Some(node.clone());
    }

    if depth == MAX_DEPTH {
        return None;
    }

    for child in tree.children(node) {
        if let Some(found) = find_menu_bar(tree, &child, depth + 1, budget) {
            return Some(found);
        }

        if *budget == 0 {
            break;
        }
    }

    None
}

fn append_item(
    tree: &dyn AccessibleTree,
    menu: &mut Vec<MenuEntry>,
    item: &NodeRef,
    depth: usize,
    budget: &mut usize,
    style: ChordStyle,
) {
    let Some(label) = tree.name(item).filter(|label| !label.is_empty()) else {
        return;
    };
    let enabled = tree.state(item, STATE_ENABLED) || tree.state(item, STATE_SENSITIVE);
    let accel = tree
        .key_binding(item)
        .map(|binding| chord::display(&binding, style))
        .filter(|binding| !binding.is_empty());
    let mut submenu = Vec::new();

    if depth < MAX_DEPTH && *budget > 0 {
        for child in tree.children(item) {
            if tree.role(&child) == Some(ROLE_MENU) {
                append_items(tree, &mut submenu, &child, depth + 1, budget, style);
                break;
            }
        }
    }

    menu.push(MenuEntry::Item {
        label,
        enabled,
        accel,
        action: Some(MenuAction::Atspi {
            service: item.service.clone(),
            path: item.path.clone(),
            action: 0,
        }),
        submenu,
    });
}

fn append_items(
    tree: &dyn AccessibleTree,
    menu: &mut Vec<MenuEntry>,
    container: &NodeRef,
    depth: usize,
    budget: &mut usize,
    style: ChordStyle,
) {
    if *budget == 0 || depth > MAX_DEPTH {
        return;
    }

    for child in tree.children(container) {
        if *budget == 0 {
            break;
        }

        *budget -= 1;

        match tree.role(&child) {
            Some(ROLE_SEPARATOR) => {
                if !menu.is_empty() && !matches!(menu.last(), Some(MenuEntry::Separator)) {
                    menu.push(MenuEntry::Separator);
                }
            }
            Some(ROLE_MENU) => {
                append_items(tree, menu, &child, depth + 1, budget, style);
            }
            Some(ROLE_MENU_ITEM | ROLE_CHECK_MENU_ITEM | ROLE_RADIO_MENU_ITEM) => {
                append_item(tree, menu, &child, depth + 1, budget, style);
            }
            _ => {}
        }
    }
}

fn menubar_for_application(
    tree: &dyn AccessibleTree,
    application: &NodeRef,
    style: ChordStyle,
) -> Vec<Menu> {
    let mut budget = MAX_NODES;
    let Some(menu_bar) = find_menu_bar(tree, application, 0, &mut budget) else {
        return Vec::new();
    };
    let mut bar = Vec::new();

    for top in tree.children(&menu_bar) {
        let Some(label) = tree.name(&top).filter(|label| !label.is_empty()) else {
            continue;
        };
        let mut entries = Vec::new();

        if tree.role(&top) == Some(ROLE_MENU) {
            append_items(tree, &mut entries, &top, 1, &mut budget, style);
        } else {
            append_item(tree, &mut entries, &top, 1, &mut budget, style);
        }

        while matches!(entries.last(), Some(MenuEntry::Separator)) {
            entries.pop();
        }

        if MenuEntry::has_items(&entries) {
            bar.push(Menu::with_entries(label, entries));
        }
    }

    bar
}

fn read_menus(tree: &dyn AccessibleTree, app_name: &str, pid: i32, style: ChordStyle) -> Vec<Menu> {
    let Some(application) = application(tree, app_name, pid) else {
        return Vec::new();
    };

    menubar_for_application(tree, &application, style)
}

/// One synchronous accessibility read.  The UI uses [`Watch`] instead; this
/// entry point keeps the bus protocol testable without a display server.
pub fn read_menus_once(app_name: &str, pid: i32, style: ChordStyle) -> Vec<Menu> {
    let Some(tree) = AtspiBus::connect() else {
        return Vec::new();
    };

    read_menus(&tree, app_name, pid, style)
}

// ---------------------------------------------------------------------------
// The live watch
// ---------------------------------------------------------------------------

/// A running accessibility watch.  Dropping it stops the worker and the main
/// loop poll that delivers its menus.
pub struct Watch {
    cancel: Arc<AtomicBool>,
    /// The poll source owns the receiver.  Removing it closes the channel, so
    /// a worker blocked in a bus call exits on its next send.
    source: Option<glib::SourceId>,
}

impl Watch {
    pub fn start(
        app_name: String,
        pid: i32,
        style: ChordStyle,
        mut changed: impl FnMut(Vec<Menu>) + 'static,
    ) -> Option<Self> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);

        std::thread::Builder::new()
            .name("whelk-atspi-watch".to_owned())
            .spawn(move || watch_thread(app_name, pid, style, sender, worker_cancel))
            .ok()?;

        let source = glib::timeout_add_local(POLL_INTERVAL, move || {
            loop {
                match receiver.try_recv() {
                    Ok(menus) => changed(menus),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return glib::ControlFlow::Break,
                }
            }

            glib::ControlFlow::Continue
        });

        Some(Self {
            cancel,
            source: Some(source),
        })
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);

        if let Some(source) = self.source.take() {
            source.remove();
        }
    }
}

fn watch_thread(
    app_name: String,
    pid: i32,
    style: ChordStyle,
    sender: std::sync::mpsc::Sender<Vec<Menu>>,
    cancel: Arc<AtomicBool>,
) {
    let context = glib::MainContext::new();

    let _ = context.with_thread_default(|| {
        let Some(tree) = AtspiBus::connect() else {
            return;
        };
        let Some(application) = application(&tree, &app_name, pid) else {
            return;
        };

        register_events(&tree, &application, true);

        let generation = Arc::new(AtomicU64::new(0));
        let mut subscriptions = Vec::new();

        for member in [
            "ChildrenChanged",
            "PropertyChange",
            "StateChanged",
            "VisibleDataChanged",
        ] {
            let generation = Arc::clone(&generation);

            subscriptions.push(tree.a11y.subscribe_to_signal(
                Some(&application.service),
                Some(EVENT_OBJECT),
                Some(member),
                None,
                None,
                gio::DBusSignalFlags::NONE,
                move |_| {
                    generation.fetch_add(1, Ordering::Relaxed);
                },
            ));
        }

        // Subscribe before the first read, and remember the generation from
        // before it: an event that lands while the first tree is being read
        // must still schedule a second delivery rather than being swallowed.
        let mut sent = generation.load(Ordering::Relaxed);
        let initial = menubar_for_application(&tree, &application, style);

        if sender.send(initial).is_err() {
            register_events(&tree, &application, false);
            return;
        }

        let mut last_change = Instant::now();

        while !cancel.load(Ordering::Relaxed) {
            while context.iteration(false) {}

            let current = generation.load(Ordering::Relaxed);

            if current != sent {
                if last_change.elapsed() >= CHANGE_DELAY {
                    let menus = menubar_for_application(&tree, &application, style);

                    if sender.send(menus).is_err() {
                        break;
                    }

                    sent = current;
                }
            } else {
                last_change = Instant::now();
            }

            std::thread::sleep(Duration::from_millis(50));
        }

        drop(subscriptions);
        register_events(&tree, &application, false);
    });
}

fn register_events(tree: &AtspiBus, application: &NodeRef, add: bool) {
    let registry = NodeRef::new(ROOT_SERVICE, REGISTRY_PATH);

    if add {
        let _ = tree.call(
            &registry,
            "org.a11y.atspi.Registry",
            "RegisterEvent",
            Some(
                &(
                    EVENT_REGISTRATION,
                    Vec::<String>::new(),
                    application.service.as_str(),
                )
                    .to_variant(),
            ),
            "()",
        );
    } else {
        let _ = tree.call(
            &registry,
            "org.a11y.atspi.Registry",
            "DeregisterEvent",
            Some(&(EVENT_REGISTRATION, application.service.as_str()).to_variant()),
            "()",
        );
    }
}

/// Activate an accessible menu item with the action a screen reader uses.  A
/// direct choice can be slow, so it is handed to a worker exactly like the
/// original C producer's synchronous call.
pub fn activate(service: &str, path: &str, action: i32) {
    let service = service.to_owned();
    let path = path.to_owned();

    std::thread::spawn(move || {
        let Some(tree) = AtspiBus::connect() else {
            return;
        };
        let node = NodeRef::new(service, path);

        let _ = tree.call(
            &node,
            "org.a11y.atspi.Action",
            "DoAction",
            Some(&(action,).to_variant()),
            "(b)",
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Clone)]
    struct Node {
        role: u32,
        name: String,
        children: Vec<String>,
        states: Vec<u32>,
        binding: Option<String>,
    }

    struct FakeTree {
        nodes: HashMap<String, Node>,
        pid: i32,
    }

    impl AccessibleTree for FakeTree {
        fn role(&self, node: &NodeRef) -> Option<u32> {
            self.nodes.get(&node.path).map(|node| node.role)
        }

        fn name(&self, node: &NodeRef) -> Option<String> {
            self.nodes
                .get(&node.path)
                .map(|node| node.name.clone())
                .filter(|name| !name.is_empty())
        }

        fn children(&self, node: &NodeRef) -> Vec<NodeRef> {
            self.nodes
                .get(&node.path)
                .map(|node| {
                    node.children
                        .iter()
                        .map(|path| NodeRef::new("app", path))
                        .collect()
                })
                .unwrap_or_default()
        }

        fn states(&self, node: &NodeRef) -> Option<Vec<u32>> {
            self.nodes.get(&node.path).map(|node| node.states.clone())
        }

        fn key_binding(&self, node: &NodeRef) -> Option<String> {
            self.nodes
                .get(&node.path)
                .and_then(|node| node.binding.clone())
        }

        fn peer_pid(&self, _candidate: &NodeRef) -> i32 {
            self.pid
        }
    }

    fn node(role: u32, name: &str, children: &[&str]) -> Node {
        Node {
            role,
            name: name.to_owned(),
            children: children.iter().map(|path| (*path).to_owned()).collect(),
            states: vec![(1 << STATE_ENABLED) | (1 << STATE_SENSITIVE)],
            binding: None,
        }
    }

    fn fake_tree() -> FakeTree {
        let mut nodes = HashMap::new();

        nodes.insert(ROOT_PATH.to_owned(), node(0, "root", &["/app"]));
        nodes.insert("/app".to_owned(), node(0, "Fake App", &["/menubar"]));
        nodes.insert(
            "/menubar".to_owned(),
            node(ROLE_MENU_BAR, "Menu Bar", &["/file", "/edit"]),
        );
        nodes.insert(
            "/file".to_owned(),
            node(ROLE_MENU, "File", &["/quit", "/sep", "/recent"]),
        );
        nodes.insert(
            "/edit".to_owned(),
            node(ROLE_MENU, "Edit", &["/copy", "/paste"]),
        );
        nodes.insert(
            "/recent".to_owned(),
            node(ROLE_MENU_ITEM, "Recent", &["/recent_menu"]),
        );
        nodes.insert(
            "/recent_menu".to_owned(),
            node(ROLE_MENU, "Recent", &["/document"]),
        );
        nodes.insert(
            "/document".to_owned(),
            node(ROLE_MENU_ITEM, "Document", &[]),
        );
        nodes.insert("/sep".to_owned(), node(ROLE_SEPARATOR, "", &[]));
        nodes.insert("/quit".to_owned(), node(ROLE_MENU_ITEM, "Quit", &[]));
        nodes.insert("/copy".to_owned(), node(ROLE_MENU_ITEM, "Copy", &[]));
        nodes.insert("/paste".to_owned(), node(ROLE_MENU_ITEM, "Paste", &[]));

        nodes.get_mut("/quit").unwrap().binding = Some("<Primary>q".to_owned());
        nodes.get_mut("/copy").unwrap().binding = Some("Ctrl+C".to_owned());
        nodes.get_mut("/paste").unwrap().states = vec![0];

        FakeTree { nodes, pid: 4242 }
    }

    #[test]
    fn reads_app_names_submenus_separators_and_states() {
        let menus = read_menus(&fake_tree(), "Fake App", 0, ChordStyle::Cua);

        assert_eq!(menus.len(), 2);
        assert_eq!(menus[0].label, "File");
        assert_eq!(menus[0].entries.len(), 3);
        assert!(matches!(
            &menus[0].entries[0],
            MenuEntry::Item { label, enabled: true, accel: Some(accel), action: Some(MenuAction::Atspi { path, .. }), .. }
                if label == "Quit" && accel == "Ctrl+Q" && path == "/quit"
        ));
        assert!(matches!(&menus[0].entries[1], MenuEntry::Separator));
        assert!(matches!(
            &menus[0].entries[2],
            MenuEntry::Item { label, submenu, .. }
                if label == "Recent"
                    && matches!(&submenu[0], MenuEntry::Item { label, .. } if label == "Document")
        ));
        assert!(matches!(
            &menus[1].entries[1],
            MenuEntry::Item { label, enabled: false, .. } if label == "Paste"
        ));
    }

    #[test]
    fn matches_by_pid_before_name_and_returns_empty_for_unknown() {
        let tree = fake_tree();

        assert!(read_menus(&tree, "Nothing Here", 0, ChordStyle::Cua).is_empty());
        assert_eq!(read_menus(&tree, "Fake App", 0, ChordStyle::Cua).len(), 2);
        assert_eq!(
            read_menus(&tree, "Wrong Name", 4242, ChordStyle::Cua).len(),
            2
        );
    }

    #[test]
    fn a_cycle_is_bounded_instead_of_looping() {
        let mut tree = fake_tree();
        tree.nodes
            .get_mut("/file")
            .unwrap()
            .children
            .push("/file".to_owned());

        let menus = read_menus(&tree, "Fake App", 0, ChordStyle::Cua);

        assert_eq!(menus.len(), 2);
    }
}
