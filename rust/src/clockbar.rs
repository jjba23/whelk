// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The clock reading, ported from `modules/clock.c`.
//!
//! `#:format` is a strftime pattern, not a whelk format: the rendered time is
//! the label, while the tooltip is the month laid out as a calendar.

use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;

pub const DEFAULT_FORMAT: &str = "%a %d %b  %H:%M";
pub const DEFAULT_INTERVAL_SECONDS: u32 = 5;

/// The label's time.  A malformed strftime pattern renders empty, as the C
/// module's `g_date_time_format` answer does; the `--:--` fallback belongs to
/// the clock with no current time at all.
pub fn format_time(now: &gtk::glib::DateTime, pattern: &str) -> String {
    now.format(pattern)
        .map(|text| text.to_string())
        .unwrap_or_default()
}

/// The calendar in the C tooltip: a heading, Monday-first weekday columns, and
/// one line per week.
pub fn calendar(now: &gtk::glib::DateTime) -> String {
    let year = now.year();
    let month = now.month();
    let Ok(first) =
        gtk::glib::DateTime::new(&gtk::glib::TimeZone::local(), year, month, 1, 0, 0, 0.0)
    else {
        return String::new();
    };
    let Ok(next_month) = first.add_months(1) else {
        return String::new();
    };
    let Ok(last) = next_month.add_days(-1) else {
        return String::new();
    };
    let heading = now
        .format("%B %Y")
        .map(|text| text.to_string())
        .unwrap_or_default();
    let leading = first.day_of_week() - 1;
    let days = last.day_of_month();
    let mut output = format!("{heading}\nMo Tu We Th Fr Sa Su\n");

    output.push_str(&"   ".repeat(leading.max(0) as usize));

    for day in 1..=days {
        let sunday = (leading + day - 1) % 7 == 6;

        if day == days && !sunday {
            output.push_str(&format!("{day:>2}"));
        } else {
            output.push_str(&format!("{day:>2}{}", if sunday { "\n" } else { " " }));
        }
    }

    if !output.ends_with('\n') {
        output.push('\n');
    }

    output
}

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let (root, host) = ModuleHost::new(
        module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    host.add_label();
    host.start(update);

    Some(root)
}

fn update(host: &ModuleHost) {
    let Some(now) = gtk::glib::DateTime::now_local().ok() else {
        host.set_text("--:--");

        return;
    };
    let pattern = host.settings().format.as_deref().unwrap_or(DEFAULT_FORMAT);
    let time = format_time(&now, pattern);
    let date = format_time(&now, "%A %d %B %Y");
    let calendar = calendar(&now);
    let icon = host.settings().icon.as_deref().unwrap_or("");
    let format = if icon.is_empty() {
        "{time}"
    } else {
        "{icon} {time}"
    };
    let values = [
        ("time", time.as_str()),
        ("date", date.as_str()),
        ("calendar", calendar.as_str()),
    ];

    host.present_with(format, icon, host.settings().tooltip.as_deref(), &values);
}
