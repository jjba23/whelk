// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Batteries from `sysfs`, ported from `modules/battery.c` and the battery half
//! of `util/sysfs.c`.
//!
//! Only a pack that is filling is quiet: while charging the percentage is
//! hidden unless `#:show-capacity-when-charging?` asks for it.  A full pack or
//! one plugged in but not taking power still shows its capacity and keeps the
//! warning and critical colours.
//!
//! Two things a percentage cannot answer are here too: *how long* (`{time}`,
//! from the pack's energy and power, or its charge and current) and *both
//! packs* (`#:device "all"`), which sums what the machine reports instead of
//! letting the first battery speak for the second.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::{BarSettings, ModuleSpec};
use crate::barfactory::ModuleContext;
use crate::barmodule::{self, ModuleHost};
use crate::style;

pub const DEFAULT_INTERVAL_SECONDS: u32 = 30;
const POWER_SUPPLY_ROOT: &str = "/sys/class/power_supply";
/// `#:device "all"` sums every pack in the machine.
pub const ALL_PACKS: &str = "all";

/// What the reading says on hover when the configuration wrote no `#:tooltip`:
/// the percentage, the state, and only the facts the pack actually reports.
const DEFAULT_TOOLTIP: &str = "Battery: {capacity}% — {state}\n{detail}";

const CARD_CSS: &str = "
.whelk-power-card {
  padding: 6px;
}
.whelk-power-card-body {
  padding: 4px;
}
.whelk-power-card-title {
  font-weight: 600;
}
.whelk-power-card-percent {
  font-size: 1.5em;
  font-weight: 300;
}
.whelk-power-card-level trough {
  min-height: 6px;
  border-radius: 3px;
  background-color: alpha(currentColor, 0.16);
}
.whelk-power-card-level block.filled {
  min-height: 6px;
  border-radius: 3px;
  background-color: currentColor;
}
.whelk-power-card-label,
.whelk-power-card-pack {
  opacity: 0.72;
}
";

/// One battery pack as the kernel reports it.
///
/// The energy pair and the charge pair are alternatives: a pack reports one or
/// the other, and a charge pair needs the voltage to become energy.  Everything
/// stays in micro-units (`µWh`, `µAh`, `µW`, `µA`, `µV`) so the arithmetic is
/// one division.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pack {
    pub name: String,
    pub status: String,
    pub capacity: i32,
    pub present: bool,
    pub energy_now: Option<i64>,
    pub energy_full: Option<i64>,
    /// What the cells held when the pack was new, which is what makes health
    /// a percentage rather than a guess.
    pub energy_full_design: Option<i64>,
    pub charge_now: Option<i64>,
    pub charge_full: Option<i64>,
    pub charge_full_design: Option<i64>,
    pub power_now: Option<i64>,
    pub current_now: Option<i64>,
    pub voltage_now: Option<i64>,
}

impl Pack {
    fn is_charging(&self) -> bool {
        self.status == "Charging"
    }

    fn is_discharging(&self) -> bool {
        self.status == "Discharging"
    }

    /// The pack's charge in micro-watt-hours, when it can be worked out.
    fn energy(&self) -> Option<(i64, Option<i64>)> {
        if let Some(now) = self.energy_now {
            return Some((now, self.energy_full));
        }

        let now = self.charge_now?;
        let voltage = self.voltage_now.filter(|voltage| *voltage > 0);

        Some((now, self.charge_full).map_now(voltage))
    }

    /// The pack's design charge in micro-watt-hours, when it reports one.
    fn design_energy(&self) -> Option<i64> {
        if let Some(design) = self.energy_full_design.filter(|design| *design > 0) {
            return Some(design);
        }

        let design = self.charge_full_design.filter(|design| *design > 0)?;
        let voltage = self.voltage_now.filter(|voltage| *voltage > 0)?;

        Some(design.saturating_mul(voltage) / 1_000_000)
    }

    /// The pack's rate in microwatts, when it can be worked out.
    fn power(&self) -> Option<i64> {
        if let Some(power) = self.power_now.filter(|power| *power > 0) {
            return Some(power);
        }

        let current = self.current_now.filter(|current| *current > 0)?;
        let voltage = self.voltage_now.filter(|voltage| *voltage > 0);

        Some(match voltage {
            // µA × µV = pW; a microwatt is a million picowatts.
            Some(voltage) => current.saturating_mul(voltage) / 1_000_000,
            None => current,
        })
    }
}

/// `(now, full)` scaled by a voltage in microvolts, when there is one.
trait ScaleByVoltage {
    fn map_now(self, voltage: Option<i64>) -> Self;
}

impl ScaleByVoltage for (i64, Option<i64>) {
    fn map_now(self, voltage: Option<i64>) -> Self {
        let Some(voltage) = voltage else {
            return self;
        };
        let scale = |value: i64| value.saturating_mul(voltage) / 1_000_000;

        (scale(self.0), self.1.map(scale))
    }
}

/// One reading the module paints: the pack (or the machine's packs) as one
/// percentage, one state and, when it can be known, one duration, one draw and
/// one measure of how worn the cells are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryReading {
    pub name: String,
    pub status: String,
    pub capacity: i32,
    pub present: bool,
    /// Seconds until empty (discharging) or until full (charging).
    pub seconds_remaining: Option<i64>,
    /// The rate the machine is taking from or giving to the pack, in microwatts.
    pub power_microwatts: Option<i64>,
    /// The full charge now against the design charge, as a percentage.
    pub health_percent: Option<i32>,
}

pub fn is_charging(status: &str) -> bool {
    status == "Charging"
}

/// The class the structural stylesheet turns into a colour.  Charging wins over
/// critical, and critical over warning, exactly as in the C module; a pack at
/// rest at the top of its range says so instead of wearing a warning colour.
pub fn battery_class(
    capacity: i32,
    status: &str,
    warning: i32,
    critical: i32,
) -> Option<&'static str> {
    if is_charging(status) {
        Some("charging")
    } else if capacity <= critical {
        Some("critical")
    } else if capacity <= warning {
        Some("warning")
    } else if status == "Full" {
        Some("full")
    } else {
        None
    }
}

/// Seconds until the pack is empty or full, from its energy and rate.
///
/// A full pack has nothing left to count, and a pack that is neither filling
/// nor emptying has no honest answer, so both are `None`.
pub fn seconds_remaining(pack: &Pack) -> Option<i64> {
    if !(pack.is_charging() || pack.is_discharging()) {
        return None;
    }

    let (now, full) = pack.energy()?;
    let rate = pack.power()?;

    if rate <= 0 {
        return None;
    }

    let micros = if pack.is_charging() {
        full?.saturating_sub(now)
    } else {
        now
    };

    if micros <= 0 {
        return None;
    }

    Some(micros.saturating_mul(3_600) / rate)
}

/// A duration as a battery reads it: `2h 05m`, `45m`, or `-` when it is not
/// known.
pub fn format_seconds(seconds: Option<i64>) -> String {
    let Some(seconds) = seconds.filter(|seconds| *seconds > 0) else {
        return "-".to_owned();
    };
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;

    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        "<1m".to_owned()
    }
}

/// A rate as a reader sees it: `12.3 W`, or `-` when the pack reports none.
pub fn format_watts(microwatts: Option<i64>) -> String {
    match microwatts.filter(|microwatts| *microwatts > 0) {
        Some(microwatts) => format!("{:.1} W", microwatts as f64 / 1_000_000.0),
        None => "-".to_owned(),
    }
}

/// The summed rate of every present pack, in microwatts.
pub fn power_microwatts(packs: &[Pack]) -> Option<i64> {
    let total: i64 = packs
        .iter()
        .filter(|pack| pack.present)
        .filter_map(Pack::power)
        .sum();

    (total > 0).then_some(total)
}

/// How much of its design capacity the pack still holds, as a percentage.
///
/// A pack that never reports the design pair has no health to give, and one
/// whose design charge is nonsense is treated the same way: the reading is
/// absent rather than 0% or 100%.
pub fn health_percent(packs: &[Pack]) -> Option<i32> {
    let mut full = 0i64;
    let mut design = 0i64;

    for pack in packs.iter().filter(|pack| pack.present) {
        let (_, pack_full) = pack.energy()?;
        let pack_full = pack_full.filter(|full| *full > 0)?;

        full += pack_full;
        design += pack.design_energy()?;
    }

    if design <= 0 {
        return None;
    }

    Some(((full.saturating_mul(100) / design).clamp(0, 100)) as i32)
}

/// The facts a reading can put in a tooltip, joined with the shell's own
/// separator.  A fact that is not known is left out rather than printed as a
/// dash: "2h 05m · 12.3 W" says more than "- · - · -".
pub fn detail(seconds: Option<i64>, power_microwatts: Option<i64>, health: Option<i32>) -> String {
    let mut facts = Vec::new();

    if seconds.filter(|seconds| *seconds > 0).is_some() {
        facts.push(format_seconds(seconds));
    }

    let watts = format_watts(power_microwatts);

    if watts != "-" {
        facts.push(watts);
    }

    if let Some(health) = health.filter(|health| *health > 0) {
        facts.push(format!("{health}% health"));
    }

    facts.join(" · ")
}

/// Read every battery in `root`, in the kernel's (sorted) order.
pub fn read_all(root: &Path) -> Vec<Pack> {
    let mut names = match fs::read_dir(root) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                read_trimmed(&path.join("type")).as_deref() == Some("Battery")
                    && path.join("capacity").is_file()
            })
            .filter_map(|path| path.file_name()?.to_str().map(str::to_owned))
            .collect::<Vec<String>>(),
        Err(_) => Vec::new(),
    };

    names.sort();

    names
        .iter()
        .filter_map(|name| read_pack(root, name))
        .collect()
}

/// Read one pack by name.
pub fn read_pack(root: &Path, name: &str) -> Option<Pack> {
    let directory = root.join(name);
    let capacity = read_trimmed(&directory.join("capacity"))?;

    Some(Pack {
        name: name.to_owned(),
        status: read_trimmed(&directory.join("status")).unwrap_or_else(|| "Unknown".to_owned()),
        capacity: capacity.parse::<i32>().unwrap_or(0).clamp(0, 100),
        present: read_trimmed(&directory.join("present"))
            .map(|value| value == "1")
            .unwrap_or(true),
        energy_now: read_number(&directory, "energy_now"),
        energy_full: read_number(&directory, "energy_full"),
        energy_full_design: read_number(&directory, "energy_full_design"),
        charge_now: read_number(&directory, "charge_now"),
        charge_full: read_number(&directory, "charge_full"),
        charge_full_design: read_number(&directory, "charge_full_design"),
        power_now: read_number(&directory, "power_now"),
        current_now: read_number(&directory, "current_now"),
        voltage_now: read_number(&directory, "voltage_now"),
    })
}

/// Whether the machine is plugged in: the first adapter's `online` flag, when
/// it reports one.
pub fn adapter_online_at(root: &Path) -> Option<bool> {
    let mut names = fs::read_dir(root)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| read_trimmed(&path.join("type")).as_deref() == Some("Mains"))
        .filter_map(|path| path.file_name()?.to_str().map(str::to_owned))
        .collect::<Vec<String>>();

    names.sort();

    names
        .iter()
        .find_map(|name| read_trimmed(&root.join(name).join("online")).map(|online| online == "1"))
}

/// The reading for `device`: a named pack, `"all"` for the machine, or the first
/// battery the kernel reports.
pub fn read_at(root: &Path, device: Option<&str>) -> Option<BatteryReading> {
    let packs = read_all(root);

    match device.filter(|value| !value.is_empty()) {
        Some(ALL_PACKS) => aggregate(&packs),
        Some(name) => read_pack(root, name).map(|pack| reading_of(&[pack])),
        None => packs
            .first()
            .map(|pack| reading_of(std::slice::from_ref(pack))),
    }
}

pub fn read(device: Option<&str>) -> Option<BatteryReading> {
    read_at(Path::new(POWER_SUPPLY_ROOT), device)
}

/// One reading from one or more packs.
///
/// One pack answers with its own numbers.  Several are summed the way a laptop
/// with two packs behaves: the percentages are weighted by capacity, the rate is
/// added up, and any pack that is charging makes the whole machine charge.
fn reading_of(packs: &[Pack]) -> BatteryReading {
    let present: Vec<&Pack> = packs.iter().filter(|pack| pack.present).collect();

    if present.is_empty() {
        return BatteryReading {
            name: packs
                .first()
                .map(|pack| pack.name.clone())
                .unwrap_or_default(),
            status: "Unknown".to_owned(),
            capacity: 0,
            present: false,
            seconds_remaining: None,
            power_microwatts: None,
            health_percent: None,
        };
    }

    let name = if present.len() == 1 {
        present[0].name.clone()
    } else {
        ALL_PACKS.to_owned()
    };
    let status = combined_status(&present);
    let capacity = combined_capacity(&present);
    let seconds_remaining = combined_seconds(&present);

    BatteryReading {
        name,
        status,
        capacity,
        present: true,
        seconds_remaining,
        power_microwatts: power_microwatts(packs),
        health_percent: health_percent(packs),
    }
}

/// `#:device "all"`: every pack the machine has.
pub fn aggregate(packs: &[Pack]) -> Option<BatteryReading> {
    if packs.is_empty() {
        return None;
    }

    Some(reading_of(packs))
}

/// What the right-click power card shows: the totals and one row per pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryCard {
    pub title: String,
    pub capacity: i32,
    pub state: String,
    /// The labelled facts, in the order the card paints them.
    pub facts: Vec<(String, String)>,
    /// One row per pack: its name, then what it is doing.
    pub packs: Vec<(String, String)>,
}

/// The card for `device`: the reading's totals, how much is drawn, how worn
/// the cells are and whether the machine is plugged in, with a row per pack
/// when more than one answered.
///
/// A machine with no present pack has no card: the reading is hidden, and a
/// card that says "0%" about nothing would be worse than no card at all.
pub fn card_at(root: &Path, device: Option<&str>) -> Option<BatteryCard> {
    let reading = read_at(root, device)?;

    if !reading.present {
        return None;
    }

    let packs = read_all(root);
    let chosen: Vec<Pack> = match device.filter(|value| !value.is_empty()) {
        Some(ALL_PACKS) => packs.clone(),
        Some(name) => packs.into_iter().filter(|pack| pack.name == name).collect(),
        None => packs.into_iter().take(1).collect(),
    };
    let health = reading
        .health_percent
        .map_or_else(|| "-".to_owned(), |health| format!("{health}%"));
    let mut facts = vec![
        ("State".to_owned(), reading.status.clone()),
        ("Time".to_owned(), format_seconds(reading.seconds_remaining)),
        ("Draw".to_owned(), format_watts(reading.power_microwatts)),
        ("Health".to_owned(), health),
    ];

    if let Some(online) = adapter_online_at(root) {
        facts.push((
            "Adapter".to_owned(),
            if online { "plugged in" } else { "on battery" }.to_owned(),
        ));
    }

    let rows = chosen
        .iter()
        .map(|pack| {
            (
                pack.name.clone(),
                format!("{}% · {}", pack.capacity, pack.status),
            )
        })
        .collect();

    Some(BatteryCard {
        title: if chosen.len() > 1 {
            "Batteries".to_owned()
        } else {
            "Battery".to_owned()
        },
        capacity: reading.capacity,
        state: reading.status,
        facts,
        packs: rows,
    })
}

fn combined_status(packs: &[&Pack]) -> String {
    if packs.iter().any(|pack| pack.is_charging()) {
        "Charging".to_owned()
    } else if packs.iter().any(|pack| pack.is_discharging()) {
        "Discharging".to_owned()
    } else if packs.iter().all(|pack| pack.status == "Full") {
        "Full".to_owned()
    } else {
        packs
            .first()
            .map(|pack| pack.status.clone())
            .unwrap_or_else(|| "Unknown".to_owned())
    }
}

/// Summed charge over summed capacity, which is what two packs of different
/// sizes actually report; a machine whose packs only give percentages falls back
/// to the mean, weighted by capacity when that is all there is.
fn combined_capacity(packs: &[&Pack]) -> i32 {
    let energy: Option<(i64, i64)> = packs
        .iter()
        .try_fold((0i64, 0i64), |(now, full), pack| {
            let (pack_now, pack_full) = pack.energy()?;
            let pack_full = pack_full.filter(|full| *full > 0)?;

            Some((now + pack_now, full + pack_full))
        })
        .filter(|(_, full)| *full > 0);

    if let Some((now, full)) = energy {
        return ((now.saturating_mul(100) / full).clamp(0, 100)) as i32;
    }

    let total: i32 = packs.iter().map(|pack| pack.capacity).sum();

    (total / packs.len() as i32).clamp(0, 100)
}

/// The summed rate and the summed charge, so the estimate is the machine's, not
/// one pack's.
fn combined_seconds(packs: &[&Pack]) -> Option<i64> {
    let charging = packs.iter().any(|pack| pack.is_charging());

    if !charging && !packs.iter().any(|pack| pack.is_discharging()) {
        return None;
    }

    let mut micros = 0i64;
    let mut rate = 0i64;

    for pack in packs {
        if !(pack.is_charging() || pack.is_discharging()) {
            continue;
        }

        let (now, full) = pack.energy()?;
        let pack_rate = pack.power().unwrap_or(0);

        // A pack that disagrees with the machine (one charging while the other
        // drains) contributes only in its own direction.
        if pack.is_charging() != charging {
            continue;
        }

        micros += if charging {
            full?.saturating_sub(now)
        } else {
            now
        };
        rate += pack_rate;
    }

    if micros <= 0 || rate <= 0 {
        return None;
    }

    Some(micros.saturating_mul(3_600) / rate)
}

fn read_number(directory: &Path, name: &str) -> Option<i64> {
    read_trimmed(&directory.join(name))?.parse::<i64>().ok()
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
}

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let module = module.clone();
    let device = module.text("device").filter(|value| !value.is_empty());
    let (root, host) = ModuleHost::new(
        &module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    host.add_label();

    if let Some(colour) = module.text("charging-color").as_deref().and_then(colour) {
        install_charging_colour(&root, &colour);
    }

    if host.settings().actions.secondary.is_none() {
        attach_card(&root, &context.bar, device.clone());
    }

    host.start(move |host| update(&module, device.as_deref(), host));

    Some(root)
}

fn update(module: &ModuleSpec, device: Option<&str>, host: &ModuleHost) {
    let Some(reading) = read(device) else {
        host.set_visible(false);
        host.set_text("");

        return;
    };

    if !reading.present {
        host.set_visible(false);
        host.set_text("");

        return;
    }

    host.set_visible(true);

    let warning = module.integer("warning").unwrap_or(30) as i32;
    let critical = module.integer("critical").unwrap_or(15) as i32;
    let charging = is_charging(&reading.status);
    let state_class = battery_class(reading.capacity, &reading.status, warning, critical);

    host.remove_class("warning");
    host.remove_class("critical");
    host.remove_class("charging");
    host.remove_class("full");

    if let Some(class) = state_class {
        host.add_class(class);
    }

    let capacity = reading.capacity.to_string();
    let time = format_seconds(reading.seconds_remaining);
    let power = format_watts(reading.power_microwatts);
    let health = reading
        .health_percent
        .map_or_else(|| "-".to_owned(), |health| format!("{health}%"));
    let detail = detail(
        reading.seconds_remaining,
        reading.power_microwatts,
        reading.health_percent,
    );
    let values = [
        ("capacity", capacity.as_str()),
        ("state", reading.status.as_str()),
        ("device", reading.name.as_str()),
        ("time", time.as_str()),
        ("power", power.as_str()),
        ("health", health.as_str()),
        ("detail", detail.as_str()),
    ];
    let mut format = module
        .text("format")
        .unwrap_or_else(|| "{icon} {capacity}%".to_owned());
    let mut icon = module.text("icon").unwrap_or_default();
    let charging_icon = module.text("charging-icon");

    if charging {
        if let Some(value) = charging_icon.as_deref() {
            icon = value.to_owned();
        }

        if let Some(value) = module.text("charging-format") {
            format = value;
        } else if !module
            .boolean("show-capacity-when-charging?")
            .unwrap_or(false)
        {
            format = "{icon}".to_owned();
        }
    }

    let tooltip = host
        .settings()
        .tooltip
        .as_deref()
        .unwrap_or(DEFAULT_TOOLTIP);

    host.present_with(&format, &icon, Some(tooltip), &values);
}

/// A colour from the configuration, normalised through GTK's own parser so a
/// value that is not a colour is a missing option rather than a stylesheet
/// injection.
fn colour(value: &str) -> Option<String> {
    gtk::gdk::RGBA::parse(value.trim())
        .ok()
        .map(|rgba| rgba.to_string())
}

/// Give this one battery reading its own class, so `#:charging-color` paints it
/// and not every battery module on the session's bars.
fn install_charging_colour(root: &gtk::Box, colour: &str) {
    static NEXT_INSTANCE: AtomicUsize = AtomicUsize::new(0);

    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let class = format!(
        "whelk-battery-tint-{}",
        NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed)
    );
    let provider = gtk::CssProvider::new();

    root.add_css_class(&class);
    provider.load_from_string(&format!(".{class}.charging {{ color: {colour}; }}\n"));
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    root.connect_destroy(move |_| {
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_remove_provider_for_display(&display, &provider);
        }
    });
}

/// The right-click power card: the pack's state and numbers, and whether the
/// machine is plugged in.  It is read fresh on every open, so a card is never a
/// stale copy of the bar beside it.
fn attach_card(root: &gtk::Box, bar: &BarSettings, device: Option<String>) {
    let popover = gtk::Popover::new();
    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);

    popover.set_has_arrow(false);
    popover.add_css_class("whelk-power-card");
    card.add_css_class("whelk-power-card-body");
    popover.set_child(Some(&card));
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
    let card_clone = card.clone();

    gesture.set_button(gtk::gdk::BUTTON_SECONDARY);
    gesture.connect_pressed(move |gesture, _, _, _| {
        gesture.set_state(gtk::EventSequenceState::Claimed);
        barmodule::close_popovers(Some(&popover_clone));

        let Some(model) = card_at(Path::new(POWER_SUPPLY_ROOT), device.as_deref()) else {
            return;
        };

        fill_card(&card_clone, &model);
        popover_clone.popup();
    });
    root.add_controller(gesture);
    root.set_cursor_from_name(Some("pointer"));
}

fn fill_card(card: &gtk::Box, model: &BatteryCard) {
    while let Some(child) = card.first_child() {
        card.remove(&child);
    }

    let title = gtk::Label::new(Some(&model.title));
    let percent = gtk::Label::new(Some(&format!("{}% · {}", model.capacity, model.state)));
    let level = gtk::LevelBar::for_interval(0.0, 100.0);
    let facts = gtk::Grid::new();

    title.add_css_class("whelk-power-card-title");
    title.set_halign(gtk::Align::Start);
    percent.add_css_class("whelk-power-card-percent");
    percent.set_halign(gtk::Align::Start);
    level.add_css_class("whelk-power-card-level");
    level.set_value(f64::from(model.capacity));
    level.set_hexpand(true);
    facts.add_css_class("whelk-power-card-facts");
    facts.set_column_spacing(12);
    facts.set_row_spacing(2);

    for (row, (label, value)) in model.facts.iter().enumerate() {
        let name = gtk::Label::new(Some(label));
        let answer = gtk::Label::new(Some(value));

        name.add_css_class("whelk-power-card-label");
        name.set_halign(gtk::Align::Start);
        answer.set_halign(gtk::Align::Start);
        facts.attach(&name, 0, row as i32, 1, 1);
        facts.attach(&answer, 1, row as i32, 1, 1);
    }

    card.append(&title);
    card.append(&percent);
    card.append(&level);
    card.append(&facts);

    if model.packs.len() > 1 {
        card.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        for (name, doing) in &model.packs {
            let row = gtk::Label::new(Some(&format!("{name}   {doing}")));

            row.add_css_class("whelk-power-card-pack");
            row.set_halign(gtk::Align::Start);
            card.append(&row);
        }
    }
}

/// Escape closes the card, so the pointer does not have to come back to the
/// reading to put it away.
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
    let mut css = CARD_CSS.to_owned();

    if let Some(foreground) = bar
        .foreground
        .as_deref()
        .filter(|foreground| !foreground.is_empty())
    {
        css.push_str(&format!(".whelk-power-card {{ color: {foreground}; }}\n"));
    }

    if let Some(background) = style::solid(bar.background.as_deref()) {
        css.push_str(&format!(
            ".whelk-power-card {{ background-color: {background}; }}\n"
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
