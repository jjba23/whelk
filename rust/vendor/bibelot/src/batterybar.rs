// bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT

//! The battery reading behind the battery card: `sysfs`, one pack or all.

#![allow(dead_code)]

use std::fs;
use std::path::Path;

const POWER_SUPPLY_ROOT: &str = "/sys/class/power_supply";
/// `#:device "all"` sums every pack in the machine.
pub const ALL_PACKS: &str = "all";

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
    read_trimmed(&directory.join(name))?.parse().ok()
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
}
