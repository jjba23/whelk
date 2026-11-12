// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/menudict.h"

#include <string.h>

/* =========================================================================
 * THE TABLE
 * =========================================================================
 * One block per application: the ids it answers to, then its menus.  The
 * chords are the ones each project documents; where a project offers two
 * bindings the one printed in its own menus is used.
 *
 * The table is deliberately small and correct rather than large and hopeful:
 * an entry that types the wrong chord is worse than no entry at all, so an
 * application only appears here when its shortcuts are known.
 */

/* --- Firefox -------------------------------------------------------------- */

static const WhelkDictItem firefox_file[] = {
    {"New Tab", "Ctrl+T", "ctrl", "t"},
    {"New Window", "Ctrl+N", "ctrl", "n"},
    {"New Private Window", "Ctrl+Shift+P", "ctrl shift", "p"},
    {"Open Location", "Ctrl+L", "ctrl", "l"},
    {"Open File", "Ctrl+O", "ctrl", "o"},
    {"Save Page As", "Ctrl+S", "ctrl", "s"},
    {"Print", "Ctrl+P", "ctrl", "p"},
    {"Quit", "Ctrl+Q", "ctrl", "q"},
};

static const WhelkDictItem firefox_edit[] = {
    {"Undo", "Ctrl+Z", "ctrl", "z"},
    {"Redo", "Ctrl+Shift+Z", "ctrl shift", "z"},
    {"Cut", "Ctrl+X", "ctrl", "x"},
    {"Copy", "Ctrl+C", "ctrl", "c"},
    {"Paste", "Ctrl+V", "ctrl", "v"},
    {"Select All", "Ctrl+A", "ctrl", "a"},
    {"Find in Page", "Ctrl+F", "ctrl", "f"},
};

static const WhelkDictItem firefox_view[] = {
    {"Reload", "F5", NULL, "F5"},
    {"Zoom In", "Ctrl++", "ctrl", "plus"},
    {"Zoom Out", "Ctrl+-", "ctrl", "minus"},
    {"Reset Zoom", "Ctrl+0", "ctrl", "0"},
    {"Full Screen", "F11", NULL, "F11"},
    {"Bookmarks", "Ctrl+Shift+O", "ctrl shift", "o"},
    {"History", "Ctrl+H", "ctrl", "h"},
};

static const char *const firefox_ids[] = {"firefox", "firefox-esr",
                                          "firefox-developer-edition",
                                          "org.mozilla.firefox", NULL};

static const WhelkDictMenu firefox_menus[] = {
    {"File", firefox_file, G_N_ELEMENTS(firefox_file)},
    {"Edit", firefox_edit, G_N_ELEMENTS(firefox_edit)},
    {"View", firefox_view, G_N_ELEMENTS(firefox_view)},
};

/* --- Chromium, Chrome, Brave, Edge --------------------------------------- */

static const WhelkDictItem chromium_file[] = {
    {"New Tab", "Ctrl+T", "ctrl", "t"},
    {"New Window", "Ctrl+N", "ctrl", "n"},
    {"New Incognito Window", "Ctrl+Shift+N", "ctrl shift", "n"},
    {"Open Location", "Ctrl+L", "ctrl", "l"},
    {"Save Page As", "Ctrl+S", "ctrl", "s"},
    {"Print", "Ctrl+P", "ctrl", "p"},
    {"Close Tab", "Ctrl+W", "ctrl", "w"},
    {"Quit", "Ctrl+Shift+Q", "ctrl shift", "q"},
};

static const WhelkDictItem chromium_edit[] = {
    {"Undo", "Ctrl+Z", "ctrl", "z"},
    {"Redo", "Ctrl+Shift+Z", "ctrl shift", "z"},
    {"Cut", "Ctrl+X", "ctrl", "x"},
    {"Copy", "Ctrl+C", "ctrl", "c"},
    {"Paste", "Ctrl+V", "ctrl", "v"},
    {"Select All", "Ctrl+A", "ctrl", "a"},
    {"Find", "Ctrl+F", "ctrl", "f"},
};

static const WhelkDictItem chromium_view[] = {
    {"Reload", "Ctrl+R", "ctrl", "r"},
    {"Zoom In", "Ctrl++", "ctrl", "plus"},
    {"Zoom Out", "Ctrl+-", "ctrl", "minus"},
    {"Reset Zoom", "Ctrl+0", "ctrl", "0"},
    {"Full Screen", "F11", NULL, "F11"},
    {"Developer Tools", "Ctrl+Shift+I", "ctrl shift", "i"},
};

static const char *const chromium_ids[] = {"chromium", "chromium-browser",
                                           "google-chrome", "chrome",
                                           "brave-browser", "brave",
                                           "microsoft-edge", "vivaldi",
                                           "org.chromium.Chromium", NULL};

static const WhelkDictMenu chromium_menus[] = {
    {"File", chromium_file, G_N_ELEMENTS(chromium_file)},
    {"Edit", chromium_edit, G_N_ELEMENTS(chromium_edit)},
    {"View", chromium_view, G_N_ELEMENTS(chromium_view)},
};

/* --- VS Code and its forks ----------------------------------------------- */

static const WhelkDictItem code_file[] = {
    {"New File", "Ctrl+N", "ctrl", "n"},
    {"Open File", "Ctrl+O", "ctrl", "o"},
    {"Save", "Ctrl+S", "ctrl", "s"},
    {"Save As", "Ctrl+Shift+S", "ctrl shift", "s"},
};

static const WhelkDictItem code_edit[] = {
    {"Undo", "Ctrl+Z", "ctrl", "z"},
    {"Redo", "Ctrl+Shift+Z", "ctrl shift", "z"},
    {"Cut", "Ctrl+X", "ctrl", "x"},
    {"Copy", "Ctrl+C", "ctrl", "c"},
    {"Paste", "Ctrl+V", "ctrl", "v"},
    {"Find", "Ctrl+F", "ctrl", "f"},
    {"Replace", "Ctrl+H", "ctrl", "h"},
};

static const WhelkDictItem code_view[] = {
    {"Command Palette", "Ctrl+Shift+P", "ctrl shift", "p"},
    {"Toggle Sidebar", "Ctrl+B", "ctrl", "b"},
    {"Toggle Terminal", "Ctrl+`", "ctrl", "grave"},
    {"Toggle Full Screen", "F11", NULL, "F11"},
};

static const char *const code_ids[] = {"code", "code-oss", "codium",
                                       "vscodium", "com.visualstudio.code",
                                       NULL};

static const WhelkDictMenu code_menus[] = {
    {"File", code_file, G_N_ELEMENTS(code_file)},
    {"Edit", code_edit, G_N_ELEMENTS(code_edit)},
    {"View", code_view, G_N_ELEMENTS(code_view)},
};

/* --- LibreOffice Writer --------------------------------------------------- */

static const WhelkDictItem writer_file[] = {
    {"New", "Ctrl+N", "ctrl", "n"},
    {"Open", "Ctrl+O", "ctrl", "o"},
    {"Save", "Ctrl+S", "ctrl", "s"},
    {"Print", "Ctrl+P", "ctrl", "p"},
};

static const WhelkDictItem writer_edit[] = {
    {"Undo", "Ctrl+Z", "ctrl", "z"},
    {"Redo", "Ctrl+Y", "ctrl", "y"},
    {"Cut", "Ctrl+X", "ctrl", "x"},
    {"Copy", "Ctrl+C", "ctrl", "c"},
    {"Paste", "Ctrl+V", "ctrl", "v"},
    {"Select All", "Ctrl+A", "ctrl", "a"},
    {"Find", "Ctrl+F", "ctrl", "f"},
};

static const WhelkDictItem writer_format[] = {
    {"Bold", "Ctrl+B", "ctrl", "b"},
    {"Italic", "Ctrl+I", "ctrl", "i"},
    {"Underline", "Ctrl+U", "ctrl", "u"},
};

static const char *const writer_ids[] = {"libreoffice-writer",
                                         "org.libreoffice.LibreOffice.writer",
                                         "libreoffice", NULL};

static const WhelkDictMenu writer_menus[] = {
    {"File", writer_file, G_N_ELEMENTS(writer_file)},
    {"Edit", writer_edit, G_N_ELEMENTS(writer_edit)},
    {"Format", writer_format, G_N_ELEMENTS(writer_format)},
};

/* --- GNOME Text Editor and gedit ----------------------------------------- */

static const WhelkDictItem editor_file[] = {
    {"New", "Ctrl+N", "ctrl", "n"},
    {"Open", "Ctrl+O", "ctrl", "o"},
    {"Save", "Ctrl+S", "ctrl", "s"},
};

static const WhelkDictItem editor_edit[] = {
    {"Undo", "Ctrl+Z", "ctrl", "z"},
    {"Redo", "Ctrl+Shift+Z", "ctrl shift", "z"},
    {"Cut", "Ctrl+X", "ctrl", "x"},
    {"Copy", "Ctrl+C", "ctrl", "c"},
    {"Paste", "Ctrl+V", "ctrl", "v"},
    {"Select All", "Ctrl+A", "ctrl", "a"},
    {"Find", "Ctrl+F", "ctrl", "f"},
};

static const WhelkDictItem editor_view[] = {
    {"Zoom In", "Ctrl++", "ctrl", "plus"},
    {"Zoom Out", "Ctrl+-", "ctrl", "minus"},
    {"Full Screen", "F11", NULL, "F11"},
};

static const char *const editor_ids[] = {"org.gnome.TextEditor", "gedit",
                                         "org.gnome.gedit", "gnome-text-editor",
                                         "kate", "org.kde.kate", "mousepad",
                                         "org.xfce.mousepad", NULL};

static const WhelkDictMenu editor_menus[] = {
    {"File", editor_file, G_N_ELEMENTS(editor_file)},
    {"Edit", editor_edit, G_N_ELEMENTS(editor_edit)},
    {"View", editor_view, G_N_ELEMENTS(editor_view)},
};

/* --- Nautilus / Files ----------------------------------------------------- */

static const WhelkDictItem files_file[] = {
    {"New Window", "Ctrl+N", "ctrl", "n"},
    {"New Tab", "Ctrl+T", "ctrl", "t"},
    {"New Folder", "Ctrl+Shift+N", "ctrl shift", "n"},
    {"Close Window", "Ctrl+W", "ctrl", "w"},
};

static const WhelkDictItem files_edit[] = {
    {"Undo", "Ctrl+Z", "ctrl", "z"},
    {"Redo", "Ctrl+Shift+Z", "ctrl shift", "z"},
    {"Cut", "Ctrl+X", "ctrl", "x"},
    {"Copy", "Ctrl+C", "ctrl", "c"},
    {"Paste", "Ctrl+V", "ctrl", "v"},
    {"Select All", "Ctrl+A", "ctrl", "a"},
};

static const WhelkDictItem files_view[] = {
    {"Reload", "Ctrl+R", "ctrl", "r"},
    {"Show Hidden Files", "Ctrl+H", "ctrl", "h"},
    {"Zoom In", "Ctrl++", "ctrl", "plus"},
    {"Zoom Out", "Ctrl+-", "ctrl", "minus"},
    {"Reset Zoom", "Ctrl+0", "ctrl", "0"},
};

static const char *const files_ids[] = {"org.gnome.Nautilus", "nautilus",
                                        "nemo", "caja", "dolphin",
                                        "org.kde.dolphin", "thunar",
                                        "org.xfce.thunar", NULL};

static const WhelkDictMenu files_menus[] = {
    {"File", files_file, G_N_ELEMENTS(files_file)},
    {"Edit", files_edit, G_N_ELEMENTS(files_edit)},
    {"View", files_view, G_N_ELEMENTS(files_view)},
};

/* --- Terminals ------------------------------------------------------------
 * Their chords are the tab-oriented ones, which every terminal emulator
 * shares because the shell inside the terminal already owns plain Ctrl+C. */

static const WhelkDictItem terminal_file[] = {
    {"New Window", "Ctrl+Shift+N", "ctrl shift", "n"},
    {"New Tab", "Ctrl+Shift+T", "ctrl shift", "t"},
    {"Close Window", "Ctrl+Shift+W", "ctrl shift", "w"},
};

static const WhelkDictItem terminal_edit[] = {
    {"Copy", "Ctrl+Shift+C", "ctrl shift", "c"},
    {"Paste", "Ctrl+Shift+V", "ctrl shift", "v"},
    {"Select All", "Ctrl+Shift+A", "ctrl shift", "a"},
};

static const WhelkDictItem terminal_view[] = {
    {"Zoom In", "Ctrl++", "ctrl", "plus"},
    {"Zoom Out", "Ctrl+-", "ctrl", "minus"},
    {"Reset Zoom", "Ctrl+0", "ctrl", "0"},
    {"Full Screen", "F11", NULL, "F11"},
};

static const char *const terminal_ids[] = {"org.gnome.Console",
                                           "org.gnome.Terminal",
                                           "gnome-terminal", "kgx",
                                           "alacritty", "Alacritty", "kitty",
                                           "foot", "org.wezfurlong.wezterm",
                                           "wezterm", "xterm", "urxvt",
                                           "konsole", "org.kde.konsole",
                                           "xfce4-terminal", "terminator",
                                           "tilix", NULL};

static const WhelkDictMenu terminal_menus[] = {
    {"File", terminal_file, G_N_ELEMENTS(terminal_file)},
    {"Edit", terminal_edit, G_N_ELEMENTS(terminal_edit)},
    {"View", terminal_view, G_N_ELEMENTS(terminal_view)},
};

/* --- Thunderbird ---------------------------------------------------------- */

static const WhelkDictItem thunderbird_file[] = {
    {"New Message", "Ctrl+N", "ctrl", "n"},
    {"Open Saved Message", "Ctrl+O", "ctrl", "o"},
    {"Save As", "Ctrl+S", "ctrl", "s"},
    {"Print", "Ctrl+P", "ctrl", "p"},
};

static const WhelkDictItem thunderbird_edit[] = {
    {"Undo", "Ctrl+Z", "ctrl", "z"},
    {"Redo", "Ctrl+Z", "ctrl", "z"},
    {"Cut", "Ctrl+X", "ctrl", "x"},
    {"Copy", "Ctrl+C", "ctrl", "c"},
    {"Paste", "Ctrl+V", "ctrl", "v"},
    {"Select All", "Ctrl+A", "ctrl", "a"},
    {"Find", "Ctrl+F", "ctrl", "f"},
};

static const WhelkDictItem thunderbird_message[] = {
    {"Reply", "Ctrl+R", "ctrl", "r"},
    {"Reply All", "Ctrl+Shift+R", "ctrl shift", "r"},
    {"Forward", "Ctrl+L", "ctrl", "l"},
    {"Archive", "A", NULL, "a"},
};

static const char *const thunderbird_ids[] = {"thunderbird",
                                              "org.mozilla.Thunderbird",
                                              NULL};

static const WhelkDictMenu thunderbird_menus[] = {
    {"File", thunderbird_file, G_N_ELEMENTS(thunderbird_file)},
    {"Edit", thunderbird_edit, G_N_ELEMENTS(thunderbird_edit)},
    {"Message", thunderbird_message, G_N_ELEMENTS(thunderbird_message)},
};

/* --- GIMP ----------------------------------------------------------------- */

static const WhelkDictItem gimp_file[] = {
    {"New Image", "Ctrl+N", "ctrl", "n"},
    {"Open Image", "Ctrl+O", "ctrl", "o"},
    {"Export As", "Ctrl+Shift+E", "ctrl shift", "e"},
    {"Print", "Ctrl+P", "ctrl", "p"},
};

static const WhelkDictItem gimp_edit[] = {
    {"Undo", "Ctrl+Z", "ctrl", "z"},
    {"Redo", "Ctrl+Y", "ctrl", "y"},
    {"Cut", "Ctrl+X", "ctrl", "x"},
    {"Copy", "Ctrl+C", "ctrl", "c"},
    {"Paste", "Ctrl+V", "ctrl", "v"},
    {"Select All", "Ctrl+A", "ctrl", "a"},
};

static const WhelkDictItem gimp_view[] = {
    {"Zoom In", "+", NULL, "plus"},
    {"Zoom Out", "-", NULL, "minus"},
    {"Full Screen", "F11", NULL, "F11"},
};

static const char *const gimp_ids[] = {"gimp", "gimp-2.10", "gimp-3.0",
                                       "org.gimp.GIMP", NULL};

static const WhelkDictMenu gimp_menus[] = {
    {"File", gimp_file, G_N_ELEMENTS(gimp_file)},
    {"Edit", gimp_edit, G_N_ELEMENTS(gimp_edit)},
    {"View", gimp_view, G_N_ELEMENTS(gimp_view)},
};

/* =========================================================================
 * THE TABLE, ASSEMBLED
 * ========================================================================= */

static const WhelkDictApp apps[] = {
    {"Firefox", firefox_ids, firefox_menus, G_N_ELEMENTS(firefox_menus)},
    {"Chromium", chromium_ids, chromium_menus, G_N_ELEMENTS(chromium_menus)},
    {"VS Code", code_ids, code_menus, G_N_ELEMENTS(code_menus)},
    {"LibreOffice Writer", writer_ids, writer_menus,
     G_N_ELEMENTS(writer_menus)},
    {"Text Editor", editor_ids, editor_menus, G_N_ELEMENTS(editor_menus)},
    {"Files", files_ids, files_menus, G_N_ELEMENTS(files_menus)},
    {"Terminal", terminal_ids, terminal_menus, G_N_ELEMENTS(terminal_menus)},
    {"Thunderbird", thunderbird_ids, thunderbird_menus,
     G_N_ELEMENTS(thunderbird_menus)},
    {"GIMP", gimp_ids, gimp_menus, G_N_ELEMENTS(gimp_menus)},
};

/* =========================================================================
 * LOOKUP
 * =========================================================================
 * A compositor reports an application id three ways: niri and sway use the
 * Wayland app id ("org.mozilla.firefox"), X11 gives WM_CLASS ("Firefox"), and
 * a desktop entry is "firefox.desktop".  All three find the same entry: the
 * comparison ignores case, drops a trailing ".desktop", and accepts a dotted
 * suffix in either direction.
 */

static char *normalize(const char *app_id) {
  if (!app_id)
    return NULL;

  char *copy = g_ascii_strdown(app_id, -1);

  if (g_str_has_suffix(copy, ".desktop")) {
    char *shorter = g_strndup(copy, strlen(copy) - strlen(".desktop"));

    g_free(copy);
    copy = shorter;
  }

  return copy;
}

static gboolean ids_match(const char *app, const char *id) {
  if (!app || !*app || !id || !*id)
    return FALSE;

  g_autofree char *normalized = normalize(id);
  const gsize app_length = strlen(app);
  const gsize id_length = strlen(normalized);

  if (g_strcmp0(app, normalized) == 0)
    return TRUE;

  /* "firefox" answers to "org.mozilla.firefox", and the other way round. */
  if (app_length > id_length && g_str_has_suffix(app, normalized) &&
      app[app_length - id_length - 1] == '.')
    return TRUE;

  if (id_length > app_length && g_str_has_suffix(normalized, app) &&
      normalized[id_length - app_length - 1] == '.')
    return TRUE;

  return FALSE;
}

const WhelkDictApp *whelk_menudict_for_app(const char *app_id) {
  g_autofree char *app = normalize(app_id);

  if (!app || *app == '\0')
    return NULL;

  for (guint i = 0; i < G_N_ELEMENTS(apps); i++)
    for (guint j = 0; apps[i].ids[j]; j++)
      if (ids_match(app, apps[i].ids[j]))
        return &apps[i];

  return NULL;
}

guint whelk_menudict_size(void) { return G_N_ELEMENTS(apps); }

const WhelkDictApp *whelk_menudict_at(guint index) {
  return index < G_N_ELEMENTS(apps) ? &apps[index] : NULL;
}
