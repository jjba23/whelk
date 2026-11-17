// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The Wi-Fi and Bluetooth panel, ported from `src/network_panel.c`.
//!
//! NetworkManager and BlueZ are read and commanded through `nmcli` and
//! `bluetoothctl`, the scripting surfaces those projects ship.  The panel is a
//! right-edge layer surface because choosing a network needs a list and, for a
//! new network, a passphrase; a bar row has room for neither.
//!
//! Parsing and the status sentences are plain functions, while the GTK half
//! only paints their result and starts the next command.  A typed passphrase
//! travels on standard input, never on a command line where `ps` could read it.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::bar::BarSettings;
use crate::command;
use crate::format;
use crate::nmcli;
use crate::style;

pub const WIDTH: i32 = 400;
const PADDING: i32 = 12;
const SURFACE_MARGIN: i32 = 8;

pub const WIFI_COMMAND: &str = "printf 'radio:%s\\n' \"$(nmcli radio wifi 2>/dev/null)\"; \
     printf 'active:%s\\n' \"$(nmcli -t -f ACTIVE,SSID dev wifi 2>/dev/null | \
     sed -n 's/^yes://p' | head -n1)\"; \
     nmcli -t -f SSID,SIGNAL,SECURITY dev wifi list 2>/dev/null";

pub const BLUETOOTH_COMMAND: &str = "printf 'powered:%s\\n' \"$(bluetoothctl show 2>/dev/null | \
     sed -n 's/^[[:space:]]*Powered: //p')\"; \
     echo --paired--; bluetoothctl devices Paired 2>/dev/null; \
     echo --connected--; bluetoothctl devices Connected 2>/dev/null";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessPoint {
    pub ssid: String,
    pub signal: i32,
    pub secured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WifiState {
    pub radio: bool,
    pub active: Option<String>,
    pub points: Vec<AccessPoint>,
}

impl WifiState {
    pub fn status(&self) -> String {
        if !self.radio {
            "Wi-Fi is off".to_owned()
        } else if let Some(active) = self.active.as_deref().filter(|value| !value.is_empty()) {
            format!("Connected to {active}")
        } else {
            "Not connected".to_owned()
        }
    }

    pub fn empty_message(&self) -> &'static str {
        if self.radio {
            "No networks in range"
        } else {
            "Wi-Fi is off"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BtDevice {
    pub name: String,
    pub mac: String,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BluetoothState {
    pub powered: bool,
    pub devices: Vec<BtDevice>,
}

impl BluetoothState {
    pub fn status(&self) -> String {
        if !self.powered {
            "Bluetooth is off".to_owned()
        } else if self.devices.is_empty() {
            "Bluetooth is on".to_owned()
        } else {
            format!(
                "{} paired device{}",
                self.devices.len(),
                if self.devices.len() == 1 { "" } else { "s" }
            )
        }
    }

    pub fn empty_message(&self) -> &'static str {
        if self.powered {
            "No paired devices"
        } else {
            "Bluetooth is off"
        }
    }
}

/// Parse the combined Wi-Fi reading: radio state, active SSID, then one
/// access point per line.  Duplicate SSIDs collapse to the first reading, and
/// the surviving points are strongest first, like the C panel.
pub fn parse_wifi(text: &str) -> WifiState {
    let mut state = WifiState::default();

    for line in text.lines() {
        if let Some(radio) = line.strip_prefix("radio:") {
            state.radio = radio == "enabled";
            continue;
        }

        if let Some(active) = line.strip_prefix("active:") {
            state.active = Some(active.to_owned());
            continue;
        }

        if line.is_empty() {
            continue;
        }

        let fields = nmcli::split_terse(line);

        if fields.len() < 3 {
            continue;
        }

        let ssid = &fields[0];
        let signal = fields[1].trim().parse().unwrap_or(0);
        let security = &fields[2];

        if ssid.is_empty() || state.points.iter().any(|point| point.ssid == *ssid) {
            continue;
        }

        state.points.push(AccessPoint {
            ssid: ssid.clone(),
            signal,
            secured: !security.is_empty() && security != "--",
        });
    }

    state
        .points
        .sort_by_key(|point| std::cmp::Reverse(point.signal));

    state
}

/// Parse the Bluetooth reading's two sections.  Only paired devices are listed;
/// the connected section is a set of addresses used to mark them.
pub fn parse_bluetooth(text: &str) -> BluetoothState {
    let mut state = BluetoothState::default();
    let mut connected_section = false;
    let mut connected = Vec::new();

    for line in text.lines() {
        if let Some(powered) = line.strip_prefix("powered:") {
            state.powered = powered == "yes";
            continue;
        }

        if line == "--paired--" {
            connected_section = false;
            continue;
        }

        if line == "--connected--" {
            connected_section = true;
            continue;
        }

        let Some(device) = line.strip_prefix("Device ") else {
            continue;
        };
        let Some((mac, name)) = device.split_once(' ') else {
            continue;
        };

        // BlueZ prints a 48-bit address.  Anything else is a malformed or
        // forged row, and this field later reaches a command line.
        if !is_bluetooth_address(mac) {
            continue;
        }

        if connected_section {
            connected.push(mac.to_owned());
        } else {
            state.devices.push(BtDevice {
                name: name.to_owned(),
                mac: mac.to_owned(),
                connected: false,
            });
        }
    }

    for device in &mut state.devices {
        device.connected = connected.contains(&device.mac);
    }

    state
}

/// A Bluetooth device address as BlueZ prints it: six hexadecimal octets in
/// colon-separated pairs.  The panel embeds this in `bluetoothctl` commands.
pub fn is_bluetooth_address(value: &str) -> bool {
    value.len() == 17
        && value.as_bytes().iter().enumerate().all(|(index, byte)| {
            if index % 3 == 2 {
                *byte == b':'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

thread_local! {
    static CURRENT: RefCell<Option<NetworkPanel>> = const { RefCell::new(None) };
}

#[derive(Clone)]
pub struct NetworkPanel {
    inner: Rc<RefCell<State>>,
}

struct State {
    window: gtk::ApplicationWindow,
    card: gtk::Box,
    stack: gtk::Stack,
    close: gtk::Button,
    wifi_list: gtk::Box,
    wifi_status: gtk::Label,
    wifi_switch: gtk::Switch,
    rescan: gtk::Button,
    password_row: gtk::Box,
    password_label: gtk::Label,
    password_entry: gtk::Entry,
    connect: gtk::Button,
    pending_ssid: Option<String>,
    bt_list: gtk::Box,
    bt_status: gtk::Label,
    bt_switch: gtk::Switch,
    refresh_source: Option<gtk::glib::SourceId>,
    css: Option<gtk::CssProvider>,
    updating: bool,
}

impl NetworkPanel {
    pub fn new(application: &gtk::Application, style: &BarSettings) -> Self {
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title("whelk network")
            .build();

        window.set_widget_name(&style::widget_id("network"));
        window.add_css_class("whelk-surface");
        window.set_decorated(false);
        window.set_resizable(true);

        let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let title = gtk::Label::new(Some("Network"));
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        let stack = gtk::Stack::new();
        let switcher = gtk::StackSwitcher::new();

        card.add_css_class("whelk-panel");
        card.set_size_request(WIDTH, -1);
        card.set_margin_start(PADDING);
        card.set_margin_end(PADDING);
        card.set_margin_top(PADDING);
        card.set_margin_bottom(PADDING);
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.add_css_class("whelk-panel-title");
        close.add_css_class("whelk-panel-button");

        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        switcher.set_stack(Some(&stack));
        switcher.set_halign(gtk::Align::Start);

        let wifi = build_wifi_page("Wi-Fi", "wifi", &stack);
        let bluetooth = build_bluetooth_page("Bluetooth", "bluetooth", &stack);

        header.append(&title);
        header.append(&close);
        card.append(&header);
        card.append(&switcher);
        card.append(&stack);
        window.set_child(Some(&card));

        let inner = Rc::new(RefCell::new(State {
            window,
            card,
            stack,
            close,
            wifi_list: wifi.list,
            wifi_status: wifi.status,
            wifi_switch: wifi.switch,
            rescan: wifi.rescan,
            password_row: wifi.password_row,
            password_label: wifi.password_label,
            password_entry: wifi.password_entry,
            connect: wifi.connect,
            pending_ssid: None,
            bt_list: bluetooth.list,
            bt_status: bluetooth.status,
            bt_switch: bluetooth.switch,
            refresh_source: None,
            css: None,
            updating: false,
        }));
        let panel = Self { inner };

        panel.install_style(style);
        panel.configure_surface();
        panel.connect_controls();
        panel
    }

    pub fn current() -> Option<Self> {
        CURRENT.with(|current| current.borrow().clone())
    }

    pub fn install(panel: Option<Self>) {
        CURRENT.with(|current| *current.borrow_mut() = panel);
    }

    pub fn show_current(page: &str) {
        if let Some(panel) = Self::current() {
            panel.show(Some(page));
        }
    }

    pub fn hide_current() {
        if let Some(panel) = Self::current() {
            panel.hide();
        }
    }

    pub fn show(&self, page: Option<&str>) {
        if let Some(page) = page.filter(|page| *page == "wifi" || *page == "bluetooth") {
            self.inner.borrow().stack.set_visible_child_name(page);
        }

        self.inner.borrow().window.present();
        self.refresh();
    }

    pub fn hide(&self) {
        self.cancel_refresh();
        self.inner.borrow().window.set_visible(false);
        self.inner.borrow_mut().pending_ssid = None;
    }

    pub fn is_visible(&self) -> bool {
        self.inner.borrow().window.is_visible()
    }

    pub fn showing(&self, page: &str) -> bool {
        self.is_visible()
            && self
                .inner
                .borrow()
                .stack
                .visible_child_name()
                .is_some_and(|name| name == page)
    }

    pub fn set_style(&self, style: &BarSettings) {
        self.install_style(style);
        self.inner.borrow().card.queue_draw();
    }

    fn configure_surface(&self) {
        let state = self.inner.borrow();
        let window = &state.window;

        if crate::ui::layer_shell_available() {
            window.init_layer_shell();
            window.set_namespace(Some("whelk-rust-network"));
            window.set_layer(Layer::Overlay);
            window.set_anchor(Edge::Top, true);
            window.set_anchor(Edge::Bottom, true);
            window.set_anchor(Edge::Right, true);
            window.set_margin(Edge::Top, SURFACE_MARGIN);
            window.set_margin(Edge::Right, SURFACE_MARGIN);
            window.set_margin(Edge::Bottom, SURFACE_MARGIN);
            window.set_margin(Edge::Left, SURFACE_MARGIN);
            window.set_exclusive_zone(0);
            // The passphrase entry needs the keyboard while the panel is open.
            window.set_keyboard_mode(KeyboardMode::OnDemand);

            let height = gtk::gdk::Display::default()
                .and_then(|display| {
                    display
                        .monitors()
                        .item(0)
                        .and_downcast::<gtk::gdk::Monitor>()
                })
                .map(|monitor| monitor.geometry().height())
                .unwrap_or(800);

            window.set_default_size(WIDTH + 2 * PADDING, (height - 2 * SURFACE_MARGIN).max(300));
        } else {
            window.set_default_size(WIDTH + 2 * PADDING, 700);
        }

        window.present();
        window.set_visible(false);
    }

    fn connect_controls(&self) {
        let state = self.inner.borrow();
        let weak = Rc::downgrade(&self.inner);

        state.wifi_switch.connect_active_notify(move |switch| {
            if let Some(inner) = weak.upgrade() {
                NetworkPanel { inner }.toggle_wifi(switch.is_active());
            }
        });

        let weak = Rc::downgrade(&self.inner);

        state.bt_switch.connect_active_notify(move |switch| {
            if let Some(inner) = weak.upgrade() {
                NetworkPanel { inner }.toggle_bluetooth(switch.is_active());
            }
        });

        let weak = Rc::downgrade(&self.inner);

        state.rescan.connect_clicked(move |_| {
            if let Some(inner) = weak.upgrade() {
                NetworkPanel { inner }.run_wifi_action("nmcli device wifi rescan");
            }
        });

        let weak = Rc::downgrade(&self.inner);
        let row = state.password_row.clone();

        state.password_entry.connect_activate(move |_| {
            if let Some(inner) = weak.upgrade() {
                NetworkPanel { inner }.submit_password(&row);
            }
        });

        let weak = Rc::downgrade(&self.inner);
        let row = state.password_row.clone();

        state.connect.connect_clicked(move |_| {
            if let Some(inner) = weak.upgrade() {
                NetworkPanel { inner }.submit_password(&row);
            }
        });

        let weak = Rc::downgrade(&self.inner);

        state.close.connect_clicked(move |_| {
            if let Some(inner) = weak.upgrade() {
                NetworkPanel { inner }.hide();
            }
        });

        let weak = Rc::downgrade(&self.inner);

        state.stack.connect_visible_child_name_notify(move |_| {
            if let Some(inner) = weak.upgrade() {
                NetworkPanel { inner }.refresh();
            }
        });
    }

    fn toggle_wifi(&self, active: bool) {
        if self.inner.borrow().updating {
            return;
        }

        self.run_wifi_action(&format!(
            "nmcli radio wifi {}",
            if active { "on" } else { "off" }
        ));
    }

    fn toggle_bluetooth(&self, active: bool) {
        if self.inner.borrow().updating {
            return;
        }

        let weak = Rc::downgrade(&self.inner);

        command::run(
            &format!(
                "bluetoothctl power {} 2>&1",
                if active { "on" } else { "off" }
            ),
            move |text| {
                if let Some(inner) = weak.upgrade() {
                    NetworkPanel { inner }.bt_action_done(text);
                }
            },
        );
    }

    fn refresh(&self) {
        self.inner.borrow().password_row.set_visible(false);
        self.refresh_current();
    }

    fn refresh_current(&self) {
        let page = self.inner.borrow().stack.visible_child_name();

        if page.as_deref() == Some("bluetooth") {
            self.run_bluetooth_refresh();
        } else {
            self.run_wifi_refresh();
        }
    }

    fn run_wifi_refresh(&self) {
        let weak = Rc::downgrade(&self.inner);

        command::run(WIFI_COMMAND, move |text| {
            if let Some(inner) = weak.upgrade() {
                NetworkPanel { inner }.apply_wifi(text.as_deref().unwrap_or(""));
            }
        });
    }

    fn run_bluetooth_refresh(&self) {
        let weak = Rc::downgrade(&self.inner);

        command::run(BLUETOOTH_COMMAND, move |text| {
            if let Some(inner) = weak.upgrade() {
                NetworkPanel { inner }.apply_bluetooth(text.as_deref().unwrap_or(""));
            }
        });
    }

    fn apply_wifi(&self, text: &str) {
        let state = parse_wifi(text);
        let (list, status, switch) = {
            let inner = self.inner.borrow();

            (
                inner.wifi_list.clone(),
                inner.wifi_status.clone(),
                inner.wifi_switch.clone(),
            )
        };

        clear_box(&list);

        for point in &state.points {
            list.append(&self.build_wifi_row(point));
        }

        if state.points.is_empty() {
            let empty = gtk::Label::new(Some(state.empty_message()));

            empty.add_css_class("whelk-panel-empty");
            list.append(&empty);
        }

        status.set_text(&state.status());

        self.inner.borrow_mut().updating = true;
        switch.set_active(state.radio);
        self.inner.borrow_mut().updating = false;
    }

    fn apply_bluetooth(&self, text: &str) {
        let state = parse_bluetooth(text);
        let (list, status, switch) = {
            let inner = self.inner.borrow();

            (
                inner.bt_list.clone(),
                inner.bt_status.clone(),
                inner.bt_switch.clone(),
            )
        };

        clear_box(&list);

        for device in &state.devices {
            list.append(&self.build_bluetooth_row(device));
        }

        if state.devices.is_empty() {
            let empty = gtk::Label::new(Some(state.empty_message()));

            empty.add_css_class("whelk-panel-empty");
            list.append(&empty);
        }

        status.set_text(&state.status());

        self.inner.borrow_mut().updating = true;
        switch.set_active(state.powered);
        self.inner.borrow_mut().updating = false;
    }

    fn build_wifi_row(&self, point: &AccessPoint) -> gtk::Button {
        let row = gtk::Button::new();
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let name = gtk::Label::new(Some(&point.ssid));
        let signal = gtk::Label::new(Some(&format!("{}%", point.signal)));

        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name.set_hexpand(true);
        content.add_css_class("whelk-network-row");
        name.add_css_class("whelk-network-name");
        signal.add_css_class("whelk-network-detail");
        content.append(&name);

        if point.secured {
            let lock = gtk::Image::from_icon_name("changes-prevent-symbolic");

            lock.set_pixel_size(14);
            content.append(&lock);
        }

        content.append(&signal);
        row.set_child(Some(&content));
        row.add_css_class("whelk-network-item");

        let ssid = point.ssid.clone();
        let secured = point.secured;
        let weak = Rc::downgrade(&self.inner);

        row.connect_clicked(move |_| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let panel = NetworkPanel { inner };

            if secured {
                panel.ask_password(&ssid);
            } else {
                panel.connect_wifi(&ssid, None);
            }
        });

        row
    }

    fn build_bluetooth_row(&self, device: &BtDevice) -> gtk::Button {
        let row = gtk::Button::new();
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let name = gtk::Label::new(Some(&device.name));
        let detail_text = if device.connected {
            "Connected".to_owned()
        } else {
            device.mac.clone()
        };
        let detail = gtk::Label::new(Some(&detail_text));

        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name.set_hexpand(true);
        content.add_css_class("whelk-network-row");
        detail.add_css_class("whelk-network-detail");
        content.append(&name);
        content.append(&detail);
        row.set_child(Some(&content));
        row.add_css_class("whelk-network-item");

        if device.connected {
            row.add_css_class("connected");
        }

        let mac = device.mac.clone();
        let connected = device.connected;
        let weak = Rc::downgrade(&self.inner);

        row.connect_clicked(move |_| {
            if !is_bluetooth_address(&mac) {
                return;
            }
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let panel = NetworkPanel { inner };

            panel.run_bluetooth_action(&format!(
                "bluetoothctl {} {mac} 2>&1",
                if connected { "disconnect" } else { "connect" }
            ));
        });

        row
    }

    fn ask_password(&self, ssid: &str) {
        let (row, label, entry) = {
            let inner = self.inner.borrow();

            (
                inner.password_row.clone(),
                inner.password_label.clone(),
                inner.password_entry.clone(),
            )
        };

        self.inner.borrow_mut().pending_ssid = Some(ssid.to_owned());
        label.set_text(&format!("Password for {ssid} (blank: use saved)"));
        row.set_visible(true);
        entry.grab_focus();
    }

    fn submit_password(&self, row: &gtk::Box) {
        let (ssid, password, entry) = {
            let mut inner = self.inner.borrow_mut();
            let Some(ssid) = inner.pending_ssid.take() else {
                return;
            };
            let password = inner.password_entry.text().to_string();
            let entry = inner.password_entry.clone();

            (ssid, password, entry)
        };

        row.set_visible(false);
        entry.set_text("");

        if password.is_empty() {
            self.connect_wifi(&ssid, None);
        } else {
            self.connect_wifi(&ssid, Some(&password));
        }
    }

    fn connect_wifi(&self, ssid: &str, password: Option<&str>) {
        if let Some(password) = password.filter(|value| !value.is_empty()) {
            let weak = Rc::downgrade(&self.inner);
            let ssid = ssid.to_owned();
            let password = password.to_owned();

            command::run(
                "nmcli -t -f NAME,TYPE connection show 2>/dev/null",
                move |text| {
                    let Some(inner) = weak.upgrade() else {
                        return;
                    };
                    let names = text
                        .map(|text| nmcli::wifi_profile_names(&text))
                        .unwrap_or_default();

                    NetworkPanel { inner }.probe_profile(&ssid, &password, &names, 0);
                },
            );
        } else {
            self.run_wifi_action(&format!(
                "nmcli --wait 20 device wifi connect {}",
                quote(ssid)
            ));
        }
    }

    fn probe_profile(&self, ssid: &str, password: &str, names: &[String], index: usize) {
        let Some(profile) = names.get(index) else {
            self.run_wifi_input(
                &format!("nmcli --ask --wait 20 device wifi connect {}", quote(ssid)),
                password,
            );

            return;
        };

        let weak = Rc::downgrade(&self.inner);
        let ssid = ssid.to_owned();
        let password = password.to_owned();
        let names = names.to_vec();
        let profile = profile.clone();
        let query = profile.clone();
        let command_line = format!(
            "nmcli -t -g 802-11-wireless.ssid connection show {} 2>/dev/null",
            quote(&query)
        );

        command::run(&command_line, move |text| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let panel = NetworkPanel { inner };

            if text.as_deref().map(str::trim) == Some(ssid.as_str()) {
                panel.write_wifi_key(&profile, &password);
            } else {
                panel.probe_profile(&ssid, &password, &names, index + 1);
            }
        });
    }

    fn write_wifi_key(&self, profile: &str, password: &str) {
        let Some(script) = nmcli::key_update_script(password) else {
            self.set_status(
                StatusPage::Wifi,
                "That password cannot be saved: it starts or ends with a space.",
            );
            self.schedule_refresh(Duration::from_millis(2_500));

            return;
        };
        let weak = Rc::downgrade(&self.inner);
        let profile = profile.to_owned();
        let command_line = format!("nmcli connection edit {} 2>&1", quote(&profile));

        self.set_status(StatusPage::Wifi, "Saving the new password…");

        command::run_input_status(&command_line, Some(&script), move |text, status| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let panel = NetworkPanel { inner };

            if status == 0 {
                panel.activate_wifi_profile(&profile);
            } else {
                panel.wifi_action_done(text);
            }
        });
    }

    fn activate_wifi_profile(&self, profile: &str) {
        self.run_wifi_action(&format!("nmcli --wait 30 connection up {}", quote(profile)));
    }

    fn run_wifi_input(&self, command_line: &str, input: &str) {
        let weak = Rc::downgrade(&self.inner);
        let input = format!("{input}\n");

        command::run_input(&format!("{command_line} 2>&1"), Some(&input), move |text| {
            if let Some(inner) = weak.upgrade() {
                NetworkPanel { inner }.wifi_action_done(text);
            }
        });
    }

    fn run_wifi_action(&self, command_line: &str) {
        let weak = Rc::downgrade(&self.inner);

        command::run(&format!("{command_line} 2>&1"), move |text| {
            if let Some(inner) = weak.upgrade() {
                NetworkPanel { inner }.wifi_action_done(text);
            }
        });
    }

    fn run_bluetooth_action(&self, command_line: &str) {
        let weak = Rc::downgrade(&self.inner);

        command::run(command_line, move |text| {
            if let Some(inner) = weak.upgrade() {
                NetworkPanel { inner }.bt_action_done(text);
            }
        });
    }

    fn wifi_action_done(&self, text: Option<String>) {
        if let Some(text) = text.filter(|text| !text.is_empty()) {
            self.set_status(StatusPage::Wifi, format::first_line(&text));
        }

        self.schedule_refresh(Duration::from_millis(1_200));
    }

    fn bt_action_done(&self, text: Option<String>) {
        if let Some(text) = text.filter(|text| !text.is_empty()) {
            self.set_status(StatusPage::Bluetooth, format::first_line(&text));
        }

        self.schedule_refresh(Duration::from_millis(1_500));
    }

    fn set_status(&self, page: StatusPage, text: &str) {
        let label = {
            let inner = self.inner.borrow();

            match page {
                StatusPage::Wifi => inner.wifi_status.clone(),
                StatusPage::Bluetooth => inner.bt_status.clone(),
            }
        };

        label.set_text(text);
    }

    fn schedule_refresh(&self, delay: Duration) {
        self.cancel_refresh();

        let weak = Rc::downgrade(&self.inner);
        let source = gtk::glib::timeout_add_local_once(delay, move || {
            if let Some(inner) = weak.upgrade() {
                let panel = NetworkPanel { inner };

                panel.inner.borrow_mut().refresh_source = None;
                panel.refresh_current();
            }
        });

        self.inner.borrow_mut().refresh_source = Some(source);
    }

    fn cancel_refresh(&self) {
        let source = self.inner.borrow_mut().refresh_source.take();

        if let Some(source) = source {
            source.remove();
        }
    }

    fn install_style(&self, bar: &BarSettings) {
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        let generated = style::css(&style::widget_id("network"), bar, "whelk-panel", None);
        let css = gtk::CssProvider::new();

        css.load_from_string(&format!("{BASE_CSS}\n{generated}"));

        let mut state = self.inner.borrow_mut();

        if let Some(previous) = state.css.take() {
            gtk::style_context_remove_provider_for_display(&display, &previous);
        }

        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        state.css = Some(css);
    }
}

impl Drop for State {
    fn drop(&mut self) {
        if let Some(source) = self.refresh_source.take() {
            source.remove();
        }

        if let Some(css) = self.css.take()
            && let Some(display) = gtk::gdk::Display::default()
        {
            gtk::style_context_remove_provider_for_display(&display, &css);
        }

        self.window.destroy();
    }
}

#[derive(Clone, Copy)]
enum StatusPage {
    Wifi,
    Bluetooth,
}

struct CommonPage {
    page: gtk::Box,
    list: gtk::Box,
    status: gtk::Label,
    switch: gtk::Switch,
    scroller: gtk::ScrolledWindow,
}

struct WifiPage {
    list: gtk::Box,
    status: gtk::Label,
    switch: gtk::Switch,
    rescan: gtk::Button,
    connect: gtk::Button,
    password_row: gtk::Box,
    password_label: gtk::Label,
    password_entry: gtk::Entry,
}

struct BluetoothPage {
    list: gtk::Box,
    status: gtk::Label,
    switch: gtk::Switch,
}

/// The list, status line and radio switch shared by both pages.  The caller
/// appends its own controls before `present_page`, so a page's widgets keep
/// the order they are built in and no constructor has to unwrap an optional
/// control later.
fn build_common(title: &str) -> CommonPage {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let heading = gtk::Label::new(Some(title));
    let switch = gtk::Switch::new();
    let status = gtk::Label::new(None);
    let scroller = gtk::ScrolledWindow::new();
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);

    heading.set_xalign(0.0);
    heading.set_hexpand(true);
    heading.add_css_class("whelk-panel-title");
    switch.set_valign(gtk::Align::Center);

    header.append(&heading);
    header.append(&switch);

    status.set_xalign(0.0);
    status.set_wrap(true);
    status.add_css_class("whelk-network-status");

    scroller.set_child(Some(&list));
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_vexpand(true);

    page.append(&header);
    page.append(&status);

    CommonPage {
        page,
        list,
        status,
        switch,
        scroller,
    }
}

fn present_page(
    page: &gtk::Box,
    scroller: &gtk::ScrolledWindow,
    stack: &gtk::Stack,
    name: &str,
    title: &str,
) {
    page.append(scroller);
    stack.add_titled(page, Some(name), title);
}

fn build_wifi_page(title: &str, name: &str, stack: &gtk::Stack) -> WifiPage {
    let CommonPage {
        page,
        list,
        status,
        switch,
        scroller,
    } = build_common(title);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let rescan = gtk::Button::with_label("Rescan");
    let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let password_label = gtk::Label::new(None);
    let password_entry = gtk::Entry::new();
    let connect = gtk::Button::with_label("Connect");

    rescan.add_css_class("whelk-network-button");
    actions.append(&rescan);
    page.append(&actions);

    password_label.set_xalign(0.0);
    password_label.set_wrap(true);
    password_label.add_css_class("whelk-network-hint");
    password_entry.set_visibility(false);
    password_entry.set_placeholder_text(Some("Network password"));
    connect.add_css_class("accent");
    row.append(&password_label);
    row.append(&password_entry);
    row.append(&connect);
    row.set_visible(false);
    page.append(&row);

    present_page(&page, &scroller, stack, name, title);

    WifiPage {
        list,
        status,
        switch,
        rescan,
        connect,
        password_row: row,
        password_label,
        password_entry,
    }
}

fn build_bluetooth_page(title: &str, name: &str, stack: &gtk::Stack) -> BluetoothPage {
    let CommonPage {
        page,
        list,
        status,
        switch,
        scroller,
    } = build_common(title);

    present_page(&page, &scroller, stack, name, title);

    BluetoothPage {
        list,
        status,
        switch,
    }
}

fn clear_box(list: &gtk::Box) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn quote(value: &str) -> String {
    gtk::glib::shell_quote(value).to_string_lossy().into_owned()
}

const BASE_CSS: &str = "
    .whelk-surface {
        background-color: transparent;
    }
    .whelk-panel {
        border-radius: 14px;
    }
    .whelk-panel-title {
        font-weight: 700;
        font-size: 1.05em;
    }
    .whelk-panel-button {
        padding: 4px;
        min-width: 0;
        min-height: 0;
        border-radius: 8px;
        background: transparent;
    }
    .whelk-panel-button:hover {
        background: alpha(currentColor, 0.12);
    }
    .whelk-panel-button:checked {
        background: alpha(currentColor, 0.18);
    }
    .whelk-panel-empty {
        padding: 30px 8px;
        opacity: 0.65;
    }
    .whelk-network-item {
        padding: 8px;
        border-radius: 8px;
        background: transparent;
    }
    .whelk-network-item:hover {
        background: alpha(currentColor, 0.08);
    }
    .whelk-network-name {
        font-weight: 500;
    }
    .whelk-network-detail,
    .whelk-network-status,
    .whelk-network-hint {
        opacity: 0.6;
        font-size: 0.85em;
    }
    .whelk-network-button {
        padding: 4px 10px;
        border-radius: 8px;
    }
";
