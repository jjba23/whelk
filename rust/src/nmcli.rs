// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The pieces of `nmcli`'s output and input a panel needs, ported from
//! `util/nmcli.c`.

/// One line of `nmcli -t` output, split on `:` while honouring the backslash
/// escapes nmcli uses for a `:` or `\\` inside a field.
pub fn split_terse(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut characters = line.chars();

    while let Some(character) = characters.next() {
        if character == '\\' {
            if let Some(escaped) = characters.next() {
                current.push(escaped);
                continue;
            }

            // A trailing backslash has nothing to escape, so it is data.
            current.push('\\');
            continue;
        }

        if character == ':' {
            fields.push(current);
            current = String::new();
            continue;
        }

        current.push(character);
    }

    fields.push(current);
    fields
}

/// The names of the Wi-Fi profiles in `nmcli -t -f NAME,TYPE connection show`
/// output, in the order nmcli listed them.
pub fn wifi_profile_names(terse_output: &str) -> Vec<String> {
    let mut names = Vec::new();

    for line in terse_output.lines() {
        let fields = split_terse(line);

        if fields.len() < 2 {
            continue;
        }

        let name = &fields[0];
        let kind = &fields[1];

        if !name.is_empty() && kind == "802-11-wireless" {
            names.push(name.clone());
        }
    }

    names
}

/// The connected reading's format: the icon alone, or the icon and the SSID.
/// Hiding the name is the default, so a bar does not announce which network
/// the machine is on to anyone who can see the screen.
pub fn wifi_format(show_ssid: bool) -> &'static str {
    if show_ssid { "{icon} {ssid}" } else { "{icon}" }
}

/// The connected reading's hover text.  It names the network only when
/// `show_ssid` is true; the signal is always there.
pub fn wifi_tooltip(show_ssid: bool) -> &'static str {
    if show_ssid {
        "{ssid} · {signal}%\nClick: networks"
    } else {
        "Wi-Fi · {signal}%\nClick: networks"
    }
}

/// The script the interactive editor (`nmcli connection edit`) reads on its
/// standard input to replace a saved key, or `None` when the key cannot be
/// written that way.
///
/// The editor takes the rest of the line as the value, literally: quotes are
/// part of the key, not syntax around it.  That suits a secret, which must
/// never be on a command line where `ps` would show it, but it also means a
/// key with a newline, a control character, or leading/trailing whitespace has
/// no faithful spelling -- such a key is refused rather than stored wrong.
pub fn key_update_script(psk: &str) -> Option<String> {
    if psk.is_empty() {
        return None;
    }

    // The editor trims the value's edges, so a key that begins or ends in
    // whitespace cannot be written without changing it.
    if psk.as_bytes().first().is_some_and(u8::is_ascii_whitespace)
        || psk.as_bytes().last().is_some_and(u8::is_ascii_whitespace)
    {
        return None;
    }

    // A newline would end the `set` command and start another; a control
    // character cannot appear in a keyfile at all.  Refuse rather than
    // truncate: a saved key that is not the typed one is worse than a refusal.
    if psk.bytes().any(|byte| byte < 0x20 || byte == 0x7f) {
        return None;
    }

    Some(format!(
        "set 802-11-wireless-security.psk {psk}\nsave\nquit\n"
    ))
}
