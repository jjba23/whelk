// The policy behind the desktop widget cards, tested without a display: the
// monitor a drag may cross, the clock hands' angles, the battery card's words
// and classes, and the markup a reminder row is drawn with.

use whelk_rs::desktopwidgets;
use whelk_rs::widgets::{Area, FALLBACK_AREA, ReminderRow};

fn monitor(connector: &str, width: i32, height: i32) -> desktopwidgets::MonitorArea {
    desktopwidgets::MonitorArea {
        connector: connector.to_owned(),
        area: Area { width, height },
    }
}

#[test]
fn a_card_is_dragged_across_the_monitor_it_belongs_to() {
    let monitors = [monitor("DP-1", 2560, 1440), monitor("HDMI-1", 1920, 1080)];

    // The settings file names the connector: that is the area, even when the
    // surface happens to sit somewhere else for a moment.
    assert_eq!(
        desktopwidgets::area_for("HDMI-1", Some("DP-1"), &monitors),
        Area {
            width: 1920,
            height: 1080
        }
    );
}

#[test]
fn a_named_monitor_that_is_gone_falls_back_to_the_surface() {
    let monitors = [monitor("DP-1", 2560, 1440), monitor("HDMI-1", 1920, 1080)];

    assert_eq!(
        desktopwidgets::area_for("DP-9", Some("DP-1"), &monitors),
        Area {
            width: 2560,
            height: 1440
        },
        "a saved connector from another session must not trap the card"
    );
}

#[test]
fn a_card_on_the_default_monitor_uses_the_one_its_surface_is_on() {
    let monitors = [monitor("DP-1", 2560, 1440), monitor("HDMI-1", 1920, 1080)];

    assert_eq!(
        desktopwidgets::area_for("default", Some("HDMI-1"), &monitors),
        Area {
            width: 1920,
            height: 1080
        }
    );
}

#[test]
fn a_card_with_no_monitor_model_uses_the_first_one_then_the_fallback() {
    let monitors = [monitor("DP-1", 2560, 1440)];

    assert_eq!(
        desktopwidgets::area_for("default", None, &monitors),
        Area {
            width: 2560,
            height: 1440
        }
    );
    assert_eq!(
        desktopwidgets::area_for("default", None, &[]),
        FALLBACK_AREA,
        "the headless renderer has no monitor model"
    );
}

#[test]
fn the_clock_hands_point_at_the_time() {
    // Midnight and noon are both straight up; three o'clock is straight right.
    let noon = desktopwidgets::clock_angles(12, 0, 0);

    assert!((noon.hour + std::f64::consts::FRAC_PI_2).abs() < 1e-9);

    let three = desktopwidgets::clock_angles(3, 0, 0);

    assert!(three.hour.abs() < 1e-9);
    assert!((three.minute + std::f64::consts::FRAC_PI_2).abs() < 1e-9);
}

#[test]
fn the_clock_hands_creep_between_whole_units() {
    // Half past six: the hour hand sits between six and seven, the minute
    // hand is straight down.
    let angles = desktopwidgets::clock_angles(6, 30, 0);

    assert!((angles.hour - 7.0 * std::f64::consts::PI / 12.0).abs() < 1e-9);
    assert!((angles.minute - std::f64::consts::FRAC_PI_2).abs() < 1e-9);

    // Fifteen seconds past a minute 30 seconds in.
    let angles = desktopwidgets::clock_angles(1, 30, 15);

    assert!(
        (angles.minute - (30.25 * std::f64::consts::PI / 30.0 - std::f64::consts::FRAC_PI_2)).abs()
            < 1e-9
    );
    assert!(
        (angles.second - (15.0 * std::f64::consts::PI / 30.0 - std::f64::consts::FRAC_PI_2)).abs()
            < 1e-9
    );
}

#[test]
fn the_battery_card_says_what_the_pack_does() {
    let critical = desktopwidgets::battery_summary(12, "Discharging", 30, 15);

    assert_eq!(critical.percent, "12%");
    assert_eq!(critical.status, "Discharging · on battery");
    assert_eq!(critical.classes, vec!["critical"]);

    let warning = desktopwidgets::battery_summary(25, "Charging", 30, 15);

    assert_eq!(warning.status, "Charging · charging");
    assert_eq!(warning.classes, vec!["warning", "charging"]);

    let healthy = desktopwidgets::battery_summary(80, "Full", 30, 15);

    assert_eq!(healthy.classes, vec!["charging"]);
    assert_eq!(
        desktopwidgets::battery_summary(80, "Discharging", 30, 15).classes,
        Vec::<&str>::new()
    );
}

#[test]
fn a_reminder_row_is_drawn_struck_through_and_escaped() {
    let done = desktopwidgets::reminder_markup(&ReminderRow {
        text: "Ship <it> & rest".to_owned(),
        done: true,
    });

    assert_eq!(done, "<s>Ship &lt;it&gt; &amp; rest</s>");

    let pending = desktopwidgets::reminder_markup(&ReminderRow {
        text: "Ship <it> & rest".to_owned(),
        done: false,
    });

    assert_eq!(pending, "Ship &lt;it&gt; &amp; rest");
}

#[test]
fn a_reminder_marker_in_the_text_is_escaped_too() {
    assert_eq!(
        desktopwidgets::reminder_markup(&ReminderRow {
            text: "a > b".to_owned(),
            done: false,
        }),
        "a &gt; b"
    );
}
