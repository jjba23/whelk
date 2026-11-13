// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/chord.h"

#include <string.h>

/* =========================================================================
 * THE KEY NAMES
 * =========================================================================
 * Menus write "Return" or "RET", CUA writes "Delete", Emacs writes "DEL" for
 * the backspace key, and wtype wants a keysym name.  One table, read
 * case-insensitively, is the whole translation.
 */

typedef struct {
  const char *written;
  const char *key;
} KeyAlias;

static const KeyAlias key_aliases[] = {
    {"ret", "Return"},       {"return", "Return"},   {"enter", "Return"},
    {"tab", "Tab"},          {"spc", "space"},       {"space", "space"},
    {"esc", "Escape"},       {"escape", "Escape"},   {"del", "BackSpace"},
    {"bs", "BackSpace"},     {"backspace", "BackSpace"},
    {"delete", "Delete"},    {"ins", "Insert"},      {"insert", "Insert"},
    {"home", "Home"},        {"end", "End"},         {"pgup", "Page_Up"},
    {"prior", "Page_Up"},    {"pageup", "Page_Up"},  {"page_up", "Page_Up"},
    {"pgdn", "Page_Down"},   {"next", "Page_Down"},  {"pagedown", "Page_Down"},
    {"page_down", "Page_Down"},
    {"left", "Left"},        {"right", "Right"},     {"up", "Up"},
    {"down", "Down"},        {"plus", "plus"},       {"+", "plus"},
    {"minus", "minus"},      {"-", "minus"},         {"equal", "equal"},
    {"=", "equal"},          {"comma", "comma"},     {",", "comma"},
    {"period", "period"},    {".", "period"},        {"slash", "slash"},
    {"/", "slash"},          {"semicolon", "semicolon"}, {";", "semicolon"},
    {"apostrophe", "apostrophe"}, {"'", "apostrophe"},
    {"backslash", "backslash"},   {"\\", "backslash"},
    {"bracketleft", "bracketleft"}, {"[", "bracketleft"},
    {"bracketright", "bracketright"}, {"]", "bracketright"},
    {"grave", "grave"},      {"`", "grave"},
};

/* The display of a keysym, which is not the keysym: a menu says "+", not
 * "plus", and "Page Up", not "Page_Up". */
typedef struct {
  const char *key;
  const char *written;
} KeyDisplay;

static const KeyDisplay key_displays[] = {
    {"plus", "+"},          {"minus", "-"},        {"equal", "="},
    {"comma", ","},         {"period", "."},       {"slash", "/"},
    {"semicolon", ";"},     {"apostrophe", "'"},   {"backslash", "\\"},
    {"bracketleft", "["},   {"bracketright", "]"}, {"grave", "`"},
    {"space", "Space"},     {"BackSpace", "Backspace"},
    {"Page_Up", "Page Up"}, {"Page_Down", "Page Down"},
};

static const char *lookup_alias(const char *written) {
  for (guint i = 0; i < G_N_ELEMENTS(key_aliases); i++)
    if (g_ascii_strcasecmp(written, key_aliases[i].written) == 0)
      return key_aliases[i].key;

  return NULL;
}

/* A single character as the keysym name that types it. */
static const char *symbol_key(char symbol) {
  switch (symbol) {
  case '+':
    return "plus";
  case '-':
    return "minus";
  case '=':
    return "equal";
  case ',':
    return "comma";
  case '.':
    return "period";
  case '/':
    return "slash";
  case ';':
    return "semicolon";
  case '\'':
    return "apostrophe";
  case '\\':
    return "backslash";
  case '[':
    return "bracketleft";
  case ']':
    return "bracketright";
  case '`':
    return "grave";
  case ' ':
    return "space";
  default:
    return NULL;
  }
}

/* The keysym name for what a person wrote, or NULL when it cannot be typed. */
static char *canonical_key(const char *written) {
  if (!written || *written == '\0')
    return NULL;

  g_autofree char *trimmed = g_strdup(written);

  g_strstrip(trimmed);

  if (*trimmed == '\0' || strpbrk(trimmed, " \t\n") != NULL)
    return NULL;

  const char *alias = lookup_alias(trimmed);

  if (alias)
    return g_strdup(alias);

  const gsize length = strlen(trimmed);

  /* A letter is typed as the letter: shift is a modifier, not a capital. */
  if (length == 1 && g_ascii_isalpha(trimmed[0]))
    return g_strdup_printf("%c", g_ascii_tolower(trimmed[0]));

  if (length == 1) {
    const char *symbol = symbol_key(trimmed[0]);

    return g_strdup(symbol ? symbol : trimmed);
  }

  /* F1 to F35: wtype and xdotool spell them with a capital F. */
  if ((trimmed[0] == 'F' || trimmed[0] == 'f') && g_ascii_isdigit(trimmed[1])) {
    gboolean digits = TRUE;

    for (const char *p = trimmed + 1; *p; p++)
      digits = digits && g_ascii_isdigit(*p);

    if (digits)
      return g_strdup_printf("F%s", trimmed + 1);
  }

  /* Numpad keys: "KP_1" is the keysym, "Numpad 1" what a menu says. */
  if (g_ascii_strncasecmp(trimmed, "numpad", 6) == 0 && trimmed[6])
    return g_strdup_printf("KP_%s", trimmed + 6);

  return g_strdup(trimmed);
}

static void chord_reset(WhelkChord *chord) {
  if (!chord)
    return;

  chord->ctrl = FALSE;
  chord->alt = FALSE;
  chord->shift = FALSE;
  chord->super = FALSE;
  chord->key = NULL;
}

void whelk_chord_clear(WhelkChord *chord) {
  if (!chord)
    return;

  g_clear_pointer(&chord->key, g_free);
}

/* =========================================================================
 * PARSING
 * ========================================================================= */

/* The last + separates the key: "Ctrl+Shift++" is Ctrl+Shift and the plus key,
 * and "Ctrl+-" is Ctrl and the minus key. */
static gboolean parse_cua(const char *text, WhelkChord *chord) {
  g_autofree char *copy = g_strdup(text);
  const gsize length = strlen(copy);
  g_autofree char *key = NULL;

  if (length == 0)
    return FALSE;

  if (copy[length - 1] == '+') {
    key = g_strdup("+");
    copy[length - 1] = '\0';

    /* "Ctrl++": the separator before the key is the second-last plus. */
    if (length >= 2 && copy[length - 2] == '+')
      copy[length - 2] = '\0';
  } else {
    char *last = strrchr(copy, '+');

    if (!last || last == copy)
      return FALSE;

    key = g_strdup(last + 1);
    *last = '\0';
  }

  g_auto(GStrv) parts = g_strsplit(copy, "+", -1);

  for (guint i = 0; parts[i]; i++) {
    g_strstrip(parts[i]);

    if (*parts[i] == '\0')
      continue;

    if (g_ascii_strcasecmp(parts[i], "ctrl") == 0 ||
        g_ascii_strcasecmp(parts[i], "control") == 0 ||
        g_ascii_strcasecmp(parts[i], "primary") == 0)
      chord->ctrl = TRUE;
    else if (g_ascii_strcasecmp(parts[i], "alt") == 0 ||
             g_ascii_strcasecmp(parts[i], "meta") == 0 ||
             g_ascii_strcasecmp(parts[i], "option") == 0)
      chord->alt = TRUE;
    else if (g_ascii_strcasecmp(parts[i], "shift") == 0)
      chord->shift = TRUE;
    else if (g_ascii_strcasecmp(parts[i], "super") == 0 ||
             g_ascii_strcasecmp(parts[i], "win") == 0 ||
             g_ascii_strcasecmp(parts[i], "cmd") == 0 ||
             g_ascii_strcasecmp(parts[i], "command") == 0)
      chord->super = TRUE;
    else
      return FALSE;
  }

  chord->key = canonical_key(key);

  return chord->key != NULL;
}

/* Emacs writes one letter and a dash per modifier, in any order.  The key is
 * whatever follows the last prefix, so "C--" is C and the minus key. */
static gboolean parse_emacs(const char *text, WhelkChord *chord) {
  const char *cursor = text;
  gboolean prefixed = FALSE;

  while (cursor[0] != '\0' && cursor[1] == '-') {
    switch (cursor[0]) {
    case 'C':
      chord->ctrl = TRUE;
      break;
    case 'M':
      chord->alt = TRUE;
      break;
    case 'S':
      chord->shift = TRUE;
      break;
    case 's':
      chord->super = TRUE;
      break;
    default:
      goto done;
    }

    prefixed = TRUE;
    cursor += 2;
  }

done:
  if (!prefixed)
    return FALSE;

  chord->key = canonical_key(cursor);

  return chord->key != NULL;
}

gboolean whelk_chord_parse(const char *text, WhelkChord *chord) {
  if (!chord)
    return FALSE;

  chord_reset(chord);

  if (!text)
    return FALSE;

  g_autofree char *trimmed = g_strdup(text);

  g_strstrip(trimmed);

  if (*trimmed == '\0')
    return FALSE;

  /* Emacs's prefixes are unmistakable, and its key may be a plus: "C-+" is
   * Ctrl and the plus key, not a CUA chord. */
  if (parse_emacs(trimmed, chord))
    return TRUE;

  chord_reset(chord);

  /* It looked like Emacs notation and had no key behind the prefixes: "C-"
   * is a typo, not a key name. */
  if (trimmed[1] == '-' && (trimmed[0] == 'C' || trimmed[0] == 'M' ||
                            trimmed[0] == 'S' || trimmed[0] == 's'))
    return FALSE;

  /* A plus is CUA's separator. */
  if (strchr(trimmed, '+') != NULL)
    return parse_cua(trimmed, chord);

  /* A bare key: "F11", "Space", "Delete". */
  chord->key = canonical_key(trimmed);

  return chord->key != NULL;
}

gboolean whelk_chord_sequence_parse(const char *text,
                                    WhelkChordSequence *sequence) {
  if (!sequence)
    return FALSE;

  sequence->chords = NULL;
  sequence->n_chords = 0;

  if (!text)
    return FALSE;

  g_auto(GStrv) parts = g_strsplit_set(text, " \t\n", -1);
  guint count = 0;

  for (guint i = 0; parts[i]; i++) {
    g_strstrip(parts[i]);

    if (*parts[i] != '\0')
      count++;
  }

  if (count == 0)
    return FALSE;

  WhelkChord *chords = g_new0(WhelkChord, count);
  guint at = 0;

  for (guint i = 0; parts[i]; i++) {
    if (*parts[i] == '\0')
      continue;

    if (!whelk_chord_parse(parts[i], &chords[at])) {
      for (guint j = 0; j <= at; j++)
        whelk_chord_clear(&chords[j]);

      g_free(chords);
      return FALSE;
    }

    at++;
  }

  sequence->chords = chords;
  sequence->n_chords = count;

  return TRUE;
}

void whelk_chord_sequence_clear(WhelkChordSequence *sequence) {
  if (!sequence)
    return;

  for (guint i = 0; i < sequence->n_chords; i++)
    whelk_chord_clear(&sequence->chords[i]);

  g_free(sequence->chords);
  sequence->chords = NULL;
  sequence->n_chords = 0;
}

char *whelk_chord_sequence_format(const WhelkChordSequence *sequence,
                                  WhelkChordStyle style) {
  if (!sequence || sequence->n_chords == 0)
    return NULL;

  GString *text = g_string_new(NULL);

  for (guint i = 0; i < sequence->n_chords; i++) {
    char *formatted = whelk_chord_format(&sequence->chords[i], style);

    if (!formatted) {
      g_string_free(text, TRUE);
      return NULL;
    }

    if (i > 0)
      g_string_append_c(text, ' ');

    g_string_append(text, formatted);
    g_free(formatted);
  }

  return g_string_free(text, FALSE);
}

char *whelk_chord_sequence_display(const char *text, WhelkChordStyle style) {
  if (!text || *text == '\0')
    return g_strdup(text ? text : "");

  /* A GTK accelerator starts with a modifier token and is one chord, not a
   * sequence.  Try it first: otherwise "<Primary>q" is an unknown bare key
   * that happens to contain no space. */
  WhelkChord chord;

  if (text[0] == '<' && whelk_chord_parse_gtk(text, &chord)) {
    char *formatted = whelk_chord_format(&chord, style);

    whelk_chord_clear(&chord);

    if (formatted)
      return formatted;
  }

  WhelkChordSequence sequence;

  if (whelk_chord_sequence_parse(text, &sequence)) {
    char *formatted = whelk_chord_sequence_format(&sequence, style);

    whelk_chord_sequence_clear(&sequence);

    if (formatted)
      return formatted;
  }

  return g_strdup(text);
}

char *whelk_chord_sequence_command(const char *tool,
                                   const WhelkChordSequence *sequence) {
  if (!tool || *tool == '\0' || !sequence || sequence->n_chords == 0)
    return NULL;

  GString *command = g_string_new(NULL);

  for (guint i = 0; i < sequence->n_chords; i++) {
    char *one = whelk_chord_command(tool, &sequence->chords[i]);

    if (!one) {
      g_string_free(command, TRUE);
      return NULL;
    }

    if (i > 0)
      g_string_append(command, " && ");

    g_string_append(command, one);
    g_free(one);
  }

  return g_string_free(command, FALSE);
}

gboolean whelk_chord_parse_gtk(const char *text, WhelkChord *chord) {
  if (!chord)
    return FALSE;

  chord_reset(chord);

  if (!text)
    return FALSE;

  const char *cursor = text;

  while (cursor[0] == '<') {
    const char *close = strchr(cursor, '>');

    if (!close)
      return FALSE;

    g_autofree char *name = g_strndup(cursor + 1, (gsize)(close - cursor - 1));

    if (g_ascii_strcasecmp(name, "Primary") == 0 ||
        g_ascii_strcasecmp(name, "Control") == 0)
      chord->ctrl = TRUE;
    else if (g_ascii_strcasecmp(name, "Shift") == 0)
      chord->shift = TRUE;
    else if (g_ascii_strcasecmp(name, "Alt") == 0 ||
             g_ascii_strcasecmp(name, "Meta") == 0)
      chord->alt = TRUE;
    else if (g_ascii_strcasecmp(name, "Super") == 0 ||
             g_ascii_strcasecmp(name, "Mod4") == 0)
      chord->super = TRUE;

    cursor = close + 1;
  }

  chord->key = canonical_key(cursor);

  return chord->key != NULL;
}

/* =========================================================================
 * WRITING A CHORD BACK
 * ========================================================================= */

static char *display_key(const char *key, WhelkChordStyle style) {
  if (!key)
    return NULL;

  for (guint i = 0; i < G_N_ELEMENTS(key_displays); i++)
    if (g_strcmp0(key, key_displays[i].key) == 0)
      return g_strdup(key_displays[i].written);

  const gsize length = strlen(key);

  if (length == 1 && g_ascii_isalpha(key[0]))
    return g_strdup_printf("%c", style == WHELK_CHORD_CUA
                                     ? g_ascii_toupper(key[0])
                                     : g_ascii_tolower(key[0]));

  if (g_str_has_prefix(key, "KP_") && key[3] != '\0')
    return g_strdup_printf("Numpad %s", key + 3);

  return g_strdup(key);
}

char *whelk_chord_modifiers(const WhelkChord *chord) {
  if (!chord)
    return NULL;

  GString *modifiers = g_string_new(NULL);

  if (chord->ctrl)
    g_string_append(modifiers, "ctrl");
  if (chord->alt) {
    if (modifiers->len > 0)
      g_string_append_c(modifiers, ' ');
    g_string_append(modifiers, "alt");
  }
  if (chord->shift) {
    if (modifiers->len > 0)
      g_string_append_c(modifiers, ' ');
    g_string_append(modifiers, "shift");
  }
  if (chord->super) {
    if (modifiers->len > 0)
      g_string_append_c(modifiers, ' ');
    g_string_append(modifiers, "super");
  }

  if (modifiers->len == 0) {
    g_string_free(modifiers, TRUE);
    return NULL;
  }

  return g_string_free(modifiers, FALSE);
}

char *whelk_chord_format(const WhelkChord *chord, WhelkChordStyle style) {
  if (!chord || !chord->key)
    return NULL;

  g_autofree char *key = display_key(chord->key, style);
  GString *text = g_string_new(NULL);

  if (style == WHELK_CHORD_EMACS) {
    if (chord->ctrl)
      g_string_append(text, "C-");
    if (chord->alt)
      g_string_append(text, "M-");
    if (chord->shift)
      g_string_append(text, "S-");
    if (chord->super)
      g_string_append(text, "s-");

    g_string_append(text, key);

    return g_string_free(text, FALSE);
  }

  if (chord->ctrl)
    g_string_append(text, "Ctrl+");
  if (chord->alt)
    g_string_append(text, "Alt+");
  if (chord->shift)
    g_string_append(text, "Shift+");
  if (chord->super)
    g_string_append(text, "Super+");

  g_string_append(text, key);

  return g_string_free(text, FALSE);
}

char *whelk_chord_display(const char *text, WhelkChordStyle style) {
  return whelk_chord_sequence_display(text, style);
}

char *whelk_chord_command(const char *tool, const WhelkChord *chord) {
  if (!tool || *tool == '\0' || !chord || !chord->key)
    return NULL;

  g_autofree char *modifiers = whelk_chord_modifiers(chord);

  /* wtype presses and releases; xdotool is told the whole chord at once. */
  if (g_str_has_suffix(tool, "xdotool")) {
    GString *typed = g_string_new(NULL);

    if (modifiers) {
      for (const char *p = modifiers; *p; p++)
        g_string_append_c(typed, *p == ' ' ? '+' : *p);
      g_string_append_c(typed, '+');
    }

    g_string_append(typed, chord->key);

    char *command = g_strdup_printf("%s key %s", tool, typed->str);

    g_string_free(typed, TRUE);

    return command;
  }

  if (!modifiers)
    return g_strdup_printf("%s -k %s", tool, chord->key);

  return g_strdup_printf("%s -M %s -k %s -m %s", tool, modifiers, chord->key,
                         modifiers);
}

WhelkChordStyle whelk_chord_style_from_name(const char *name) {
  if (name && g_ascii_strcasecmp(name, "emacs") == 0)
    return WHELK_CHORD_EMACS;

  return WHELK_CHORD_CUA;
}

const char *whelk_chord_style_name(WhelkChordStyle style) {
  return style == WHELK_CHORD_EMACS ? "emacs" : "cua";
}
