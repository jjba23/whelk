// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The launcher's searchable data, separated from its GTK overlay.
//!
//! The C launcher builds eight little databases: desktop entries, PATH
//! commands, niri windows, file matches, SSH hosts, menu rows, session actions
//! and the built-in session menu.  Keeping the parsing and ranking here means
//! those rules can be checked without a display, while `launcherpanel` only
//! paints their result and starts the chosen action.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::filesearch;
use crate::fuzzy;
use crate::json::Json;

pub const RESULT_LIMIT: usize = 40;
pub const DEFAULT_WEB_SEARCH: &str = "https://duckduckgo.com/?q=";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    App,
    Window,
    Command,
    File,
    Web,
    Calc,
    Menu,
    Script,
    Ssh,
    Quit,
    Clipboard,
    Session,
    /// A row from the focused application's live menubar.
    MenuItem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherItem {
    pub kind: ItemKind,
    pub name: String,
    pub detail: String,
    pub icon: Option<String>,
    /// The lowercase text fuzzy matching sees.
    pub search: String,
    /// A desktop path, window id, command, path, query or script label.
    pub action: String,
    /// The stable clipboard entry id for a clipboard row.
    pub clipboard_id: Option<String>,
    /// A pre-scaled PNG thumbnail for an image clipboard row.
    pub preview: Option<Vec<u8>>,
    pub disabled: bool,
}

impl LauncherItem {
    pub fn new(kind: ItemKind, name: impl Into<String>, detail: impl Into<String>) -> Self {
        let name = name.into();

        Self {
            kind,
            search: name.to_ascii_lowercase(),
            name,
            detail: detail.into(),
            icon: None,
            action: String::new(),
            clipboard_id: None,
            preview: None,
            disabled: false,
        }
    }

    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = action.into();

        self
    }

    pub fn with_icon(mut self, icon: Option<&str>) -> Self {
        self.icon = icon.filter(|icon| !icon.is_empty()).map(str::to_owned);

        self
    }

    pub fn with_clipboard_id(mut self, id: impl Into<String>) -> Self {
        self.clipboard_id = Some(id.into());

        self
    }

    pub fn with_preview(mut self, preview: Option<Vec<u8>>) -> Self {
        self.preview = preview;

        self
    }

    pub fn add_search(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        self.search.push(' ');
        self.search.push_str(&text.to_ascii_lowercase());
    }
}

/// The XDG application directories, in the order a desktop id is looked up.
/// The user directory wins over every system directory.
///
/// The XDG variables name data directories, and desktop entries live in the
/// `applications` subdirectory of each one.  Scanning the data directory itself
/// finds no entry at all: `/usr/share` holds no `.desktop` file, only
/// `/usr/share/applications` does.
pub fn desktop_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    let user = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"));
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());

    let mut directories = vec![user];

    directories.extend(
        system
            .split(':')
            .filter(|part| !part.is_empty())
            .map(PathBuf::from),
    );

    // The session's list first, then the roots a compositor tends not to hand a
    // shell.  A directory that is already named is not repeated, so a session
    // that does export the profile still decides the order.
    for directory in well_known_data_dirs(&home) {
        if !directories.iter().any(|named| named == &directory) {
            directories.push(directory);
        }
    }

    directories
        .into_iter()
        .map(|directory| directory.join("applications"))
        .collect()
}

/// The application roots a session may not have put in `XDG_DATA_DIRS`.
///
/// whelk is started by a compositor, and a compositor's environment is as small
/// as its startup file made it: Guix, Nix, Flatpak and Snap each keep their own
/// applications directory, and a shell that only read the inherited list found
/// a fraction of what is installed.  These follow the session's directories, so
/// the user's own entries and the session's precedence are untouched.
fn well_known_data_dirs(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".guix-profile/share"),
        home.join(".nix-profile/share"),
        PathBuf::from("/run/current-system/sw/share"),
        PathBuf::from("/nix/var/nix/profiles/default/share"),
        home.join(".local/share/flatpak/exports/share"),
        PathBuf::from("/var/lib/flatpak/exports/share"),
        PathBuf::from("/var/lib/snapd/desktop"),
    ]
}

fn desktop_bool(value: Option<&String>) -> bool {
    value.is_some_and(|value| {
        let value = value.trim();

        value.eq_ignore_ascii_case("true") || value == "1"
    })
}

/// Unescape the escapes a GKeyFile knows in a desktop-entry string.  A desktop
/// file is not a shell script: only the documented escapes are interpreted.
pub fn desktop_unescape(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut characters = value.chars();

    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }

        match characters.next() {
            Some('n') => output.push('\n'),
            Some('t') => output.push('\t'),
            Some('r') => output.push('\r'),
            Some('s') => output.push(' '),
            Some('\\') => output.push('\\'),
            Some(';') => output.push(';'),
            Some(other) => output.push(other),
            None => output.push('\\'),
        }
    }

    output
}

/// What a `.desktop` file says about the id it is stored under.
///
/// Three answers, and the third is the one a scan usually gets wrong: a file
/// that hides its id has *claimed* that id, so a later directory must not answer
/// for it, while a file that is merely unusable has not, so the next directory
/// may still provide the application.  Collapsing both into "skip" either
/// resurrects an application the user deleted or loses one because a single
/// file is malformed.
enum Verdict {
    /// A launchable application.  Boxed because the row is much larger than
    /// the other two answers, which are read far more often than a row is.
    Shown(Box<LauncherItem>),
    /// `Hidden=true` or `NoDisplay=true`: the id is deleted at this level.
    Hidden,
    /// Not an application, or nothing usable in it.
    Unusable,
}

/// The locale the session asks for: `LC_ALL`, then `LC_MESSAGES`, then `LANG`.
///
/// A desktop file carries translated keys beside the untranslated ones, and the
/// first variable a session sets is the one it means, so they are not merged.
pub fn session_locale() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
        .unwrap_or_default()
}

/// The `Name[...]` suffixes to try for a locale, most specific first.
///
/// GLib's order is kept -- the country form with encoding and modifier, the
/// country form alone, then the language -- because that is the order a desktop
/// file's own translations were written for.  `C` and `POSIX` name no language,
/// so they produce nothing and the plain key answers.
pub fn locale_keys(locale: &str) -> Vec<String> {
    let locale = locale.trim();

    if locale.is_empty() || locale == "C" || locale == "POSIX" {
        return Vec::new();
    }

    let (body, modifier) = match locale.split_once('@') {
        Some((body, modifier)) => (body, Some(modifier)),
        None => (locale, None),
    };
    let (body, encoding) = match body.split_once('.') {
        Some((body, encoding)) => (body, Some(encoding)),
        None => (body, None),
    };
    let (language, country) = match body.split_once('_') {
        Some((language, country)) => (language, Some(country)),
        None => (body, None),
    };
    let mut keys = Vec::new();

    if let Some(country) = country {
        let base = format!("{language}_{country}");

        if let Some(encoding) = encoding {
            if let Some(modifier) = modifier {
                keys.push(format!("{base}.{encoding}@{modifier}"));
            }

            keys.push(format!("{base}.{encoding}"));
        }

        if let Some(modifier) = modifier {
            keys.push(format!("{base}@{modifier}"));
        }

        keys.push(base);
    }

    if let Some(modifier) = modifier {
        keys.push(format!("{language}@{modifier}"));
    }

    keys.push(language.to_owned());
    keys
}

/// The value of a localised key, falling back to the untranslated one.
///
/// An empty translation is not an answer -- a file with `Name[de]=` beside
/// `Name=Firefox` should read "Firefox" -- so the search continues past it.
fn localized<'a>(
    values: &'a HashMap<String, String>,
    key: &str,
    languages: &[String],
) -> Option<&'a String> {
    languages
        .iter()
        .filter_map(|language| values.get(&format!("{key}[{language}]")))
        .find(|value| !value.trim().is_empty())
        .or_else(|| values.get(key))
}

/// Read a `.desktop` file as text, whatever bytes it holds.
///
/// `fs::read_to_string` refuses a file that is not valid UTF-8, and a scan that
/// drops the whole file loses an application every other menu shows, because one
/// translated `Comment` was written in latin-1.  A byte-order mark is legal at
/// the start of the file too, and it is not part of the first section's name:
/// read verbatim, the header becomes `\u{feff}[Desktop Entry]` and the entry is
/// invisible.  The bytes are therefore stripped of the mark and decoded
/// lossily, which costs one mangled character instead of one missing app.
pub fn read_desktop_file(path: &Path) -> Option<String> {
    Some(desktop_text(&fs::read(path).ok()?))
}

fn desktop_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);

    String::from_utf8_lossy(bytes).into_owned()
}

fn entry_values(text: &str) -> HashMap<String, String> {
    let mut in_entry = false;
    let mut values = HashMap::<String, String>::new();

    // A mark that survived the byte-level strip (a caller passed text it had
    // already decoded) is not part of the key either.
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }

        if !in_entry {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            values.insert(key.trim().to_owned(), desktop_unescape(value.trim()));
        }
    }

    values
}

fn verdict(text: &str, filename: &str, path: &Path, languages: &[String]) -> Verdict {
    let values = entry_values(text);

    if desktop_bool(values.get("NoDisplay")) || desktop_bool(values.get("Hidden")) {
        return Verdict::Hidden;
    }

    if values.get("Type").map(String::as_str) != Some("Application") {
        return Verdict::Unusable;
    }

    let Some(name) = localized(&values, "Name", languages).map(|name| name.trim()) else {
        return Verdict::Unusable;
    };

    if name.is_empty() {
        return Verdict::Unusable;
    }

    let detail = localized(&values, "GenericName", languages)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .or_else(|| values.get("Exec").map(String::as_str))
        .unwrap_or("");
    let mut item = LauncherItem::new(ItemKind::App, name, detail)
        .with_action(path.to_string_lossy().into_owned())
        .with_icon(values.get("Icon").map(String::as_str));

    for key in ["GenericName", "Comment", "Keywords"] {
        if let Some(value) = localized(&values, key, languages) {
            item.add_search(value);
        }
    }

    item.add_search(filename);

    Verdict::Shown(Box::new(item))
}

/// Parse the `[Desktop Entry]` section of a `.desktop` file.  Directories and
/// hidden entries are not launchable things; a missing `Name` is likewise not
/// an application.
pub fn parse_desktop_entry(text: &str, filename: &str, path: &Path) -> Option<LauncherItem> {
    parse_desktop_entry_in_locale(text, filename, path, &session_locale())
}

/// Parse an entry as a session with this locale would read it.
///
/// The locale comes from the caller so the decision is testable without
/// changing the environment of the test process, and so a picker that lists
/// another language can ask for that language directly.
pub fn parse_desktop_entry_in_locale(
    text: &str,
    filename: &str,
    path: &Path,
    locale: &str,
) -> Option<LauncherItem> {
    match verdict(text, filename, path, &locale_keys(locale)) {
        Verdict::Shown(item) => Some(*item),
        Verdict::Hidden | Verdict::Unusable => None,
    }
}

/// Scan application directories once per directory, first filename wins.  The
/// filename is the desktop id, so the user's copy of `firefox.desktop` shadows
/// the system copy.
///
/// "Wins" is decided by the file, not by whether it produced a row: `Hidden=true`
/// and `NoDisplay=true` are how a user deletes an entry a package installed, so
/// such a file claims its id and the copy further down the search path must not
/// bring the application back.  A file that is unreadable, is not an
/// application, or has no name claims nothing, and the next directory is asked.
pub fn scan_desktop_dirs(directories: &[PathBuf]) -> Vec<LauncherItem> {
    let languages = locale_keys(&session_locale());
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for directory in directories {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };

        let mut files = entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .filter(|name| name.to_string_lossy().ends_with(".desktop"))
            .collect::<Vec<_>>();

        files.sort();

        for name in files {
            let name = name.to_string_lossy().into_owned();

            if seen.contains(&name) {
                continue;
            }

            let path = directory.join(&name);
            let Some(text) = read_desktop_file(&path) else {
                continue;
            };

            match verdict(&text, &name, &path, &languages) {
                Verdict::Shown(item) => {
                    seen.insert(name);
                    items.push(*item);
                }
                Verdict::Hidden => {
                    seen.insert(name);
                }
                Verdict::Unusable => {}
            }
        }
    }

    items
}

pub fn load_apps() -> Vec<LauncherItem> {
    scan_desktop_dirs(&desktop_dirs())
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

pub fn path_dirs(path: &str) -> Vec<PathBuf> {
    path.split(':')
        .filter(|part| !part.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Every executable on PATH, once, in PATH order.  A command's detail is the
/// directory it came from, which is what tells two same-named programs apart.
pub fn scan_command_dirs(directories: &[PathBuf]) -> Vec<LauncherItem> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for directory in directories {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };

        let mut files = entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .collect::<Vec<_>>();

        files.sort();

        for name in files {
            let name = name.to_string_lossy().into_owned();

            if seen.contains(&name) {
                continue;
            }

            let path = directory.join(&name);

            if !is_executable(&path) {
                continue;
            }

            seen.insert(name.clone());
            items.push(
                LauncherItem::new(ItemKind::Command, &name, directory.to_string_lossy())
                    .with_action(name),
            );
        }
    }

    items
}

pub fn load_commands() -> Vec<LauncherItem> {
    std::env::var("PATH")
        .map(|path| scan_command_dirs(&path_dirs(&path)))
        .unwrap_or_default()
}

/// The literal `Host` names from `~/.ssh/config`, without patterns.  ssh reads
/// one or more names per line; a pattern is a rule, not a host.
pub fn parse_ssh_hosts(text: &str) -> Vec<LauncherItem> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line
            .strip_prefix("Host ")
            .or_else(|| line.strip_prefix("Host\t"))
        else {
            continue;
        };

        for host in rest.split([' ', '\t']) {
            if host.is_empty() || host.contains(['*', '?', '!']) || !seen.insert(host.to_owned()) {
                continue;
            }

            items.push(
                LauncherItem::new(ItemKind::Ssh, host, "ssh")
                    .with_action(host)
                    .with_icon(Some("utilities-terminal-symbolic")),
            );
        }
    }

    items
}

pub fn load_ssh_hosts() -> Vec<LauncherItem> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let path = PathBuf::from(home).join(".ssh/config");

    fs::read_to_string(path)
        .map(|text| parse_ssh_hosts(&text))
        .unwrap_or_default()
}

/// Parse `niri msg -j windows` into rows.  The C launcher only knows niri, so
/// the Rust launcher keeps that contract while the rest of the shell retains
/// its multi-session probes.
pub fn parse_niri_windows(text: &str, quit: bool) -> Vec<LauncherItem> {
    let Ok(root) = Json::parse(text) else {
        return Vec::new();
    };
    let mut items = Vec::new();

    for window in root.array().unwrap_or_default() {
        let title = window.get_str("title").unwrap_or("");
        let id = window.get_number("id", 0.0);

        if id <= 0.0 || title.is_empty() {
            continue;
        }

        let app_id = window.get_str("app_id").unwrap_or("window");
        let kind = if quit {
            ItemKind::Quit
        } else {
            ItemKind::Window
        };
        let mut item = LauncherItem::new(kind, title, app_id)
            .with_action(format!("{id:.0}"))
            .with_icon(Some(if quit {
                "window-close-symbolic"
            } else {
                "focus-windows-symbolic"
            }));

        item.add_search(app_id);
        items.push(item);
    }

    items
}

fn parse_menu_row(row: &[u8]) -> Option<LauncherItem> {
    let nul = row.iter().position(|byte| *byte == 0);
    let label_end = nul.unwrap_or(row.len());
    let label = String::from_utf8_lossy(&row[..label_end]).into_owned();

    if label.is_empty() {
        return None;
    }

    if nul.is_none() {
        let line = String::from_utf8_lossy(row).into_owned();
        let mut parts = line.splitn(2, '\t');
        let label = parts.next().unwrap_or("");

        if label.is_empty() {
            return None;
        }

        let action = parts.next().unwrap_or(label);

        return Some(
            LauncherItem::new(ItemKind::Menu, label, action)
                .with_action(action)
                .with_icon(Some("application-x-executable-symbolic")),
        );
    }

    let mut item = LauncherItem::new(ItemKind::Script, label.clone(), "menu")
        .with_action(label)
        .with_icon(Some("application-x-executable-symbolic"));
    let mut cursor = label_end + 1;

    while cursor < row.len() {
        let end = row[cursor..]
            .iter()
            .position(|byte| *byte == 0)
            .map(|offset| cursor + offset)
            .unwrap_or(row.len());
        let field = String::from_utf8_lossy(&row[cursor..end]).into_owned();

        if let Some((key, value)) = field.split_once('\u{1f}') {
            match key {
                "icon" if !value.is_empty() => item.icon = Some(value.to_owned()),
                "disabled" if value == "true" => item.disabled = true,
                _ => {}
            }
        }

        cursor = end.saturating_add(1);
    }

    if item.disabled && item.name.starts_with('─') {
        return None;
    }

    Some(item)
}

/// Parse the two menu generator shapes.  A row is either `label<TAB>command` or
/// the NUL-separated script protocol with `icon`/`disabled` fields.
pub fn parse_menu_bytes(bytes: &[u8]) -> Vec<LauncherItem> {
    let mut items = Vec::new();
    let mut start = 0;

    for index in 0..=bytes.len() {
        if index != bytes.len() && bytes[index] != b'\n' {
            continue;
        }

        let mut end = index;

        if end > start && bytes[end - 1] == b'\r' {
            end -= 1;
        }

        if end > start
            && let Some(item) = parse_menu_row(&bytes[start..end])
        {
            items.push(item);
        }

        start = index + 1;
    }

    items
}

/// The built-in session menu.  It is part of the shell so a power button never
/// depends on an external menu program, and every row's command is
/// [`crate::power`]'s chain rather than a systemd-only verb: the same menu works
/// on a systemd session, an elogind one and a machine with neither.
pub fn session_items() -> Vec<LauncherItem> {
    [
        (
            "Lock Screen",
            "Require the session password",
            "system-lock-screen-symbolic",
            crate::power::Action::Lock,
        ),
        (
            "Log Out",
            "End this session",
            "system-log-out-symbolic",
            crate::power::Action::LogOut,
        ),
        (
            "Suspend",
            "Keep the session in memory",
            "system-suspend-symbolic",
            crate::power::Action::Suspend,
        ),
        (
            "Hibernate",
            "Write the session to disk",
            "system-hibernate-symbolic",
            crate::power::Action::Hibernate,
        ),
        (
            "Restart",
            "Reboot the machine",
            "system-reboot-symbolic",
            crate::power::Action::Restart,
        ),
        (
            "Shut Down",
            "Power the machine off",
            "system-shutdown-symbolic",
            crate::power::Action::ShutDown,
        ),
    ]
    .into_iter()
    .map(|(name, detail, icon, action)| {
        LauncherItem::new(ItemKind::Session, name, detail)
            .with_action(action.command())
            .with_icon(Some(icon))
    })
    .collect()
}

/// Turn a completed home walk into file rows.  The ranking already considered
/// the whole path, so the rows are kept in that order rather than fuzzy-ranked
/// again by their basenames.
pub fn file_items(query: &str, walk: &str, limit: u32) -> Vec<LauncherItem> {
    filesearch::rank_entries(query, walk, limit)
        .into_iter()
        .map(|(path, is_directory)| {
            let base = path.rsplit('/').next().unwrap_or(&path);
            let name = if is_directory {
                format!("{base}/")
            } else {
                base.to_owned()
            };
            let icon = if is_directory {
                "folder-symbolic"
            } else {
                "text-x-generic-symbolic"
            };

            LauncherItem::new(ItemKind::File, name, &path)
                .with_action(&path)
                .with_icon(Some(icon))
        })
        .collect()
}

/// The indices of the best `limit` matches, in descending score order.  The
/// sort is stable, so two equal scores keep the database order.
pub fn rank_indices(items: &[LauncherItem], query: &str, limit: usize) -> Vec<usize> {
    let mut ranked = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| fuzzy::score(query, &item.search).map(|score| (score, index)))
        .collect::<Vec<_>>();

    ranked.sort_by_key(|(score, _)| std::cmp::Reverse(*score));

    ranked
        .into_iter()
        .take(limit)
        .map(|(_, index)| index)
        .collect()
}

/// The mode's heading, in the same order and wording as the C launcher.
pub fn mode_label(mode: &str) -> &'static str {
    match mode {
        "window" => "Windows",
        "run" => "Run a command",
        "calc" => "Calculator",
        "file" => "Files",
        "web" => "Web search",
        "menu" => "Menu",
        "ssh" => "SSH hosts",
        "quit" => "Close a window",
        "clipboard" => "Clipboard",
        "menuitem" => "Menu items",
        "session" => "Session",
        _ => "Applications",
    }
}

/// Scale a CSS font size (`11pt`, `0.9em`) by a percentage.  A value that is not
/// a positive number is returned untouched, exactly as the C helper does.
pub fn scaled_font_size(base: Option<&str>, percent: i32) -> String {
    let source = base.filter(|value| !value.is_empty()).unwrap_or("11pt");
    let number_end = source
        .char_indices()
        .find(|(_, character)| !character.is_ascii_digit() && *character != '.')
        .map(|(index, _)| index)
        .unwrap_or(source.len());
    let value = source[..number_end].parse::<f64>().ok();

    let Some(value) = value.filter(|value| *value > 0.0) else {
        return source.to_owned();
    };
    let unit = source[number_end..].trim();
    let unit = if unit.is_empty() { "pt" } else { unit };

    format!("{:.2}{unit}", value * f64::from(percent) / 100.0)
}

/// Percent-encode a web query as `g_uri_escape_string(..., TRUE)` does.
pub fn web_url(query: &str, base: Option<&str>) -> String {
    let base = base
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_WEB_SEARCH);
    let mut escaped = String::with_capacity(query.len());

    for byte in query.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            escaped.push(char::from(byte));
        } else {
            escaped.push_str(&format!("%{byte:02X}"));
        }
    }

    format!("{base}{escaped}")
}

#[cfg(test)]
mod tests {
    use super::{
        ItemKind, desktop_text, desktop_unescape, file_items, locale_keys, parse_desktop_entry,
        parse_desktop_entry_in_locale, parse_menu_bytes, parse_ssh_hosts, scaled_font_size,
        web_url,
    };

    #[test]
    fn desktop_escapes_are_not_shell_escapes() {
        assert_eq!(desktop_unescape(r"one\stwo\nthree"), "one two\nthree");
        assert_eq!(desktop_unescape(r"a\;b"), "a;b");
    }

    #[test]
    fn font_sizes_scale_with_their_unit() {
        assert_eq!(scaled_font_size(Some("11pt"), 120), "13.20pt");
        assert_eq!(scaled_font_size(Some("0.9em"), 90), "0.81em");
        assert_eq!(scaled_font_size(Some("12"), 100), "12.00pt");
        assert_eq!(scaled_font_size(Some("inherit"), 150), "inherit");
        assert_eq!(scaled_font_size(None, 100), "11.00pt");
    }

    #[test]
    fn files_keep_the_path_ranking_and_get_directory_names() {
        let walk = "d\t/home/me/notes\nf\t/home/me/note.txt\nf\t/home/me/other/note.md\n";
        let items = file_items("note", walk, 3);

        assert_eq!(
            items
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            ["note.txt", "note.md", "notes/"]
        );
        assert_eq!(items[0].kind, ItemKind::File);
        assert_eq!(items[2].action, "/home/me/notes");
    }

    #[test]
    fn menu_urls_escape_every_reserved_byte() {
        assert_eq!(
            web_url("a b&c", Some("https://example.test/?q=")),
            "https://example.test/?q=a%20b%26c"
        );
    }

    #[test]
    fn a_desktop_entry_carries_its_search_terms() {
        let item = parse_desktop_entry(
            "[Desktop Entry]\nName=Firefox\nGenericName=Web Browser\nComment=Browse the web\nKeywords=internet;browser;\nIcon=firefox\nExec=firefox %u\nType=Application\n",
            "firefox.desktop",
            std::path::Path::new("/tmp/firefox.desktop"),
        )
        .expect("an application");

        assert_eq!(item.kind, ItemKind::App);
        assert_eq!(item.name, "Firefox");
        assert_eq!(item.detail, "Web Browser");
        assert_eq!(item.action, "/tmp/firefox.desktop");
        assert!(item.search.contains("browse the web"));
        assert!(item.search.contains("firefox.desktop"));
    }

    #[test]
    fn a_byte_order_mark_is_not_part_of_the_header() {
        let item = parse_desktop_entry(
            "\u{feff}[Desktop Entry]\nType=Application\nName=Marked\nExec=marked\n",
            "marked.desktop",
            std::path::Path::new("/tmp/marked.desktop"),
        )
        .expect("an entry behind a mark");

        assert_eq!(item.name, "Marked");
    }

    #[test]
    fn bytes_that_are_not_utf8_keep_their_entry() {
        // One latin-1 byte in a translated comment, which is what a package
        // built on an older system writes.
        let text = desktop_text(b"[Desktop Entry]\nType=Application\nName=Caf\xe9\nExec=cafe\n");
        let item = parse_desktop_entry(
            &text,
            "cafe.desktop",
            std::path::Path::new("/tmp/cafe.desktop"),
        )
        .expect("an entry with a lossy byte");

        assert!(item.name.starts_with("Caf"));
        // The same bytes with a mark in front of them.
        let marked = desktop_text(b"\xEF\xBB\xBF[Desktop Entry]\nType=Application\nName=Ok\n");

        assert_eq!(
            parse_desktop_entry(
                &marked,
                "ok.desktop",
                std::path::Path::new("/tmp/ok.desktop")
            )
            .expect("an entry behind a mark")
            .name,
            "Ok"
        );
    }

    #[test]
    fn locale_keys_run_from_the_most_specific_form_to_the_language() {
        assert_eq!(
            locale_keys("de_DE.UTF-8@euro"),
            [
                "de_DE.UTF-8@euro",
                "de_DE.UTF-8",
                "de_DE@euro",
                "de_DE",
                "de@euro",
                "de"
            ]
        );
        assert_eq!(locale_keys("pt_BR.UTF-8"), ["pt_BR.UTF-8", "pt_BR", "pt"]);
        assert_eq!(locale_keys("sr@latin"), ["sr@latin", "sr"]);
        assert_eq!(locale_keys("fr"), ["fr"]);
        assert!(locale_keys("C").is_empty());
        assert!(locale_keys("POSIX").is_empty());
        assert!(locale_keys("").is_empty());
    }

    #[test]
    fn a_translated_name_wins_over_the_untranslated_one() {
        let text = "[Desktop Entry]\nType=Application\nName=Files\nName[de]=Dateien\nName[de_DE]=Dateien (DE)\nName[fr]=Fichiers\nGenericName=File Manager\nGenericName[de]=Dateiverwaltung\nExec=files\n";
        let path = std::path::Path::new("/tmp/files.desktop");
        let german = parse_desktop_entry_in_locale(text, "files.desktop", path, "de_DE.UTF-8")
            .expect("a German entry");

        assert_eq!(german.name, "Dateien (DE)");
        assert_eq!(german.detail, "Dateiverwaltung");
        assert_eq!(
            parse_desktop_entry_in_locale(text, "files.desktop", path, "de_AT.UTF-8")
                .expect("the country form falls back to the language")
                .name,
            "Dateien"
        );
        assert_eq!(
            parse_desktop_entry_in_locale(text, "files.desktop", path, "fr_FR.UTF-8")
                .expect("a French entry")
                .name,
            "Fichiers"
        );
        assert_eq!(
            parse_desktop_entry_in_locale(text, "files.desktop", path, "C")
                .expect("an untranslated entry")
                .name,
            "Files"
        );
        assert_eq!(
            parse_desktop_entry_in_locale(text, "files.desktop", path, "es_ES.UTF-8")
                .expect("a locale with no translation")
                .name,
            "Files"
        );
    }

    #[test]
    fn an_empty_translation_falls_back_instead_of_emptying_the_name() {
        let item = parse_desktop_entry_in_locale(
            "[Desktop Entry]\nType=Application\nName=Notes\nName[de]=\nExec=notes\n",
            "notes.desktop",
            std::path::Path::new("/tmp/notes.desktop"),
            "de_DE.UTF-8",
        )
        .expect("an entry with an empty translation");

        assert_eq!(item.name, "Notes");
    }

    #[test]
    fn hidden_and_non_application_entries_are_skipped() {
        for text in [
            "[Desktop Entry]\nName=Hidden\nType=Application\nHidden=true\n",
            "[Desktop Entry]\nName=No display\nType=Application\nNoDisplay=1\n",
            "[Desktop Entry]\nName=Directory\nType=Directory\n",
            "[Desktop Entry]\nType=Application\n",
        ] {
            assert_eq!(
                parse_desktop_entry(text, "x.desktop", std::path::Path::new("/tmp/x")),
                None
            );
        }
    }

    #[test]
    fn ssh_patterns_are_rules_not_hosts() {
        let hosts = parse_ssh_hosts(
            "Host *\n  ForwardAgent yes\nHost alpha beta\nHost gamma?\nHost beta\nHost !blocked\n",
        );

        assert_eq!(
            hosts
                .iter()
                .map(|host| host.name.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "beta"]
        );
    }

    #[test]
    fn menu_script_fields_set_icon_and_disabled() {
        let rows = parse_menu_bytes(
            b"One\tone-command\nTwo\t\x00icon\x1fstarred\x00disabled\x1ftrue\x00\n\xe2\x94\x80\x00disabled\x1ftrue\x00\n",
        );

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, ItemKind::Menu);
        assert_eq!(rows[0].action, "one-command");
        assert_eq!(rows[1].kind, ItemKind::Script);
        assert_eq!(rows[1].icon.as_deref(), Some("starred"));
        assert!(rows[1].disabled);
    }
}
