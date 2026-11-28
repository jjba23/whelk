// bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Desktop widget runtime state, ported from `util/widgetstate.c`.
//!
//! Where a widget was dragged to, and the small pieces of a widget that change
//! without a settings save (today: which reminders are done).  This is runtime
//! state, not configuration: it lives under `XDG_STATE_HOME`, one file per
//! settings file, and deleting it returns every widget to the place its
//! settings file gave it.
//!
//! The file is a `GKeyFile`, the same format the C shell writes, so both
//! shells read each other's positions and a switch from one to the other does
//! not move anybody's desktop.  A line that cannot be parsed costs one value
//! and not the rest of the file.

use std::path::{Path, PathBuf};

use gtk4 as gtk;

use gtk::glib;

/// The grid a dragged widget snaps to, in pixels.
pub const GRID: i32 = 8;

const POSITION_GROUP: &str = "positions";
const STATE_GROUP: &str = "state";

/// The state file for the widgets a settings file describes:
/// `$XDG_STATE_HOME/whelk/widgets-<settings-stem>.ini`, or
/// `~/.local/state/whelk/...` when `XDG_STATE_HOME` is unset.  A settings path
/// with no file name has no state file to key it by.
pub fn state_path(config_path: &Path) -> Option<PathBuf> {
    let state_home = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from);
    let home = glib::home_dir();

    state_path_in(
        config_path,
        state_home.as_deref(),
        (!home.as_os_str().is_empty()).then_some(home.as_path()),
    )
}

/// The same rule with the environment passed in, so the directory choice is a
/// value a test can state instead of a race over the process environment.
pub fn state_path_in(
    config_path: &Path,
    state_home: Option<&Path>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    let name = config_path.file_name()?.to_str()?;

    if name.is_empty() {
        return None;
    }

    let stem = name.strip_suffix(".scm").unwrap_or(name);
    let directory = match state_home.filter(|path| !path.as_os_str().is_empty()) {
        Some(path) => path.join("whelk"),
        None => home?
            .to_path_buf()
            .join(".local")
            .join("state")
            .join("whelk"),
    };

    Some(directory.join(format!("widgets-{stem}.ini")))
}

/// Round `value` to the nearest multiple of `grid`, never below zero.  A grid
/// of zero or less means no snapping.
pub fn snap(value: i32, grid: i32) -> i32 {
    if value <= 0 {
        return 0;
    }

    if grid <= 0 {
        return value;
    }

    ((value + grid / 2) / grid) * grid
}

/// Keep a `width` x `height` widget inside an `area_width` x `area_height`
/// monitor.  A widget larger than the monitor is pinned to the top-left corner
/// rather than given a negative position.
pub fn clamp(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    area_width: i32,
    area_height: i32,
) -> (i32, i32) {
    let max_x = (area_width - width).max(0);
    let max_y = (area_height - height).max(0);

    (x.clamp(0, max_x), y.clamp(0, max_y))
}

/// The state file, or `None` when it is missing (the first run) or unreadable.
/// A missing file is not an error; an unreadable one is, and it leaves every
/// widget where its settings file put it.
fn load(path: &Path) -> Option<glib::KeyFile> {
    let file = glib::KeyFile::new();

    match file.load_from_file(path, glib::KeyFileFlags::NONE) {
        Ok(()) => Some(file),
        Err(error) if error.matches(glib::FileError::Noent) => Some(file),
        Err(_) => None,
    }
}

/// Create the directory mode `0700`, as the C shell does: a widget placement
/// is nobody else's business.
fn save(path: &Path, file: &glib::KeyFile) -> bool {
    let Some(directory) = path.parent() else {
        return false;
    };
    let mut builder = std::fs::DirBuilder::new();

    builder.recursive(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;

        builder.mode(0o700);
    }

    if builder.create(directory).is_err() {
        return false;
    }

    let data = file.to_data();

    std::fs::write(path, data.as_bytes()).is_ok()
}

/// A position is `"x,y"`, and a negative or malformed one is refused.
fn parse_position(value: &str) -> Option<(i32, i32)> {
    let (x, y) = value.split_once(',')?;
    let x: i32 = x.parse().ok()?;
    let y: i32 = y.parse().ok()?;

    if x < 0 || y < 0 {
        return None;
    }

    Some((x, y))
}

fn position_at(file: &glib::KeyFile, key: &str) -> Option<(i32, i32)> {
    let value = file.string(POSITION_GROUP, key).ok()?;

    parse_position(&value)
}

/// The key a placement is stored under.  `"default"` is how a settings file
/// says "no monitor in particular", so it is the monitor-less key too.
fn position_key(id: &str, monitor: Option<&str>) -> String {
    match monitor {
        Some(monitor) if !monitor.is_empty() && monitor != "default" => {
            format!("{id}@{monitor}")
        }
        _ => id.to_owned(),
    }
}

fn state_key(id: &str, key: &str) -> String {
    format!("{id}.{key}")
}

/// A widget's saved top-left corner, monitor-less.
pub fn load_position(path: &Path, id: &str) -> Option<(i32, i32)> {
    load_position_for(path, id, None)
}

/// The monitor-aware half of the placement memory.  A widget remembers where
/// it was dragged on each monitor it has lived on, and falls back to the
/// monitor-less position when the named one has none -- a monitor that is
/// gone, or a placement saved before any monitor was recorded.
pub fn load_position_for(path: &Path, id: &str, monitor: Option<&str>) -> Option<(i32, i32)> {
    let file = load(path)?;
    let key = position_key(id, monitor);

    position_at(&file, &key).or_else(|| (key != id).then(|| position_at(&file, id)).flatten())
}

/// Save a widget's top-left corner without naming a monitor.
pub fn save_position(path: &Path, id: &str, x: i32, y: i32) -> bool {
    save_position_for(path, id, None, x, y)
}

/// Save the position twice: under the monitor, and under the widget alone.
/// The second is the fallback that keeps a widget on screen when its monitor
/// is unplugged, so the monitor's own memory is not lost either.
pub fn save_position_for(path: &Path, id: &str, monitor: Option<&str>, x: i32, y: i32) -> bool {
    if x < 0 || y < 0 {
        return false;
    }

    let Some(file) = load(path) else {
        return false;
    };
    let value = format!("{x},{y}");

    if let Some(monitor) = monitor
        && !monitor.is_empty()
        && monitor != "default"
    {
        file.set_string(POSITION_GROUP, &position_key(id, Some(monitor)), &value);
    }

    file.set_string(POSITION_GROUP, id, &value);
    save(path, &file)
}

/// Read one string of per-widget state.
pub fn get(path: &Path, id: &str, key: &str) -> Option<String> {
    let file = load(path)?;

    file.string(STATE_GROUP, &state_key(id, key))
        .ok()
        .map(|value| value.to_string())
}

/// Write, or with `None` remove, one string of per-widget state.
pub fn set(path: &Path, id: &str, key: &str, value: Option<&str>) -> bool {
    let Some(file) = load(path) else {
        return false;
    };
    let name = state_key(id, key);

    match value {
        Some(value) => file.set_string(STATE_GROUP, &name, value),
        None => {
            let _ = file.remove_key(STATE_GROUP, &name);
        }
    }

    save(path, &file)
}
