// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The tray's display-free rules, ported from `modules/tray.c`.
//!
//! The watcher names an item as `service:/object/path` or just a bus name, the
//! user's `#:hide` names fragments rather than ids, and a bar with its own
//! Wi-Fi or Bluetooth reading suppresses the matching applet by exact id.  The
//! pixmap conversion is here too: it is byte arithmetic, and a malformed
//! `IconPixmap` should be rejected rather than painted over the shell.

use crate::applets;

pub const DEFAULT_MAX_ITEMS: usize = 6;
pub const DEFAULT_ITEM_SIZE: i32 = 16;

/// Splits a watcher entry into its bus name and object path.  A bare bus name
/// uses the protocol's default item path.
pub fn split_entry(entry: &str) -> Option<(String, String)> {
    let entry = entry.trim();

    if entry.is_empty() {
        return None;
    }

    if let Some(slash) = entry.find('/') {
        let service = &entry[..slash];

        if service.is_empty() {
            return None;
        }

        Some((service.to_owned(), entry[slash..].to_owned()))
    } else {
        Some((entry.to_owned(), "/StatusNotifierItem".to_owned()))
    }
}

/// The configured `#:hide` words, trimmed and without empty entries.
pub fn hide_fragments(hide: &str) -> Vec<String> {
    hide.split(',')
        .map(str::trim)
        .filter(|fragment| !fragment.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Whether an item is one the bar already speaks for.
///
/// An explicit `#:hide` wins: it is the user's rule, and it looks for words in
/// both the id and the title.  Without one, only the exact network or Bluetooth
/// applet ids are hidden, never a title that merely mentions them.
pub fn should_hide(
    id: Option<&str>,
    title: Option<&str>,
    hide: &str,
    native_network: bool,
    native_bluetooth: bool,
) -> bool {
    // Even a non-empty rule with no usable words is still an explicit rule:
    // it suppresses the automatic applet list, exactly as the C module does.
    if !hide.is_empty() {
        let fragments = hide_fragments(hide);
        let id = id.unwrap_or_default().to_ascii_lowercase();
        let title = title.unwrap_or_default().to_ascii_lowercase();

        return fragments
            .iter()
            .any(|fragment| id.contains(fragment) || title.contains(fragment));
    }

    id.is_some_and(|id| {
        (native_network && applets::is_network(id))
            || (native_bluetooth && applets::is_bluetooth(id))
    })
}

/// The configured limit, with the C module's minimum of one.
pub fn max_items(value: Option<i64>) -> usize {
    usize::try_from(value.unwrap_or(DEFAULT_MAX_ITEMS as i64).max(1)).unwrap_or(usize::MAX)
}

pub fn visible_count(total: usize, limit: usize) -> usize {
    total.min(limit.max(1))
}

pub fn needs_overflow(total: usize, limit: usize) -> bool {
    total > limit.max(1)
}

/// A decoded StatusNotifierItem pixmap: width, height and premultiplied RGBA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixmap {
    pub width: i32,
    pub height: i32,
    pub rgba: Vec<u8>,
}

/// The protocol's pixmap is ARGB32 in network byte order.  The shell paints
/// premultiplied RGBA, so each pixel's bytes are reordered.  A width, height or
/// byte count that cannot describe an image is refused instead of trusted.
pub fn argb32_pixmap_to_rgba(width: i32, height: i32, bytes: &[u8]) -> Option<Pixmap> {
    if width <= 0 || height <= 0 {
        return None;
    }

    let pixels = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    let expected = pixels.checked_mul(4)?;

    if bytes.len() < expected {
        return None;
    }

    let mut rgba = Vec::with_capacity(expected);

    for index in 0..pixels {
        let offset = index * 4;
        let pixel = &bytes[offset..offset + 4];

        // `A` is the first byte in the wire format, then `R`, `G`, `B`.
        rgba.extend_from_slice(&[pixel[1], pixel[2], pixel[3], pixel[0]]);
    }

    Some(Pixmap {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_split_at_the_first_slash() {
        assert_eq!(
            split_entry("org.example.App/StatusNotifierItem"),
            Some((
                "org.example.App".to_owned(),
                "/StatusNotifierItem".to_owned()
            ))
        );
        assert_eq!(
            split_entry("org.example.App"),
            Some((
                "org.example.App".to_owned(),
                "/StatusNotifierItem".to_owned()
            ))
        );
        assert_eq!(split_entry("/StatusNotifierItem"), None);
        assert_eq!(split_entry("  "), None);
    }

    #[test]
    fn fragments_are_trimmed_and_case_insensitive() {
        assert_eq!(
            hide_fragments(" Nextcloud, ,Steam "),
            vec!["nextcloud".to_owned(), "steam".to_owned()]
        );
        assert!(should_hide(
            Some("org.nextcloud.desktopclient"),
            None,
            "Nextcloud",
            false,
            false
        ));
        assert!(should_hide(
            None,
            Some("Steam status"),
            "steam",
            false,
            false
        ));
    }

    #[test]
    fn an_explicit_hide_overrides_native_suppression() {
        assert!(!should_hide(Some("nm-applet"), None, "Steam", true, false));
        assert!(should_hide(Some("nm-applet"), None, "", true, false));
        assert!(should_hide(Some("blueman"), None, "", false, true));
        assert!(should_hide(Some("NM-APPLET"), None, "", true, false));
        assert!(!should_hide(Some("nm-applet"), None, " , ", true, false));
    }

    #[test]
    fn automatic_hiding_only_matches_exact_applet_ids() {
        assert!(!should_hide(
            Some("org.example.network"),
            Some("Network"),
            "",
            true,
            true
        ));
        assert!(!should_hide(None, Some("nm-applet"), "", true, false));
    }

    #[test]
    fn the_item_limit_is_at_least_one() {
        assert_eq!(max_items(None), 6);
        assert_eq!(max_items(Some(0)), 1);
        assert_eq!(max_items(Some(-4)), 1);
        assert_eq!(max_items(Some(3)), 3);
        assert_eq!(visible_count(8, 3), 3);
        assert_eq!(visible_count(2, 3), 2);
        assert!(needs_overflow(7, 6));
        assert!(!needs_overflow(6, 6));
    }

    #[test]
    fn pixmaps_are_reordered_from_argb_to_rgba() {
        let pixmap =
            argb32_pixmap_to_rgba(1, 1, &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]).unwrap();

        assert_eq!(pixmap.width, 1);
        assert_eq!(pixmap.height, 1);
        assert_eq!(pixmap.rgba, vec![0x22, 0x33, 0x44, 0x11]);
    }

    #[test]
    fn malformed_pixmaps_are_refused() {
        assert_eq!(argb32_pixmap_to_rgba(0, 1, &[0; 4]), None);
        assert_eq!(argb32_pixmap_to_rgba(1, -1, &[0; 4]), None);
        assert_eq!(argb32_pixmap_to_rgba(2, 2, &[0; 15]), None);
        assert_eq!(argb32_pixmap_to_rgba(i32::MAX, i32::MAX, &[]), None);
    }
}
