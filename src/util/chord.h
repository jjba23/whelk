// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_CHORD_H
#define WHELK_CHORD_H

#include <glib.h>

/* =========================================================================
 * KEY CHORDS
 * =========================================================================
 * A chord is a key and the modifiers held with it, and it is written two ways
 * on a desktop.  CUA writes "Ctrl+Shift+P"; Emacs writes "C-S-p", and its
 * documentation and its users write "M-F4", "C-x", "s-p" -- one letter and a
 * dash per modifier, in any order.
 *
 * The shell reads both, because the menu dictionary is configuration and a
 * person who lives in Emacs should not have to translate their own muscle
 * memory into another notation to add a menu entry.  What a chord is stored as
 * is therefore not what it is shown as: the dictionary may write "C-v", the
 * menu may display "Ctrl+V" or "C-v", and what gets typed into the focused
 * window is the same key either way.
 *
 * Parsing produces the key name wtype and xdotool understand ("v", "F4",
 * "Return", "plus"), not the character on the key cap.
 */

typedef enum {
  WHELK_CHORD_CUA,   /* Ctrl+Shift+P, Alt+F4 */
  WHELK_CHORD_EMACS  /* C-S-p, M-F4 */
} WhelkChordStyle;

typedef struct {
  gboolean ctrl;
  gboolean alt;
  gboolean shift;
  gboolean super;
  /* The key name to type, owned by the chord. */
  char *key;
} WhelkChord;

/**
 * whelk_chord_parse:
 * @text: a chord in Emacs notation ("C-v", "M-F4", "C-M-x", "s-p"), CUA
 *   notation ("Ctrl+V", "Alt+F4", "Ctrl++"), or a bare key ("F11", "Space").
 * @chord: (out caller-allocates): the chord, to be cleared with
 *   whelk_chord_clear().
 *
 * Returns: TRUE when the text is a chord this shell can type.  A key name it
 *   does not know is still accepted -- wtype knows more keysyms than this file
 *   does -- but a chord with no key, or a key with whitespace in it, is not.
 */
gboolean whelk_chord_parse(const char *text, WhelkChord *chord);

/**
 * whelk_chord_parse_gtk:
 * @text: an accelerator as GTK writes it in a menu model, "<Primary>q",
 *   "<Shift><Control>a" or "<Alt>F4".
 * @chord: (out caller-allocates): the chord.
 *
 * The accelerator attribute of an application's exported menubar is in GTK's
 * spelling, not either of the two a person writes.  Returns: TRUE when it could
 * be read.
 */
gboolean whelk_chord_parse_gtk(const char *text, WhelkChord *chord);

/* Releases the chord's key; the chord is reusable afterwards. */
void whelk_chord_clear(WhelkChord *chord);

/* "ctrl alt shift super", or NULL when nothing is held.  The order wtype and
 * xdotool are given. */
char *whelk_chord_modifiers(const WhelkChord *chord);

/* The chord as a menubar shows it: "Ctrl+Shift+P" or "C-S-p". */
char *whelk_chord_format(const WhelkChord *chord, WhelkChordStyle style);

/**
 * whelk_chord_display:
 * @text: a chord in any notation this file reads, or text it does not.
 * @style: how to write it back.
 *
 * Parses @text and writes it in @style, so a dictionary that stores "C-v" can
 * be shown as "Ctrl+V".  Text that is not a chord is returned unchanged: a
 * menu entry's printed chord is never worse than what was written for it.
 *
 * Returns: (transfer full): the display text.
 */
char *whelk_chord_display(const char *text, WhelkChordStyle style);

/**
 * whelk_chord_command:
 * @tool: (nullable): "wtype", "xdotool", or a path to either.
 * @chord: the chord to type.
 *
 * Returns: (transfer full) (nullable): the command that types @chord into the
 *   focused window, or NULL when there is no tool or no key.
 */
char *whelk_chord_command(const char *tool, const WhelkChord *chord);

/* A style from its configuration spelling: "emacs", or CUA for anything else. */
WhelkChordStyle whelk_chord_style_from_name(const char *name);

/* "emacs" or "cua". */
const char *whelk_chord_style_name(WhelkChordStyle style);

#endif // WHELK_CHORD_H
