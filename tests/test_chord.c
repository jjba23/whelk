// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/chord.h"

#include <glib.h>

/* A chord is written two ways and typed one way.  These cases are the two
 * notations, the keys whose names are not their glyphs, and the spelling GTK
 * puts in an exported menubar. */

static void test_emacs_notation(void) {
  WhelkChord chord;

  g_assert_true(whelk_chord_parse("C-v", &chord));
  g_assert_true(chord.ctrl);
  g_assert_false(chord.alt);
  g_assert_false(chord.shift);
  g_assert_cmpstr(chord.key, ==, "v");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse("M-F4", &chord));
  g_assert_true(chord.alt);
  g_assert_cmpstr(chord.key, ==, "F4");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse("C-M-x", &chord));
  g_assert_true(chord.ctrl);
  g_assert_true(chord.alt);
  g_assert_cmpstr(chord.key, ==, "x");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse("C-S-p", &chord));
  g_assert_true(chord.ctrl);
  g_assert_true(chord.shift);
  g_assert_cmpstr(chord.key, ==, "p");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse("s-p", &chord));
  g_assert_true(chord.super);
  g_assert_cmpstr(chord.key, ==, "p");
  whelk_chord_clear(&chord);

  /* The key may be a plus or a minus, which is why the prefixes are read
   * before the separators. */
  g_assert_true(whelk_chord_parse("C-+", &chord));
  g_assert_true(chord.ctrl);
  g_assert_cmpstr(chord.key, ==, "plus");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse("C--", &chord));
  g_assert_true(chord.ctrl);
  g_assert_cmpstr(chord.key, ==, "minus");
  whelk_chord_clear(&chord);
}

static void test_cua_notation(void) {
  WhelkChord chord;

  g_assert_true(whelk_chord_parse("Ctrl+Shift+P", &chord));
  g_assert_true(chord.ctrl);
  g_assert_true(chord.shift);
  g_assert_cmpstr(chord.key, ==, "p");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse("Alt+F4", &chord));
  g_assert_true(chord.alt);
  g_assert_cmpstr(chord.key, ==, "F4");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse("Ctrl++", &chord));
  g_assert_true(chord.ctrl);
  g_assert_cmpstr(chord.key, ==, "plus");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse("Ctrl+-", &chord));
  g_assert_true(chord.ctrl);
  g_assert_cmpstr(chord.key, ==, "minus");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse("Ctrl+,", &chord));
  g_assert_cmpstr(chord.key, ==, "comma");
  whelk_chord_clear(&chord);
}

static void test_bare_and_named_keys(void) {
  WhelkChord chord;

  g_assert_true(whelk_chord_parse("F11", &chord));
  g_assert_false(chord.ctrl);
  g_assert_cmpstr(chord.key, ==, "F11");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse("Space", &chord));
  g_assert_cmpstr(chord.key, ==, "space");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse("RET", &chord));
  g_assert_cmpstr(chord.key, ==, "Return");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse("DEL", &chord));
  g_assert_cmpstr(chord.key, ==, "BackSpace");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse("Page_Down", &chord));
  g_assert_cmpstr(chord.key, ==, "Page_Down");
  whelk_chord_clear(&chord);
}

static void test_refusals(void) {
  WhelkChord chord;

  g_assert_false(whelk_chord_parse(NULL, &chord));
  g_assert_false(whelk_chord_parse("", &chord));
  g_assert_false(whelk_chord_parse("C-", &chord));
  g_assert_false(whelk_chord_parse("Ctrl+NotAModifier+V", &chord));
}

static void test_gtk_accelerators(void) {
  WhelkChord chord;

  g_assert_true(whelk_chord_parse_gtk("<Primary>q", &chord));
  g_assert_true(chord.ctrl);
  g_assert_cmpstr(chord.key, ==, "q");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse_gtk("<Shift><Control>a", &chord));
  g_assert_true(chord.ctrl);
  g_assert_true(chord.shift);
  g_assert_cmpstr(chord.key, ==, "a");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse_gtk("<Alt>F4", &chord));
  g_assert_true(chord.alt);
  g_assert_cmpstr(chord.key, ==, "F4");
  whelk_chord_clear(&chord);
}

static void test_formatting(void) {
  WhelkChord chord;

  g_assert_true(whelk_chord_parse("C-S-p", &chord));

  g_autofree char *cua = whelk_chord_format(&chord, WHELK_CHORD_CUA);
  g_autofree char *emacs = whelk_chord_format(&chord, WHELK_CHORD_EMACS);

  g_assert_cmpstr(cua, ==, "Ctrl+Shift+P");
  g_assert_cmpstr(emacs, ==, "C-S-p");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse("Ctrl++", &chord));
  g_autofree char *plus_cua = whelk_chord_format(&chord, WHELK_CHORD_CUA);
  g_autofree char *plus_emacs = whelk_chord_format(&chord, WHELK_CHORD_EMACS);

  g_assert_cmpstr(plus_cua, ==, "Ctrl++");
  g_assert_cmpstr(plus_emacs, ==, "C-+");
  whelk_chord_clear(&chord);
}

static void test_display(void) {
  /* The same stored chord, written the way either kind of user reads it. */
  g_autofree char *cua = whelk_chord_display("C-v", WHELK_CHORD_CUA);
  g_autofree char *emacs = whelk_chord_display("Ctrl+V", WHELK_CHORD_EMACS);
  g_autofree char *gtk = whelk_chord_display("<Primary>q", WHELK_CHORD_EMACS);
  g_autofree char *bare = whelk_chord_display("Numpad1", WHELK_CHORD_CUA);

  g_assert_cmpstr(cua, ==, "Ctrl+V");
  g_assert_cmpstr(emacs, ==, "C-v");
  g_assert_cmpstr(gtk, ==, "C-q");
  g_assert_cmpstr(bare, ==, "Numpad 1");

  /* What is not a chord is not mangled. */
  g_autofree char *text = whelk_chord_display("No chord", WHELK_CHORD_CUA);

  g_assert_cmpstr(text, ==, "No chord");
}

static void test_commands(void) {
  WhelkChord chord;

  g_assert_true(whelk_chord_parse("C-S-p", &chord));

  g_autofree char *wtype = whelk_chord_command("wtype", &chord);
  g_autofree char *x11 = whelk_chord_command("xdotool", &chord);

  g_assert_cmpstr(wtype, ==, "wtype -M ctrl shift -k p -m ctrl shift");
  g_assert_cmpstr(x11, ==, "xdotool key ctrl+shift+p");
  whelk_chord_clear(&chord);

  g_assert_true(whelk_chord_parse("F11", &chord));
  g_autofree char *plain = whelk_chord_command("wtype", &chord);

  g_assert_cmpstr(plain, ==, "wtype -k F11");
  whelk_chord_clear(&chord);

  g_assert_null(whelk_chord_command(NULL, &chord));
}

static void test_styles(void) {
  g_assert_cmpint(whelk_chord_style_from_name("emacs"), ==, WHELK_CHORD_EMACS);
  g_assert_cmpint(whelk_chord_style_from_name("Emacs"), ==, WHELK_CHORD_EMACS);
  g_assert_cmpint(whelk_chord_style_from_name("cua"), ==, WHELK_CHORD_CUA);
  g_assert_cmpint(whelk_chord_style_from_name(NULL), ==, WHELK_CHORD_CUA);
  g_assert_cmpstr(whelk_chord_style_name(WHELK_CHORD_EMACS), ==, "emacs");
  g_assert_cmpstr(whelk_chord_style_name(WHELK_CHORD_CUA), ==, "cua");
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/chord/emacs", test_emacs_notation);
  g_test_add_func("/chord/cua", test_cua_notation);
  g_test_add_func("/chord/bare", test_bare_and_named_keys);
  g_test_add_func("/chord/refusals", test_refusals);
  g_test_add_func("/chord/gtk", test_gtk_accelerators);
  g_test_add_func("/chord/formatting", test_formatting);
  g_test_add_func("/chord/display", test_display);
  g_test_add_func("/chord/commands", test_commands);
  g_test_add_func("/chord/styles", test_styles);

  return g_test_run();
}
