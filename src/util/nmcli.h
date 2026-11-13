// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_NMCLI_H
#define WHELK_NMCLI_H

#include <glib.h>

/* =========================================================================
 * NMCLI'S ANSWERS
 * =========================================================================
 * The pieces of `nmcli`'s output and input a panel needs, kept away from the
 * widgets so they can be tested without a display.
 */

/**
 * whelk_nmcli_split_terse:
 * @line: one line of `nmcli -t` output.
 *
 * Splits it on ':' while honouring the backslash escapes nmcli uses for a ':'
 * or '\' inside a field (an SSID is allowed to contain either).  The result is
 * a #GPtrArray of newly allocated strings, freed with g_ptr_array_unref().
 */
GPtrArray *whelk_nmcli_split_terse(const char *line);

/**
 * whelk_nmcli_wifi_profile_names:
 * @terse_output: the whole output of
 *   `nmcli -t -f NAME,TYPE connection show`.
 *
 * The names of the Wi-Fi profiles in it, in the order nmcli listed them.
 */
GPtrArray *whelk_nmcli_wifi_profile_names(const char *terse_output);

/**
 * whelk_nmcli_wifi_format:
 * @show_ssid: whether the network's name may be shown.
 *
 * The connected reading's format: the icon alone, or the icon and the SSID.
 * Hiding the name is the default, so a bar does not announce which network
 * the machine is on to anyone who can see the screen.
 */
const char *whelk_nmcli_wifi_format(gboolean show_ssid);

/**
 * whelk_nmcli_wifi_tooltip:
 * @show_ssid: whether the network's name may be shown.
 *
 * The connected reading's hover text.  It names the network only when
 * @show_ssid is true; the signal is always there.
 */
const char *whelk_nmcli_wifi_tooltip(gboolean show_ssid);

/**
 * whelk_nmcli_key_update_script:
 * @psk: a Wi-Fi passphrase the user typed.
 *
 * The script the interactive editor (`nmcli connection edit`) reads on its
 * standard input to replace a saved key, or NULL when the key cannot be
 * written that way.
 *
 * The editor takes the rest of the line as the value, literally: quotes are
 * part of the key, not syntax around it.  That suits a secret, which must
 * never be on a command line where `ps` would show it, but it also means a key
 * with a newline (which would end the `set` command), a control character, or
 * leading/trailing whitespace (which the editor trims) has no faithful
 * spelling -- such a key is refused rather than stored wrong.
 */
char *whelk_nmcli_key_update_script(const char *psk);

#endif // WHELK_NMCLI_H
