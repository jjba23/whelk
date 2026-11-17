// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The clipboard history, ported from `src/clipboard.c`.
//!
//! A Wayland client cannot read the selection while another window has focus.
//! The listener is therefore `wl-paste --watch`, which runs this same binary
//! with `--clipboard-store` after every copy.  The helper fetches the richest
//! advertised representation and writes one length-prefixed `WHELK1` record;
//! [`parse_records`] owns the framing, [`History`] owns the policy, and this
//! module's service owns the subprocess, persistence and logind lock watch.
//!
//! The data half deliberately has no GTK or D-Bus dependency.  It is where
//! duplicate ids, pinning, budgets, expiry and the record parser are tested.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gdk_pixbuf::prelude::*;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::logind::PROPERTIES_INTERFACE;
use crate::settings::ClipboardSettings;
use crate::sources;

pub const DEFAULT_MAX_ENTRIES: u32 = 50;
pub const DEFAULT_MAX_ENTRY_BYTES: u32 = 32 * 1024 * 1024;
pub const DEFAULT_MAX_TOTAL_BYTES: u32 = 128 * 1024 * 1024;
pub const DEFAULT_MAX_HELPER_BYTES: usize = 32 * 1024 * 1024;
pub const DATA_MAX: usize = 64 * 1024 * 1024;
pub const TEXT_MAX: usize = 8192;
pub const FILE_MAX: u64 = 512 * 1024 * 1024;
const HEADER_MAX: usize = 512;
const READ_SIZE: usize = 8192;
const RECORD_MAGIC: &[u8] = b"WHELK1";
const HISTORY_MAGIC_V1: &str = "whelk-clipboard-1";
const HISTORY_MAGIC_V2: &str = "whelk-clipboard-2";
const SAVE_DELAY: Duration = Duration::from_millis(180);
const PRUNE_SECONDS: u32 = 60;
const HELPER_TIMEOUT_MS: u64 = 5000;
const TYPES_TIMEOUT_MS: u64 = 2000;

const RICH_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/webp",
    "text/uri-list",
    "text/html",
    "text/rtf",
    "text/plain;charset=utf-8",
    "text/plain",
];
const TEXT_TYPES: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
    "TEXT",
];

thread_local! {
    /// The shell has one clipboard service.  The bar and the launcher read it
    /// here, just as the C modules read the shell singleton.
    static CURRENT: RefCell<Option<Clipboard>> = const { RefCell::new(None) };
}

/// One copy.  The bytes are shared so a row, a restore and a persistence job
/// do not copy a screenshot three times.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub mime: String,
    pub text: String,
    pub data: Arc<[u8]>,
    pub pinned: bool,
    pub created_us: i64,
}

impl Entry {
    pub fn new(mime: impl Into<String>, text: impl Into<String>, data: Vec<u8>) -> Self {
        let mime = mime.into();
        let text = text.into();
        let data: Arc<[u8]> = Arc::from(data);
        let id = content_id(&mime, &text, &data);

        Self {
            id,
            mime,
            text,
            data,
            pinned: false,
            created_us: now_us(),
        }
    }

    /// A stored entry keeps the identity and timestamp the file gave it; a
    /// missing id or timestamp gets a fresh one, as the C loader does.
    fn restored(
        id: Option<&str>,
        created_us: i64,
        mime: &str,
        text: &str,
        data: Vec<u8>,
        pinned: bool,
    ) -> Self {
        let data: Arc<[u8]> = Arc::from(data);
        let id = id
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| content_id(mime, text, &data));

        Self {
            id,
            mime: mime.to_owned(),
            text: text.to_owned(),
            data,
            pinned,
            created_us: if created_us > 0 { created_us } else { now_us() },
        }
    }

    pub fn size(&self) -> usize {
        self.data.len()
    }

    pub fn is_plain_text(&self) -> bool {
        mime_is_plain_text(&self.mime)
    }

    pub fn is_image(&self) -> bool {
        self.mime.starts_with("image/")
    }
}

/// The stable content identity: plain text in any spelling hashes the same;
/// rich content hashes its type and bytes.
fn content_id(mime: &str, text: &str, data: &[u8]) -> String {
    let mut material = Vec::new();

    if mime_is_plain_text(mime) && !text.is_empty() {
        material.extend_from_slice(b"text:");
        material.extend_from_slice(text.as_bytes());
    } else {
        material.extend_from_slice(mime.as_bytes());
        material.push(0);
        material.extend_from_slice(data);
    }

    glib::compute_checksum_for_data(glib::ChecksumType::Sha1, &material)
        .expect("SHA-1 is available")
        .to_string()
}

pub fn mime_is_plain_text(mime: &str) -> bool {
    mime.starts_with("text/plain")
        || mime.eq_ignore_ascii_case("UTF8_STRING")
        || mime.eq_ignore_ascii_case("STRING")
        || mime.eq_ignore_ascii_case("TEXT")
}

fn type_line_matches(line: &str, wanted: &str) -> bool {
    line.trim().eq_ignore_ascii_case(wanted)
}

fn types_have(types: &str, wanted: &str) -> bool {
    types.lines().any(|line| type_line_matches(line, wanted))
}

/// A password manager marks its selection with a type name containing
/// `passwordmanagerhint` or `x-kde-password`.  The value is not read.
pub fn types_secret(types: &str) -> bool {
    let lower = types.to_ascii_lowercase();

    lower.contains("passwordmanagerhint") || lower.contains("x-kde-password")
}

/// The richest usable advertised type, preferring an image, then files, rich
/// text and plain text.  `text_only` is the fallback fetch the helper uses for
/// a rich text copy's plain rendering.
pub fn pick_mime(types: &str, text_only: bool) -> Option<&'static str> {
    let candidates = if text_only { TEXT_TYPES } else { RICH_TYPES };

    candidates
        .iter()
        .copied()
        .find(|candidate| types_have(types, candidate))
}

/// The newest-first history and its two budgets.  Pinned entries are never
/// evicted by the count cap, the byte cap or expiry.
#[derive(Debug, Clone)]
pub struct History {
    entries: Vec<Entry>,
    max_entries: u32,
    max_entry_bytes: u32,
    max_total_bytes: u32,
    last_removed: Option<(Entry, usize)>,
}

impl Default for History {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: DEFAULT_MAX_ENTRIES,
            max_entry_bytes: DEFAULT_MAX_ENTRY_BYTES,
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
            last_removed: None,
        }
    }
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn bytes(&self) -> usize {
        self.entries.iter().map(Entry::size).sum()
    }

    pub fn max_entries(&self) -> u32 {
        self.max_entries
    }

    pub fn max_entry_bytes(&self) -> u32 {
        self.max_entry_bytes
    }

    pub fn max_total_bytes(&self) -> u32 {
        self.max_total_bytes
    }

    pub fn set_limits(&mut self, max_entries: u32, max_entry_bytes: u32, max_total_bytes: u32) {
        self.max_entries = if max_entries > 0 {
            max_entries
        } else {
            DEFAULT_MAX_ENTRIES
        };
        self.max_entry_bytes = if max_entry_bytes > 0 {
            max_entry_bytes
        } else {
            DEFAULT_MAX_ENTRY_BYTES
        };
        self.max_total_bytes = if max_total_bytes > 0 {
            max_total_bytes
        } else {
            DEFAULT_MAX_TOTAL_BYTES
        };

        self.trim();
    }

    pub fn find(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.entries.iter().position(|entry| entry.id == id)
    }

    /// Add a copy at the front.  A duplicate moves the existing entry, keeping
    /// its pin, and takes the new copy's timestamp.
    pub fn add(&mut self, entry: Entry) {
        if let Some(index) = self.index_of(&entry.id) {
            let mut kept = self.entries.remove(index);

            kept.created_us = entry.created_us;
            self.entries.insert(0, kept);
            self.trim_count();
            return;
        }

        self.entries.insert(0, entry);
        self.trim_count();
    }

    fn trim_count(&mut self) {
        let mut unpinned = self.entries.iter().filter(|entry| !entry.pinned).count();

        let mut index = self.entries.len();

        while unpinned > self.max_entries as usize && index > 0 {
            index -= 1;

            if self.entries[index].pinned {
                continue;
            }

            self.entries.remove(index);
            unpinned -= 1;
        }
    }

    fn trim_total(&mut self) {
        let mut total = self.bytes();
        let mut index = self.entries.len();

        while total > self.max_total_bytes as usize && index > 0 {
            index -= 1;

            if self.entries[index].pinned {
                continue;
            }

            total -= self.entries[index].size();
            self.entries.remove(index);
        }
    }

    pub fn trim(&mut self) {
        self.trim_count();
        self.trim_total();
    }

    /// Drop unpinned entries older than `ttl_seconds` relative to `now_us`.
    pub fn prune(&mut self, ttl_seconds: u32, now_us: i64) -> usize {
        if ttl_seconds == 0 {
            return 0;
        }

        let cutoff = now_us.saturating_sub(i64::from(ttl_seconds) * 1_000_000);
        let before = self.entries.len();

        self.entries
            .retain(|entry| entry.pinned || entry.created_us <= 0 || entry.created_us > cutoff);

        before - self.entries.len()
    }

    /// Pin or unpin by id.  Returns `false` when the id is gone.
    pub fn set_pinned(&mut self, id: &str, pinned: bool) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };

        entry.pinned = pinned;
        self.trim();
        true
    }

    pub fn toggle_pin(&mut self, id: &str) -> bool {
        let Some(entry) = self.entries.iter().find(|entry| entry.id == id) else {
            return false;
        };

        let pinned = !entry.pinned;

        self.set_pinned(id, pinned)
    }

    /// Remove one entry and remember it for the single undo.  A new removal
    /// replaces the tombstone, exactly like the C history.
    pub fn remove(&mut self, id: &str) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };

        let entry = self.entries.remove(index);

        self.last_removed = Some((entry, index));
        true
    }

    pub fn undo_remove(&mut self) -> bool {
        let Some((entry, index)) = self.last_removed.take() else {
            return false;
        };

        let index = index.min(self.entries.len());

        self.entries.insert(index, entry);
        self.trim();
        true
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.last_removed = None;
    }

    pub fn clear_unpinned(&mut self) -> bool {
        let before = self.entries.len();

        self.entries.retain(|entry| entry.pinned);
        self.last_removed = None;
        before != self.entries.len()
    }

    /// Restore an entry to the front after it was put back on the clipboard.
    pub fn move_to_front(&mut self, id: &str) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };

        if index == 0 {
            return true;
        }

        let entry = self.entries.remove(index);

        self.entries.insert(0, entry);
        true
    }
}

/// One `WHELK1 <mime> <data> <text>\n` record, without the payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RecordHeader {
    header_length: usize,
    mime_length: usize,
    data_length: usize,
    text_length: usize,
}

fn record_header(pending: &[u8]) -> Result<Option<RecordHeader>, ()> {
    if pending.len() < RECORD_MAGIC.len() {
        return Ok(None);
    }

    if !pending.starts_with(RECORD_MAGIC) {
        return Err(());
    }

    let Some(newline) = pending.iter().position(|byte| *byte == b'\n') else {
        return if pending.len() < HEADER_MAX {
            Ok(None)
        } else {
            Err(())
        };
    };
    let line = std::str::from_utf8(&pending[..newline]).map_err(|_| ())?;
    let mut parts = line.split(' ');

    if parts.next() != Some("WHELK1") {
        return Err(());
    }

    let mime_length = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or(())?;
    let data_length = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or(())?;
    let text_length = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or(())?;

    if parts.next().is_some()
        || mime_length == 0
        || mime_length > 256
        || data_length > DATA_MAX
        || text_length > DATA_MAX
    {
        return Err(());
    }

    Ok(Some(RecordHeader {
        header_length: newline + 1,
        mime_length,
        data_length,
        text_length,
    }))
}

/// Consume every complete record in `pending`.  Incomplete bytes stay for the
/// next read; a malformed record clears the buffer rather than poisoning every
/// later copy.
pub fn parse_records(pending: &mut Vec<u8>, history: &mut History) -> usize {
    let mut records = 0;

    loop {
        let header = match record_header(pending) {
            Ok(Some(header)) => header,
            Ok(None) => break,
            Err(()) => {
                eprintln!("whelk: clipboard: dropping a malformed record");
                pending.clear();
                break;
            }
        };
        let total = header
            .header_length
            .saturating_add(header.mime_length)
            .saturating_add(header.data_length)
            .saturating_add(header.text_length);

        if pending.len() < total {
            break;
        }

        let payload = &pending[header.header_length..];
        let mime_bytes = &payload[..header.mime_length];
        let data_start = header.mime_length;
        let data_end = data_start + header.data_length;
        let data = payload[data_start..data_end].to_vec();
        let text_start = data_end;
        let text_end = text_start + header.text_length;
        let mut text = payload[text_start..text_end]
            .iter()
            .copied()
            .take(TEXT_MAX)
            .collect::<Vec<_>>();
        let mime = String::from_utf8_lossy(mime_bytes).into_owned();

        if text.first() == Some(&0) {
            text.clear();
        }

        if text.is_empty() && mime.starts_with("text/") {
            text = data.iter().copied().take(TEXT_MAX).collect();
        }

        if let Some(nul) = text.iter().position(|byte| *byte == 0) {
            text.truncate(nul);
        }

        let text = String::from_utf8_lossy(&text).into_owned();

        history.add(Entry::new(mime, text, data));
        pending.drain(..total);
        records += 1;
    }

    records
}

/// The helper's framing.  Kept public so a test can feed the listener exactly
/// what the helper writes, including NUL bytes in an image.
pub fn encode_record(mime: &str, data: &[u8], text: &[u8]) -> Vec<u8> {
    let mut record = format!("WHELK1 {} {} {}\n", mime.len(), data.len(), text.len()).into_bytes();

    record.extend_from_slice(mime.as_bytes());
    record.extend_from_slice(data);
    record.extend_from_slice(text);
    record
}

/// A short first-line rendering.  A copied paragraph is many lines, but a
/// picker row is one.
pub fn entry_label(entry: &Entry) -> String {
    if entry.mime == "text/uri-list" {
        let files = entry
            .text
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .count();

        return match files {
            1 => "1 file".to_owned(),
            count => format!("{count} files"),
        };
    }

    if !entry.text.is_empty() {
        let line = entry.text.lines().next().unwrap_or("").trim();
        let line = if line.chars().count() > 80 {
            line.chars().take(80).collect::<String>()
        } else {
            line.to_owned()
        };

        return if line.is_empty() {
            "(blank line)".to_owned()
        } else {
            line
        };
    }

    let kind = if entry.is_image() {
        "Image"
    } else {
        "Rich content"
    };

    format!("[{kind} — {}]", entry.mime)
}

pub fn mime_kind(mime: &str) -> &'static str {
    if mime.starts_with("image/") {
        "Image"
    } else if mime == "text/html" {
        "HTML"
    } else if mime == "text/uri-list" {
        "Files"
    } else if mime == "text/rtf" {
        "Rich text"
    } else if mime_is_plain_text(mime) {
        "Text"
    } else {
        "Content"
    }
}

pub fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub fn format_age(created_us: i64, now_us: i64) -> String {
    if created_us <= 0 {
        return "unknown age".to_owned();
    }

    let seconds = (now_us - created_us).max(0) / 1_000_000;

    if seconds < 60 {
        "just now".to_owned()
    } else if seconds < 3600 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h ago", seconds / 3600)
    } else {
        format!("{}d ago", seconds / 86_400)
    }
}

pub fn entry_detail(entry: &Entry) -> String {
    let pinned = if entry.pinned { "Pinned · " } else { "" };

    format!(
        "{pinned}{} · {} · {}",
        mime_kind(&entry.mime),
        format_size(entry.size()),
        format_age(entry.created_us, now_us())
    )
}

/// A small PNG thumbnail for an image row.  The picker should not decode a
/// full screenshot for a 24-pixel icon; the C code scaled to 72 pixels first.
pub fn thumbnail_png(entry: &Entry) -> Option<Vec<u8>> {
    if !entry.is_image() {
        return None;
    }

    let bytes = glib::Bytes::from(entry.data.as_ref());
    let stream = gio::MemoryInputStream::from_bytes(&bytes);
    let pixbuf = gdk_pixbuf::Pixbuf::from_stream(&stream, None::<&gio::Cancellable>).ok()?;
    let width = pixbuf.width().max(1);
    let height = pixbuf.height().max(1);
    let scale = (72.0 / f64::from(width))
        .min(72.0 / f64::from(height))
        .min(1.0);
    let scaled = if scale < 1.0 {
        let width = (f64::from(width) * scale).max(1.0) as i32;
        let height = (f64::from(height) * scale).max(1.0) as i32;

        pixbuf.scale_simple(width, height, gdk_pixbuf::InterpType::Bilinear)?
    } else {
        pixbuf
    };

    scaled.save_to_bufferv("png", &[]).ok()
}

fn now_us() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

/// The service status the bar and the empty picker show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ready,
    Paused,
    Unavailable,
}

impl Status {
    pub fn text(self) -> &'static str {
        match self {
            Self::Ready => "Clipboard history",
            Self::Paused => "Clipboard history paused",
            Self::Unavailable => "Clipboard listener stopped",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Ready => "Copy something and it will appear here",
            Self::Paused => "Use the clipboard bar menu or reload the shell to resume",
            Self::Unavailable => "wl-paste is not running; the history is still available",
        }
    }
}

struct State {
    history: History,
    settings: ClipboardSettings,
    persist_path: Option<PathBuf>,
    persist_loaded: bool,
    save_timeout: Option<glib::SourceId>,
    save_in_flight: bool,
    save_dirty: bool,
    prune_timeout: Option<glib::SourceId>,
    watch: Option<gio::Subprocess>,
    stream: Option<gio::InputStream>,
    read_generation: u64,
    pending: Vec<u8>,
    paused: bool,
    available: bool,
    lock_connection: Option<gio::DBusConnection>,
    lock_subscription: Option<gio::SignalSubscription>,
    lock_watch_pending: bool,
    next_listener_id: u64,
    listeners: Vec<(u64, Rc<dyn Fn()>)>,
}

impl State {
    fn next_listener(&mut self) -> u64 {
        self.next_listener_id = self.next_listener_id.wrapping_add(1);
        self.next_listener_id
    }
}

/// A live change listener.  Dropping the guard removes the callback, so a
/// rebuilt bar or launcher does not leave one behind.
pub struct ClipboardSubscription {
    inner: Weak<RefCell<State>>,
    id: u64,
}

impl Drop for ClipboardSubscription {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            inner
                .borrow_mut()
                .listeners
                .retain(|(id, _)| *id != self.id);
        }
    }
}

/// The shell's clipboard service.
#[derive(Clone)]
pub struct Clipboard {
    inner: Rc<RefCell<State>>,
}

impl Clipboard {
    /// Build a memory-only service, or one that loads the configured history
    /// immediately.  The watcher starts in [`start`](Self::start), after `ui`
    /// has installed the service so early records find their listeners.
    pub fn new(settings: ClipboardSettings) -> Self {
        let settings = effective_settings(&settings, environment_persist());

        let persist_path = resolve_persist_path(&settings);
        let mut history = History::new();

        history.set_limits(
            settings.max_entries,
            settings.max_entry_bytes,
            settings.max_total_bytes,
        );

        let inner = Rc::new(RefCell::new(State {
            history,
            settings,
            persist_path,
            persist_loaded: false,
            save_timeout: None,
            save_in_flight: false,
            save_dirty: false,
            prune_timeout: None,
            watch: None,
            stream: None,
            read_generation: 0,
            pending: Vec::new(),
            paused: false,
            available: false,
            lock_connection: None,
            lock_subscription: None,
            lock_watch_pending: false,
            next_listener_id: 0,
            listeners: Vec::new(),
        }));

        Self { inner }
    }

    pub fn current() -> Option<Self> {
        CURRENT.with(|current| current.borrow().clone())
    }

    pub fn install(service: Option<Self>) {
        CURRENT.with(|current| *current.borrow_mut() = service);
    }

    /// Load the persisted history, start the listener and the expiry timer,
    /// and subscribe to logind's `LockedHint` when the configuration asks.
    pub fn start(&self) {
        self.load_persistence();
        self.start_watch();
        self.restart_prune_timer();
        self.start_lock_watch();
    }

    pub fn shutdown(&self) {
        let watch = {
            let mut state = self.inner.borrow_mut();

            state.read_generation = state.read_generation.wrapping_add(1);
            state.pending.clear();
            state.available = false;
            state.lock_subscription.take();
            state.lock_connection.take();

            if let Some(source) = state.save_timeout.take() {
                sources::remove(source);
            }

            if let Some(source) = state.prune_timeout.take() {
                sources::remove(source);
            }

            state.watch.take()
        };

        if let Some(watch) = watch {
            watch.force_exit();
        }

        self.flush();
    }

    pub fn on_changed<F>(&self, callback: F) -> ClipboardSubscription
    where
        F: Fn() + 'static,
    {
        let id = {
            let mut state = self.inner.borrow_mut();

            state.next_listener()
        };

        self.inner
            .borrow_mut()
            .listeners
            .push((id, Rc::new(callback)));

        ClipboardSubscription {
            inner: Rc::downgrade(&self.inner),
            id,
        }
    }

    fn changed(&self) {
        let listeners = self.inner.borrow().listeners.clone();

        for (_, listener) in listeners {
            listener();
        }

        self.schedule_save();
    }

    pub fn entries(&self) -> Vec<Entry> {
        self.inner.borrow().history.entries().to_vec()
    }

    pub fn count(&self) -> usize {
        self.inner.borrow().history.len()
    }

    pub fn bytes(&self) -> usize {
        self.inner.borrow().history.bytes()
    }

    pub fn find(&self, id: &str) -> Option<Entry> {
        self.inner.borrow().history.find(id).cloned()
    }

    pub fn is_paused(&self) -> bool {
        self.inner.borrow().paused
    }

    pub fn is_available(&self) -> bool {
        self.inner.borrow().available
    }

    pub fn status(&self) -> Status {
        let state = self.inner.borrow();

        if state.paused {
            Status::Paused
        } else if !state.available {
            Status::Unavailable
        } else {
            Status::Ready
        }
    }

    pub fn status_text(&self) -> &'static str {
        self.status().text()
    }

    /// The one-line answer `--clipboard-status` prints, in the C shell's
    /// spelling: availability, entry count and byte count on one line.
    pub fn status_line(&self) -> String {
        let count = self.count();
        let noun = if count == 1 { "entry" } else { "entries" };

        format!(
            "{}; {} {}; {} bytes",
            self.status_text(),
            count,
            noun,
            self.bytes()
        )
    }

    pub fn max_entries(&self) -> u32 {
        self.inner.borrow().history.max_entries()
    }

    pub fn max_entry_bytes(&self) -> u32 {
        self.inner.borrow().history.max_entry_bytes()
    }

    pub fn max_total_bytes(&self) -> u32 {
        self.inner.borrow().history.max_total_bytes()
    }

    pub fn persistence_path(&self) -> Option<PathBuf> {
        self.inner.borrow().persist_path.clone()
    }

    pub fn set_paused(&self, paused: bool) {
        let changed = {
            let mut state = self.inner.borrow_mut();

            if state.paused == paused {
                false
            } else {
                state.paused = paused;
                true
            }
        };

        if !changed {
            return;
        }

        if paused {
            self.stop_watch();
        } else {
            self.start_watch();
        }

        self.changed();
    }

    pub fn restart(&self) {
        self.stop_watch();

        if !self.is_paused() {
            self.start_watch();
        }

        self.changed();
    }

    pub fn clear(&self) {
        self.inner.borrow_mut().history.clear();
        self.changed();
    }

    pub fn clear_unpinned(&self) -> bool {
        let changed = self.inner.borrow_mut().history.clear_unpinned();

        if changed {
            self.changed();
        }

        changed
    }

    pub fn remove(&self, id: &str) -> bool {
        let changed = self.inner.borrow_mut().history.remove(id);

        if changed {
            self.changed();
        }

        changed
    }

    pub fn undo_remove(&self) -> bool {
        let changed = self.inner.borrow_mut().history.undo_remove();

        if changed {
            self.changed();
        }

        changed
    }

    pub fn toggle_pin(&self, id: &str) -> bool {
        let changed = self.inner.borrow_mut().history.toggle_pin(id);

        if changed {
            self.changed();
        }

        changed
    }

    pub fn set_pinned(&self, id: &str, pinned: bool) -> bool {
        let changed = self.inner.borrow_mut().history.set_pinned(id, pinned);

        if changed {
            self.changed();
        }

        changed
    }

    /// Add one copy.  The per-entry budget is checked before the bytes join
    /// the history so a huge advertised type does not sit in memory first.
    pub fn add(&self, mime: &str, text: &str, data: Vec<u8>) -> bool {
        let limit = {
            let state = self.inner.borrow();

            state.history.max_entry_bytes() as usize
        };

        if data.len() > limit {
            eprintln!(
                "whelk: clipboard: not storing a {}-byte copy: the entry budget is {} bytes",
                data.len(),
                limit
            );

            return false;
        }

        {
            let mut state = self.inner.borrow_mut();

            state.history.add(Entry::new(mime, text, data));
            state.history.trim();
        }

        self.changed();
        true
    }

    /// Public text convenience, used by the launcher's calculator.
    pub fn copy(&self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }

        let added = self.add("text/plain;charset=utf-8", text, text.as_bytes().to_vec());

        if added {
            let id = self
                .inner
                .borrow()
                .history
                .entries()
                .first()
                .map(|entry| entry.id.clone());

            if let Some(id) = id {
                self.restore(&id);
            }
        }

        added
    }

    pub fn restore(&self, id: &str) -> bool {
        let Some(entry) = self.find(id) else {
            return false;
        };

        write_clipboard(&entry);
        self.inner.borrow_mut().history.move_to_front(id);
        self.changed();
        true
    }

    pub fn restore_index(&self, index: usize) -> bool {
        let id = self
            .inner
            .borrow()
            .history
            .entries()
            .get(index)
            .map(|entry| entry.id.clone());

        id.is_some_and(|id| self.restore(&id))
    }

    pub fn thumbnail(&self, id: &str) -> Option<Vec<u8>> {
        self.find(id).and_then(|entry| thumbnail_png(&entry))
    }

    /// Apply a reloaded configuration.  The running history survives a limit
    /// change; a different persistence path is flushed and loaded as a new
    /// store, just as starting with that path would.
    pub fn apply_settings(&self, settings: &ClipboardSettings) {
        let settings = effective_settings(settings, environment_persist());
        let path = resolve_persist_path(&settings);
        let old_path = self.inner.borrow().persist_path.clone();
        let path_changed = old_path != path;

        if path_changed {
            self.flush();
        }

        {
            let mut state = self.inner.borrow_mut();

            if path_changed {
                state.persist_path = path;
                state.persist_loaded = false;
            }

            state.settings = settings;

            let limits = state.settings.clone();

            state.history.set_limits(
                limits.max_entries,
                limits.max_entry_bytes,
                limits.max_total_bytes,
            );
        }

        if path_changed {
            self.load_persistence();
        }

        self.restart_prune_timer();
        self.start_lock_watch();
        self.schedule_save();
        self.changed();
    }

    fn load_persistence(&self) {
        let Some(path) = self.inner.borrow().persist_path.clone() else {
            return;
        };

        let pinned_only = self.inner.borrow().settings.persist_only_pinned;
        let mut state = self.inner.borrow_mut();

        state.persist_loaded = true;

        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => {
                eprintln!(
                    "whelk: clipboard: cannot inspect '{}': {error}",
                    path.display()
                );

                return;
            }
        };

        if metadata.len() > FILE_MAX {
            eprintln!("whelk: clipboard: history file is too large; it was not loaded");

            return;
        }

        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!(
                    "whelk: clipboard: cannot read '{}': {error}",
                    path.display()
                );

                return;
            }
        };

        if let Some(entries) = decode_history(&bytes, pinned_only) {
            state.history.clear();

            // `decode_history` is newest first; `add` prepends, so consume
            // the older entries first.
            for entry in entries.into_iter().rev() {
                state.history.add(entry);
            }

            state.history.trim();
            let ttl = state.settings.ttl_seconds;

            state.history.prune(ttl, now_us());
            eprintln!(
                "whelk: clipboard: restored {} entries from {}",
                state.history.len(),
                path.display()
            );
        }
    }

    fn start_watch(&self) {
        let mut state = self.inner.borrow_mut();

        if state.paused || state.watch.is_some() {
            return;
        }

        let exe = std::env::current_exe()
            .ok()
            .and_then(|path| path.to_str().map(str::to_owned))
            .unwrap_or_else(|| "whelk".to_owned());
        let executable = OsString::from(exe);
        let argv = [
            OsStr::new("wl-paste"),
            OsStr::new("--watch"),
            executable.as_os_str(),
            OsStr::new("--clipboard-store"),
        ];
        let process = match gio::Subprocess::newv(
            &argv,
            gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE,
        ) {
            Ok(process) => process,
            Err(error) => {
                state.available = false;
                eprintln!("whelk: clipboard: wl-paste is not installed: {error}");
                return;
            }
        };
        let Some(stream) = process.stdout_pipe() else {
            process.force_exit();
            state.available = false;
            return;
        };

        state.read_generation = state.read_generation.wrapping_add(1);
        state.pending.clear();
        state.watch = Some(process);
        state.stream = Some(stream);
        state.available = true;

        drop(state);
        read_next(&self.inner);
    }

    fn stop_watch(&self) {
        let process = {
            let mut state = self.inner.borrow_mut();

            state.read_generation = state.read_generation.wrapping_add(1);
            state.pending.clear();
            state.stream.take();
            state.available = false;
            state.watch.take()
        };

        if let Some(process) = process {
            process.force_exit();
        }
    }

    fn restart_prune_timer(&self) {
        let (ttl, source) = {
            let mut state = self.inner.borrow_mut();

            (state.settings.ttl_seconds, state.prune_timeout.take())
        };

        if let Some(source) = source {
            sources::remove(source);
        }

        if ttl == 0 {
            return;
        }

        let seconds = ttl.clamp(1, PRUNE_SECONDS);
        let weak = Rc::downgrade(&self.inner);

        self.prune();

        let source = glib::timeout_add_seconds_local(seconds, move || {
            let Some(inner) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };

            Clipboard { inner }.prune();

            glib::ControlFlow::Continue
        });

        self.inner.borrow_mut().prune_timeout = Some(source);
    }

    fn prune(&self) {
        let ttl = self.inner.borrow().settings.ttl_seconds;
        let removed = self.inner.borrow_mut().history.prune(ttl, now_us());

        if removed > 0 {
            self.changed();
        }
    }

    fn schedule_save(&self) {
        let path = self.inner.borrow().persist_path.clone();

        if path.is_none() {
            return;
        }

        {
            let mut state = self.inner.borrow_mut();

            if !state.persist_loaded {
                return;
            }

            state.save_dirty = true;

            if state.save_in_flight || state.save_timeout.is_some() {
                return;
            }
        }

        let weak = Rc::downgrade(&self.inner);
        let source = glib::timeout_add_local_once(SAVE_DELAY, move || {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let clipboard = Clipboard { inner };

            clipboard.save_now();
        });

        self.inner.borrow_mut().save_timeout = Some(source);
    }

    fn save_now(&self) {
        let (path, bytes) = {
            let mut state = self.inner.borrow_mut();

            state.save_timeout.take();

            let Some(path) = state.persist_path.clone() else {
                return;
            };

            if !state.persist_loaded {
                return;
            }

            let bytes = serialize_history(&state.history, state.settings.persist_only_pinned);

            state.save_in_flight = true;
            state.save_dirty = false;
            (path, bytes)
        };

        if let Err(error) = ensure_private_parent(&path) {
            eprintln!(
                "whelk: clipboard: refusing to save '{}': {error}",
                path.display()
            );
            self.inner.borrow_mut().save_in_flight = false;

            return;
        }

        let file = gio::File::for_path(&path);
        let data = glib::Bytes::from_owned(bytes);
        let weak = Rc::downgrade(&self.inner);
        let saved_path = path.clone();

        file.replace_contents_bytes_async(
            &data,
            None,
            false,
            gio::FileCreateFlags::REPLACE_DESTINATION,
            None::<&gio::Cancellable>,
            move |result| {
                if let Err(error) = result {
                    eprintln!(
                        "whelk: clipboard: could not save '{}': {error}",
                        saved_path.display()
                    );
                } else if let Err(error) =
                    std::fs::set_permissions(&saved_path, std::fs::Permissions::from_mode(0o600))
                {
                    eprintln!(
                        "whelk: clipboard: could not make '{}' private: {error}",
                        saved_path.display()
                    );
                }

                let Some(inner) = weak.upgrade() else {
                    return;
                };
                let clipboard = Clipboard { inner };
                let dirty = {
                    let mut state = clipboard.inner.borrow_mut();

                    state.save_in_flight = false;
                    state.save_dirty
                };

                if dirty {
                    clipboard.schedule_save();
                }
            },
        );
    }

    /// Write synchronously for shutdown and explicit tests.  The normal path
    /// is the 180 ms debounced asynchronous write above.
    pub fn flush(&self) {
        self.wait_for_save();

        let (path, bytes) = {
            let mut state = self.inner.borrow_mut();

            if let Some(source) = state.save_timeout.take() {
                sources::remove(source);
            }

            let Some(path) = state.persist_path.clone() else {
                return;
            };

            if !state.persist_loaded {
                return;
            }

            let bytes = serialize_history(&state.history, state.settings.persist_only_pinned);

            (path, bytes)
        };

        if let Err(error) = write_private(&path, &bytes) {
            eprintln!(
                "whelk: clipboard: could not save '{}': {error}",
                path.display()
            );
        } else {
            self.inner.borrow_mut().save_dirty = false;
        }
    }

    /// Let an asynchronous write finish before the synchronous flush.  The
    /// worker may still hold older history; writing now and letting that
    /// completion land later would restore stale data on disk.
    fn wait_for_save(&self) {
        while self.inner.borrow().save_in_flight {
            glib::MainContext::default().iteration(true);
        }
    }

    fn start_lock_watch(&self) {
        let needed = self.inner.borrow().settings.clear_on_lock;

        if !needed {
            let mut state = self.inner.borrow_mut();

            state.lock_subscription.take();
            state.lock_connection.take();
            return;
        }

        {
            let mut state = self.inner.borrow_mut();

            if state.lock_subscription.is_some() || state.lock_watch_pending {
                return;
            }

            state.lock_watch_pending = true;
        }

        let weak = Rc::downgrade(&self.inner);

        gio::bus_get(
            gio::BusType::System,
            gio::Cancellable::NONE,
            move |result| {
                let Some(inner) = weak.upgrade() else {
                    return;
                };
                let connection = match result {
                    Ok(connection) => connection,
                    Err(error) => {
                        inner.borrow_mut().lock_watch_pending = false;
                        eprintln!("whelk: clipboard: no system bus for lock detection: {error}");

                        return;
                    }
                };

                inner.borrow_mut().lock_connection = Some(connection.clone());

                let weak = Rc::downgrade(&inner);
                let session_connection = connection.clone();

                connection.call(
                    Some("org.freedesktop.login1"),
                    "/org/freedesktop/login1",
                    "org.freedesktop.login1.Manager",
                    "GetSession",
                    Some(&("auto",).to_variant()),
                    Some(glib::VariantTy::new("(o)").expect("static type")),
                    gio::DBusCallFlags::NONE,
                    2000,
                    gio::Cancellable::NONE,
                    move |result| {
                        let Some(inner) = weak.upgrade() else {
                            return;
                        };

                        inner.borrow_mut().lock_watch_pending = false;

                        let reply = match result {
                            Ok(reply) => reply,
                            Err(error) => {
                                eprintln!(
                                    "whelk: clipboard: could not find the logind session: {error}"
                                );

                                return;
                            }
                        };
                        let Some((path,)) = reply.get::<(String,)>() else {
                            return;
                        };
                        let enabled = inner.borrow().settings.clear_on_lock;

                        if !enabled {
                            return;
                        }

                        let weak = Rc::downgrade(&inner);
                        let subscription = session_connection.subscribe_to_signal(
                            Some("org.freedesktop.login1"),
                            Some(PROPERTIES_INTERFACE),
                            Some("PropertiesChanged"),
                            Some(&path),
                            None,
                            gio::DBusSignalFlags::NONE,
                            move |signal| {
                                let Some(inner) = weak.upgrade() else {
                                    return;
                                };

                                if locked_hint(signal.parameters) {
                                    let clipboard = Clipboard { inner };

                                    if clipboard.clear_unpinned() {
                                        eprintln!(
                                            "whelk: clipboard: session locked: unpinned history cleared"
                                        );
                                    }
                                }
                            },
                        );

                        inner.borrow_mut().lock_subscription = Some(subscription);
                    },
                );
            },
        );
    }
}

impl Drop for State {
    fn drop(&mut self) {
        if let Some(source) = self.save_timeout.take() {
            sources::remove(source);
        }

        if let Some(source) = self.prune_timeout.take() {
            sources::remove(source);
        }

        self.lock_subscription.take();
        self.lock_connection.take();

        if let Some(watch) = self.watch.take() {
            watch.force_exit();
        }

        if self.persist_loaded
            && self.save_dirty
            && let (Some(path), Some(bytes)) = (
                self.persist_path.as_ref(),
                Some(serialize_history(
                    &self.history,
                    self.settings.persist_only_pinned,
                )),
            )
        {
            let _ = write_private(path, &bytes);
        }
    }
}

fn locked_hint(parameters: &glib::Variant) -> bool {
    let Some((_changed_interface, changed, _invalidated)) =
        parameters.get::<(String, HashMap<String, glib::Variant>, Vec<String>)>()
    else {
        return false;
    };

    changed
        .get("LockedHint")
        .and_then(|value| value.get::<bool>())
        .unwrap_or(false)
}

fn read_next(inner: &Rc<RefCell<State>>) {
    let (stream, generation) = {
        let state = inner.borrow();

        let Some(stream) = state.stream.clone() else {
            return;
        };

        (stream, state.read_generation)
    };
    let weak = Rc::downgrade(inner);

    stream.read_bytes_async(
        READ_SIZE,
        glib::Priority::DEFAULT,
        None::<&gio::Cancellable>,
        move |result| {
            let Some(inner) = weak.upgrade() else {
                return;
            };

            let records = {
                let mut state = inner.borrow_mut();

                if state.read_generation != generation {
                    return;
                }

                match result {
                    Ok(bytes) if !bytes.is_empty() => {
                        state.pending.extend_from_slice(bytes.as_ref());

                        let mut pending = std::mem::take(&mut state.pending);
                        let records = parse_records(&mut pending, &mut state.history);

                        state.pending = pending;

                        if records > 0 {
                            state.history.trim();
                        }

                        records
                    }
                    _ => {
                        // EOF or an error: this watcher is gone.  Keep the
                        // history and report unavailable until a restart.
                        state.read_generation = state.read_generation.wrapping_add(1);
                        state.stream.take();
                        state.pending.clear();
                        state.available = false;

                        let watch = state.watch.take();

                        drop(state);

                        if let Some(watch) = watch {
                            watch.force_exit();
                        }

                        let clipboard = Clipboard { inner };

                        clipboard.changed();
                        return;
                    }
                }
            };

            if records > 0 {
                let clipboard = Clipboard {
                    inner: Rc::clone(&inner),
                };

                clipboard.changed();
            }

            read_next(&inner);
        },
    );
}

fn resolve_persist_path(settings: &ClipboardSettings) -> Option<PathBuf> {
    settings.persistence_path(
        std::env::var("XDG_STATE_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

fn effective_settings(
    settings: &ClipboardSettings,
    environment: Option<String>,
) -> ClipboardSettings {
    let mut settings = settings.clone();

    // The compatibility variable is consulted only when the settings file did
    // not configure a clipboard policy, matching the C shell.
    if !settings.configured {
        settings.persist = environment;
    }

    settings
}

fn environment_persist() -> Option<String> {
    std::env::var("WHELK_CLIPBOARD_PERSIST")
        .ok()
        .filter(|value| !value.is_empty())
        .filter(|value| value != "0" && !value.eq_ignore_ascii_case("false"))
}

/// Require a directory the current user owns and no one else can enter.  The
/// history contains clipboard contents, so an existing world-writable or
/// symlinked parent is a refusal, not something to repair silently.
fn ensure_private_parent(path: &Path) -> std::io::Result<()> {
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "clipboard persistence path has no parent directory",
        ));
    };

    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;

    let metadata = std::fs::symlink_metadata(parent)?;

    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "clipboard persistence directory '{}' is not a private directory owned by this user",
                parent.display()
            ),
        ));
    }

    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    ensure_private_parent(path)?;

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "clipboard persistence path has no parent directory",
            )
        })?;
    // `create_new` is O_CREAT|O_EXCL and therefore refuses a symlink planted
    // at the temporary name.  A random name keeps concurrent saves apart.
    let temporary = parent.join(format!(
        ".whelk-clipboard-{}.tmp",
        glib::uuid_string_random()
    ));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;

        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }

    result
}

fn serialize_history(history: &History, pinned_only: bool) -> Vec<u8> {
    let items = history
        .entries()
        .iter()
        .filter(|entry| !pinned_only || entry.pinned)
        .map(|entry| {
            let data = glib::Variant::array_from_fixed_array(entry.data.as_ref());

            glib::Variant::tuple_from_iter([
                entry.id.to_variant(),
                entry.created_us.to_variant(),
                entry.mime.to_variant(),
                entry.text.to_variant(),
                data,
                entry.pinned.to_variant(),
            ])
        })
        .collect::<Vec<_>>();
    let tuple_type = glib::VariantTy::new("(sxssayb)").expect("static type");
    let array = glib::Variant::array_from_iter_with_type(tuple_type, items);
    let root = glib::Variant::tuple_from_iter([HISTORY_MAGIC_V2.to_variant(), array]);

    root.data_as_bytes().as_ref().to_vec()
}

fn decode_history(bytes: &[u8], pinned_only: bool) -> Option<Vec<Entry>> {
    for (version, type_name, entry_count) in [(2_u8, "(sa(sxssayb))", 6), (1_u8, "(sa(ssayb))", 4)]
    {
        let Ok(type_) = glib::VariantTy::new(type_name) else {
            continue;
        };
        let root = glib::Variant::from_bytes_with_type(&glib::Bytes::from(bytes), type_);

        if !root.is_type(type_) || !root.is_normal_form() || root.n_children() != 2 {
            continue;
        }

        let Some(magic) = root.child_value(0).get::<String>() else {
            continue;
        };
        let expected = if version == 2 {
            HISTORY_MAGIC_V2
        } else {
            HISTORY_MAGIC_V1
        };

        if magic != expected {
            continue;
        }

        let array = root.child_value(1);
        let mut entries = Vec::new();
        let mut valid = true;

        // The file is newest first and `History::add` prepends, so walk the
        // GVariant array backwards.
        for index in (0..array.n_children()).rev() {
            let Some(item) = array.try_child_value(index) else {
                valid = false;
                break;
            };

            if item.n_children() != entry_count {
                valid = false;
                break;
            }

            let parsed = (|| {
                if version == 2 {
                    Some((
                        item.child_value(0).get::<String>(),
                        item.child_value(1).get::<i64>().unwrap_or(0),
                        item.child_value(2).get::<String>()?,
                        item.child_value(3).get::<String>()?,
                        item.child_value(4).get::<Vec<u8>>()?,
                        item.child_value(5).get::<bool>()?,
                    ))
                } else {
                    Some((
                        None,
                        0,
                        item.child_value(0).get::<String>()?,
                        item.child_value(1).get::<String>()?,
                        item.child_value(2).get::<Vec<u8>>()?,
                        item.child_value(3).get::<bool>()?,
                    ))
                }
            })();
            let Some((id, created_us, mime, text, data, pinned)) = parsed else {
                valid = false;
                break;
            };

            if pinned_only && !pinned {
                continue;
            }

            entries.insert(
                0,
                Entry::restored(id.as_deref(), created_us, &mime, &text, data, pinned),
            );
        }

        if valid {
            return Some(entries);
        }
    }

    None
}

fn write_clipboard(entry: &Entry) {
    if write_clipboard_gdk(entry) {
        return;
    }

    write_clipboard_fallback(entry);
}

fn write_clipboard_gdk(entry: &Entry) -> bool {
    let Some(display) = gtk::gdk::Display::default() else {
        return false;
    };
    let clipboard = display.clipboard();
    let mut providers = Vec::new();
    let mut seen = HashSet::new();

    add_provider(&mut providers, &mut seen, &entry.mime, entry.data.as_ref());

    if !entry.text.is_empty() && entry.mime.starts_with("text/") && !mime_is_plain_text(&entry.mime)
    {
        add_provider(
            &mut providers,
            &mut seen,
            "text/plain;charset=utf-8",
            entry.text.as_bytes(),
        );
        add_provider(
            &mut providers,
            &mut seen,
            "text/plain",
            entry.text.as_bytes(),
        );
        add_provider(
            &mut providers,
            &mut seen,
            "UTF8_STRING",
            entry.text.as_bytes(),
        );
    } else if mime_is_plain_text(&entry.mime) {
        add_provider(&mut providers, &mut seen, "text/plain", entry.data.as_ref());
        add_provider(
            &mut providers,
            &mut seen,
            "UTF8_STRING",
            entry.data.as_ref(),
        );
    }

    if entry.is_image()
        && entry.mime != "image/png"
        && let Some(png) = image_png(entry)
    {
        add_provider(&mut providers, &mut seen, "image/png", &png);
    }

    if providers.is_empty() {
        return false;
    }

    let provider = gtk::gdk::ContentProvider::new_union(&providers);

    match clipboard.set_content(Some(&provider)) {
        Ok(()) => true,
        Err(error) => {
            eprintln!("whelk: clipboard: could not set the selection: {error}");
            false
        }
    }
}

fn add_provider(
    providers: &mut Vec<gtk::gdk::ContentProvider>,
    seen: &mut HashSet<String>,
    mime: &str,
    data: &[u8],
) {
    if mime.is_empty() || !seen.insert(mime.to_owned()) {
        return;
    }

    providers.push(gtk::gdk::ContentProvider::for_bytes(
        mime,
        &glib::Bytes::from(data),
    ));
}

fn image_png(entry: &Entry) -> Option<Vec<u8>> {
    let bytes = glib::Bytes::from(entry.data.as_ref());
    let stream = gio::MemoryInputStream::from_bytes(&bytes);
    let pixbuf = gdk_pixbuf::Pixbuf::from_stream(&stream, None::<&gio::Cancellable>).ok()?;

    pixbuf.save_to_bufferv("png", &[]).ok()
}

fn write_clipboard_fallback(entry: &Entry) {
    let argv = [
        OsStr::new("wl-copy"),
        OsStr::new("--type"),
        OsStr::new(&entry.mime),
    ];
    let process = match gio::Subprocess::newv(&argv, gio::SubprocessFlags::STDIN_PIPE) {
        Ok(process) => process,
        Err(error) => {
            eprintln!("whelk: clipboard: could not run wl-copy: {error}");
            return;
        }
    };
    let data = glib::Bytes::from(entry.data.as_ref());

    process.communicate_async(Some(&data), gio::Cancellable::NONE, |result| {
        if let Err(error) = result {
            eprintln!("whelk: clipboard: wl-copy failed: {error}");
        }
    });
}

/// The short-lived program `wl-paste --watch` runs.  It drains the watcher's
/// default representation, asks for the type list and then the richest type,
/// and writes one record to standard output.
fn clipboard_sensitive() -> bool {
    std::env::var_os("CLIPBOARD_STATE").as_deref() == Some(OsStr::new("sensitive"))
}

fn read_type_list() -> Option<String> {
    let types = wl_paste(&["wl-paste", "--list-types"], 64 * 1024, TYPES_TIMEOUT_MS)?;

    Some(String::from_utf8_lossy(&types).into_owned())
}

pub fn helper() -> i32 {
    // `wl-paste --watch` marks password-manager selections in the
    // environment.  The helper drains the event first, so check both before
    // and after every fetch: the selection can change while it runs.
    if clipboard_sensitive() {
        return 0;
    }

    let mut input = std::io::stdin().lock();

    let _ = std::io::copy(&mut input, &mut std::io::sink());

    if clipboard_sensitive() {
        return 0;
    }

    let Some(types) = read_type_list() else {
        return 0;
    };

    if types.is_empty() {
        return 0;
    }

    if types_secret(&types) {
        eprintln!("whelk: clipboard: not storing a password manager's selection");
        return 0;
    }

    let Some(mime) = pick_mime(&types, false) else {
        return 0;
    };
    let limit = helper_limit();
    let Some(data) = wl_paste(
        &["wl-paste", "--no-newline", "--type", mime],
        limit,
        HELPER_TIMEOUT_MS,
    ) else {
        return 0;
    };

    if clipboard_sensitive() || data.is_empty() {
        return 0;
    }

    // A selection offer can be withdrawn or replaced between the type list
    // and the data fetch.  Only store a record whose type is still offered,
    // and do not re-fetch text from a different selection.
    let Some(current_types) = read_type_list() else {
        return 0;
    };

    if current_types != types || types_secret(&current_types) {
        return 0;
    }

    let text = if mime.starts_with("text/") && !mime_is_plain_text(mime) {
        pick_mime(&types, true)
            .filter(|text_mime| *text_mime != mime)
            .and_then(|text_mime| {
                wl_paste(
                    &["wl-paste", "--no-newline", "--type", text_mime],
                    limit,
                    HELPER_TIMEOUT_MS,
                )
            })
    } else {
        None
    };

    if clipboard_sensitive() {
        return 0;
    }

    let text = text.unwrap_or_default();
    let record = encode_record(mime, &data, &text);
    let mut output = std::io::stdout().lock();

    if output.write_all(&record).is_err() {
        return 1;
    }

    0
}

fn helper_limit() -> usize {
    std::env::var("WHELK_CLIPBOARD_MAX_ENTRY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value >= 1024)
        .map(|value| value.min(DATA_MAX))
        .unwrap_or(DEFAULT_MAX_HELPER_BYTES)
}

/// Run one `wl-paste` invocation with a byte budget and a deadline.  Keeping
/// the read asynchronous is what lets the timer fire while a source stalls.
fn wl_paste(argv: &[&str], limit: usize, timeout_ms: u64) -> Option<Vec<u8>> {
    let argv = argv.iter().map(OsStr::new).collect::<Vec<_>>();
    let process = gio::Subprocess::newv(
        &argv,
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE,
    )
    .ok()?;
    let Some(stream) = process.stdout_pipe() else {
        process.force_exit();

        return None;
    };
    let capture = Rc::new(Capture {
        process,
        stream,
        output: RefCell::new(Vec::new()),
        limit,
        cancellable: gio::Cancellable::new(),
        main_loop: glib::MainLoop::new(None, false),
        finished: Cell::new(false),
        success: Cell::new(false),
        timer: RefCell::new(None),
    });
    let weak = Rc::downgrade(&capture);

    *capture.timer.borrow_mut() = Some(glib::timeout_add_local_once(
        Duration::from_millis(timeout_ms),
        move || {
            if let Some(capture) = weak.upgrade() {
                capture.finish(false);
            }
        },
    ));

    capture.read_next();
    capture.main_loop.run();

    if capture.finished.get() && capture.success.get() {
        Some(capture.output.borrow_mut().clone())
    } else {
        None
    }
}

struct Capture {
    process: gio::Subprocess,
    stream: gio::InputStream,
    output: RefCell<Vec<u8>>,
    limit: usize,
    cancellable: gio::Cancellable,
    main_loop: glib::MainLoop,
    finished: Cell<bool>,
    success: Cell<bool>,
    timer: RefCell<Option<glib::SourceId>>,
}

impl Capture {
    fn read_next(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);

        self.stream.read_bytes_async(
            READ_SIZE,
            glib::Priority::DEFAULT,
            None::<&gio::Cancellable>,
            move |result| {
                if let Some(capture) = weak.upgrade() {
                    capture.on_read(result);
                }
            },
        );
    }

    fn on_read(self: &Rc<Self>, result: Result<glib::Bytes, glib::Error>) {
        if self.finished.get() {
            return;
        }

        match result {
            Ok(bytes) if bytes.is_empty() => self.finish(true),
            Ok(bytes) => {
                let mut output = self.output.borrow_mut();

                if output.len() + bytes.len() > self.limit {
                    drop(output);
                    self.finish(false);
                    return;
                }

                output.extend_from_slice(bytes.as_ref());
                drop(output);
                self.read_next();
            }
            Err(_) => self.finish(false),
        }
    }

    fn finish(&self, success: bool) {
        if self.finished.replace(true) {
            return;
        }

        self.success.set(success);
        self.cancellable.cancel();
        self.process.force_exit();

        if let Some(timer) = self.timer.borrow_mut().take() {
            sources::remove(timer);
        }

        self.main_loop.quit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_saves_create_a_private_parent_and_file() {
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("whelk").join("clipboard-history");

        write_private(&path, b"secret").expect("private write");

        assert_eq!(std::fs::read(&path).expect("read back"), b"secret");
        assert_eq!(
            std::fs::metadata(&path).expect("metadata").mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(path.parent().expect("parent"))
                .expect("parent metadata")
                .mode()
                & 0o777,
            0o700
        );
    }

    #[test]
    fn a_public_parent_is_refused() {
        let root = tempfile::tempdir().expect("temporary directory");
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755))
            .expect("chmod");

        let error = write_private(&root.path().join("history"), b"secret")
            .expect_err("a public persistence directory is refused");

        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(!root.path().join("history").exists());
    }

    #[test]
    fn records_round_trip_including_nul_bytes() {
        let mut pending = encode_record("image/png", b"\x89PNG\0\xff", b"fallback");
        let mut history = History::new();

        assert_eq!(parse_records(&mut pending, &mut history), 1);
        assert!(pending.is_empty());
        assert_eq!(history.len(), 1);
        assert_eq!(history.entries()[0].mime, "image/png");
        assert_eq!(history.entries()[0].data.as_ref(), b"\x89PNG\0\xff");
        assert_eq!(history.entries()[0].text, "fallback");
    }

    #[test]
    fn history_keeps_pins_and_enforces_both_budgets() {
        let mut history = History::new();

        history.set_limits(2, 1024, 64);
        history.add(Entry::new("text/plain", "one", b"1".to_vec()));
        history.add(Entry::new("text/plain", "two", b"22".to_vec()));
        history.add(Entry::new("text/plain", "three", b"333".to_vec()));

        assert_eq!(history.len(), 2);
        assert_eq!(history.entries()[0].text, "three");
        assert_eq!(history.entries()[1].text, "two");
        assert_eq!(history.bytes(), 5);

        let two = history.entries()[1].id.clone();

        assert!(history.set_pinned(&two, true));
        history.add(Entry::new("text/plain", "four", b"4444".to_vec()));

        assert_eq!(history.len(), 3);
        assert!(history.find(&two).is_some_and(|entry| entry.pinned));
        assert_eq!(history.bytes(), 9);

        let four = history.entries()[0].id.clone();

        assert!(history.remove(&four));
        assert_eq!(history.len(), 2);
        assert!(history.undo_remove());
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn expiry_keeps_pinned_entries_and_fresh_copies() {
        let mut history = History::new();
        let now = 10_000_000_000_i64;
        let mut old = Entry::new("text/plain", "old", b"old".to_vec());
        let mut fresh = Entry::new("text/plain", "fresh", b"fresh".to_vec());
        let mut pinned = Entry::new("text/plain", "pinned", b"pinned".to_vec());

        old.created_us = now - 2_000_000;
        fresh.created_us = now - 500_000;
        pinned.created_us = now - 3_000_000;
        pinned.pinned = true;
        history.add(old);
        history.add(fresh);
        history.add(pinned);

        assert_eq!(history.prune(1, now), 1);
        assert_eq!(history.len(), 2);
        assert!(history.entries().iter().any(|entry| entry.text == "fresh"));
        assert!(history.entries().iter().any(|entry| entry.text == "pinned"));
    }

    #[test]
    fn persistence_round_trips_v2_entries() {
        let mut history = History::new();
        let mut pinned = Entry::new("text/html", "plain", b"<b>rich</b>".to_vec());

        pinned.pinned = true;
        pinned.created_us = 42;
        history.add(Entry::new("text/plain", "first", b"first".to_vec()));
        history.add(pinned.clone());

        let decoded =
            decode_history(&serialize_history(&history, false), false).expect("v2 history");

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].id, pinned.id);
        assert!(decoded[0].pinned);
        assert_eq!(decoded[0].created_us, 42);
        assert_eq!(decoded[0].data.as_ref(), b"<b>rich</b>");

        let pinned_only =
            decode_history(&serialize_history(&history, true), false).expect("pinned history");

        assert_eq!(pinned_only.len(), 1);
        assert_eq!(pinned_only[0].id, pinned.id);
    }

    #[test]
    fn environment_persistence_only_fills_an_unconfigured_policy() {
        let unconfigured = ClipboardSettings::default();
        let configured = ClipboardSettings {
            configured: true,
            persist: None,
            ..ClipboardSettings::default()
        };

        assert_eq!(
            effective_settings(&unconfigured, Some("auto".to_owned()))
                .persist
                .as_deref(),
            Some("auto")
        );
        assert!(
            effective_settings(&configured, Some("auto".to_owned()))
                .persist
                .is_none()
        );
    }

    #[test]
    fn lock_hint_reads_only_the_session_boolean() {
        let changed = HashMap::from([("LockedHint".to_owned(), true.to_variant())]);
        let parameters = (
            "org.freedesktop.login1.Session".to_owned(),
            changed,
            Vec::<String>::new(),
        )
            .to_variant();

        assert!(locked_hint(&parameters));

        let changed = HashMap::from([("LockedHint".to_owned(), false.to_variant())]);
        let parameters = (
            "org.freedesktop.login1.Session".to_owned(),
            changed,
            Vec::<String>::new(),
        )
            .to_variant();

        assert!(!locked_hint(&parameters));
    }

    #[test]
    fn v1_history_still_loads_without_ids() {
        let data = glib::Variant::array_from_fixed_array(b"legacy");
        let item = glib::Variant::tuple_from_iter([
            "text/plain".to_variant(),
            "legacy".to_variant(),
            data,
            false.to_variant(),
        ]);
        let array = glib::Variant::array_from_iter_with_type(
            glib::VariantTy::new("(ssayb)").expect("static type"),
            [item],
        );
        let root = glib::Variant::tuple_from_iter(["whelk-clipboard-1".to_variant(), array]);
        let decoded = decode_history(root.data_as_bytes().as_ref(), false).expect("v1 history");

        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].text, "legacy");
        assert_eq!(decoded[0].data.as_ref(), b"legacy");
        assert!(!decoded[0].id.is_empty());
    }

    #[test]
    fn persistence_writes_a_private_v2_file() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("state/clipboard.variant");
        let settings = ClipboardSettings {
            configured: true,
            persist: Some(path.display().to_string()),
            ..ClipboardSettings::default()
        };
        let clipboard = Clipboard::new(settings);

        clipboard.load_persistence();

        assert!(clipboard.add("text/plain", "saved", b"saved".to_vec()));
        clipboard.flush();

        let mode = std::fs::metadata(&path)
            .expect("history file")
            .permissions()
            .mode()
            & 0o777;
        let bytes = std::fs::read(&path).expect("history bytes");
        let entries = decode_history(&bytes, false).expect("v2 history");

        assert_eq!(mode, 0o600);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].text, "saved");
    }

    #[test]
    fn malformed_persistence_is_not_loaded() {
        assert!(decode_history(b"not a variant", false).is_none());
    }

    #[test]
    fn labels_distinguish_rich_content_and_file_lists() {
        let mut image = Entry::new("image/png", "", vec![1, 2, 3]);
        let files = Entry::new("text/uri-list", "file:///a\nfile:///b\n", Vec::new());
        let text = Entry::new(
            "text/plain",
            "first line\nsecond",
            b"first line\nsecond".to_vec(),
        );

        assert_eq!(entry_label(&image), "[Image — image/png]");
        assert_eq!(entry_label(&files), "2 files");
        assert_eq!(entry_label(&text), "first line");
        assert_eq!(format_size(2048), "2.0 KiB");
        assert_eq!(format_age(1_000_000, 301_000_000), "5m ago");
        assert_eq!(mime_kind("text/html"), "HTML");

        image.pinned = true;
        assert!(entry_detail(&image).starts_with("Pinned · Image"));
    }

    #[test]
    fn incomplete_records_wait_and_malformed_ones_are_dropped() {
        let record = encode_record("text/plain", b"hello", b"hello");
        let mut pending = record[..record.len() - 2].to_vec();
        let mut history = History::new();

        assert_eq!(parse_records(&mut pending, &mut history), 0);
        assert_eq!(pending, record[..record.len() - 2]);

        pending.extend_from_slice(&record[record.len() - 2..]);
        assert_eq!(parse_records(&mut pending, &mut history), 1);
        assert_eq!(history.len(), 1);

        pending.extend_from_slice(b"WHELK1 2 1\nxx");
        assert_eq!(parse_records(&mut pending, &mut history), 0);
        assert!(pending.is_empty());
    }
    #[test]
    fn the_status_line_uses_the_c_spelling() {
        let clipboard = Clipboard::new(ClipboardSettings::default());

        assert_eq!(
            clipboard.status_line(),
            "Clipboard listener stopped; 0 entries; 0 bytes"
        );

        assert!(clipboard.add("text/plain", "one", b"1".to_vec()));
        assert_eq!(
            clipboard.status_line(),
            "Clipboard listener stopped; 1 entry; 1 bytes"
        );
    }
}
