// The policy behind the desktop widget cards, tested without a display: the
// monitor a drag may cross, the clock hands' angles, the battery card's words
// and classes, and the markup a reminder row is drawn with.

use whelk_rs::desktopwidgets::{self, FetchError, Health};
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

#[test]
fn a_missing_curl_is_named_instead_of_saying_unavailable() {
    // "unavailable" does not tell anyone what to fix; a missing curl does, and
    // it is the one failure the user can fix without a network.
    let (headline, hint) =
        desktopwidgets::failure_lines(desktopwidgets::FetchError::NoCurl, "Weather");

    assert_eq!(headline, "curl is not installed");
    assert_eq!(hint, "install curl, or put it on PATH");

    // The stocks card names the same fault, and its card has no second line.
    let (headline, _) = desktopwidgets::failure_lines(desktopwidgets::FetchError::NoCurl, "Quotes");

    assert_eq!(headline, "curl is not installed");
}

#[test]
fn a_card_that_never_loaded_is_failed_with_its_last_try() {
    let health = Health::after(None, Err(FetchError::Failed), 40);

    assert_eq!(
        health,
        Health::Failed {
            error: FetchError::Failed,
            at: 40
        }
    );
    assert_eq!(health.age(90), None, "nothing has been read yet");
    assert_eq!(
        health.quiet_age(90_000, 300),
        None,
        "there is no age to show"
    );
    assert_eq!(
        health.tooltip("Weather", 100).as_deref(),
        Some("Weather unavailable\nfailed 1 min ago")
    );
}

#[test]
fn a_failed_refresh_keeps_the_reading_and_the_error_apart() {
    let ready = Health::after(None, Ok(()), 100);

    assert_eq!(ready, Health::Ready { at: 100 });
    assert_eq!(ready.tooltip("Weather", 100), None);
    assert_eq!(ready.quiet_age(100, 300), None);

    let stale = Health::after(Some(ready), Err(FetchError::NoCurl), 700);

    assert_eq!(
        stale,
        Health::Stale {
            since: 100,
            error: FetchError::NoCurl
        }
    );
    assert_eq!(stale.age(700), Some(600), "the reading keeps its own time");
    // Two intervals is the line: one missed tick says nothing, and the count
    // keeps moving because the timestamp is the reading's, not the failure's.
    assert_eq!(stale.quiet_age(699, 300), None);
    assert_eq!(stale.quiet_age(701, 300).as_deref(), Some("10 min ago"));
    assert_eq!(stale.quiet_age(2_500, 300).as_deref(), Some("40 min ago"));
    assert_eq!(
        stale.tooltip("Weather", 700).as_deref(),
        Some("curl is not installed\ninstall curl, or put it on PATH")
    );
}

#[test]
fn a_late_answer_forgets_the_error_it_answered_over() {
    let stale = Health::Stale {
        since: 100,
        error: FetchError::Failed,
    };
    let ready = Health::after(Some(stale), Ok(()), 800);

    assert_eq!(ready, Health::Ready { at: 800 });
    assert_eq!(ready.tooltip("Quotes", 800), None);
    assert_eq!(ready.quiet_age(800, 300), None);
}

#[test]
fn a_card_still_failing_remembers_the_last_try_not_the_first() {
    let first = Health::after(None, Err(FetchError::Failed), 10);
    let second = Health::after(Some(first), Err(FetchError::NoCurl), 500);

    assert_eq!(
        second,
        Health::Failed {
            error: FetchError::NoCurl,
            at: 500
        }
    );
    assert!(
        second
            .tooltip("Quotes", 560)
            .is_some_and(|text| text.contains("failed 1 min ago")),
        "the title should date the last attempt"
    );
}

#[test]
fn an_age_is_read_in_whole_minutes_and_never_as_zero() {
    assert_eq!(desktopwidgets::age_text(5), "1 min ago");
    assert_eq!(desktopwidgets::age_text(60), "1 min ago");
    assert_eq!(desktopwidgets::age_text(3_600), "60 min ago");
}

#[test]
fn an_ordinary_failed_read_keeps_the_old_wording() {
    for (what, expected) in [
        ("Weather", "Weather unavailable"),
        ("Quotes", "Quotes unavailable"),
    ] {
        let (headline, hint) =
            desktopwidgets::failure_lines(desktopwidgets::FetchError::Failed, what);

        assert_eq!(headline, expected);
        assert_eq!(hint, "", "a network failure has no advice to give");
    }
}
