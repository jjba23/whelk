// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "launcher.h"

#include <gio/gdesktopappinfo.h>
#include <string.h>

#include "logger.h"
#include "style.h"
#include "surface.h"
#include "util/calc.h"
#include "util/command.h"
#include "util/filesearch.h"
#include "util/format.h"
#include "util/fuzzy.h"
#include "util/json.h"

/* =========================================================================
 * THE LAUNCHER
 * =========================================================================
 * A launcher is four things: a database of things that can be opened, a way to
 * rank them against what was typed, a list that shows the ranking, and one
 * function that opens the chosen thing.  Everything below is one of those.
 */

#define WHELK_LAUNCHER_WIDTH 660
#define WHELK_LAUNCHER_LIST_HEIGHT 420
#define WHELK_LAUNCHER_RESULT_LIMIT 40
#define WHELK_LAUNCHER_DEBOUNCE_MS 120
#define WHELK_LAUNCHER_FILE_DEBOUNCE_MS 260

/* Walking the home directory takes about a second; ranking a walk does not.
 * Each query is served from the last walk until it gets this old, so typing a
 * name costs one walk instead of one per keystroke. */
#define WHELK_LAUNCHER_WALK_MAX_AGE_US (30 * G_TIME_SPAN_SECOND)

#define WHELK_WEB_SEARCH_DEFAULT "https://duckduckgo.com/?q="

typedef enum {
  ITEM_APP,
  ITEM_WINDOW,
  ITEM_COMMAND,
  ITEM_FILE,
  ITEM_WEB,
  ITEM_CALC,
  ITEM_MENU,
} ItemKind;

typedef struct {
  ItemKind kind;
  char *name;
  char *detail;
  char *icon;
  char *search; /* lowercase haystack */
  char *action; /* a desktop file, a window id, a command, a path, a URL */
} LauncherItem;

typedef struct {
  int score;
  LauncherItem *item;
} Ranked;

struct _WhelkLauncher {
  GObject parent_instance;

  GtkApplication *app;
  WhelkBarConfig *style;
  GtkCssProvider *skin;
  WhelkSurface *surface;
  GtkWidget *card;
  GtkWidget *entry;
  GtkWidget *list;
  GtkWidget *mode_label;
  GtkWidget *hint;

  GPtrArray *apps;
  GPtrArray *commands;
  GPtrArray *windows;
  GPtrArray *files;
  GPtrArray *menu;
  GPtrArray *results; /* borrowed from the arrays above */

  char *mode;
  char *menu_command;
  char *last_file_query;

  gboolean apps_loaded;
  gboolean commands_loaded;
  guint windows_generation;
  guint files_generation;
  char *files_walk; /* the last home-directory walk, before ranking */
  gint64 files_walk_at;
  guint query_timeout;
};

G_DEFINE_TYPE(WhelkLauncher, whelk_launcher, G_TYPE_OBJECT)

static void refresh_results(WhelkLauncher *self);

/* =========================================================================
 * ITEMS
 * ========================================================================= */

static LauncherItem *item_new(ItemKind kind, const char *name,
                              const char *detail) {
  LauncherItem *item = g_new0(LauncherItem, 1);

  item->kind = kind;
  item->name = g_strdup(name ? name : "");
  item->detail = g_strdup(detail ? detail : "");
  item->search = g_ascii_strdown(item->name, -1);

  return item;
}

static void item_free(gpointer data) {
  LauncherItem *item = data;

  if (!item)
    return;

  g_free(item->name);
  g_free(item->detail);
  g_free(item->icon);
  g_free(item->search);
  g_free(item->action);
  g_free(item);
}

/* Appends @text to the item's searchable text: a launcher that only matched the
 * name would miss "browser" for Firefox. */
static void item_add_search(LauncherItem *item, const char *text) {
  if (!text || *text == '\0')
    return;

  g_autofree char *lower = g_ascii_strdown(text, -1);
  char *joined = g_strdup_printf("%s %s", item->search, lower);

  g_free(item->search);
  item->search = joined;
}

/* =========================================================================
 * THE APPLICATION DATABASE
 * =========================================================================
 * Every applications/ directory on the system, once, at first use.  A .desktop
 * file is the desktop's own index of what can be started, so reading it is
 * reading the machine instead of guessing at its programs.
 */

static void load_app_from_file(WhelkLauncher *self, const char *directory,
                               const char *filename) {
  g_autofree char *path = g_build_filename(directory, filename, NULL);
  g_autoptr(GKeyFile) keyfile = g_key_file_new();

  if (!g_key_file_load_from_file(keyfile, path, G_KEY_FILE_NONE, NULL))
    return;

  if (g_key_file_get_boolean(keyfile, "Desktop Entry", "NoDisplay", NULL) ||
      g_key_file_get_boolean(keyfile, "Desktop Entry", "Hidden", NULL))
    return;

  g_autofree char *name = g_key_file_get_string(keyfile, "Desktop Entry", "Name", NULL);

  if (!name || *name == '\0')
    return;

  /* Only applications: a link's .desktop starts a browser, which is fine, but a
   * Directory entry is not something to launch. */
  g_autofree char *type =
      g_key_file_get_string(keyfile, "Desktop Entry", "Type", NULL);

  if (g_strcmp0(type, "Application") != 0)
    return;

  g_autofree char *generic =
      g_key_file_get_string(keyfile, "Desktop Entry", "GenericName", NULL);
  g_autofree char *comment =
      g_key_file_get_string(keyfile, "Desktop Entry", "Comment", NULL);
  g_autofree char *keywords =
      g_key_file_get_string(keyfile, "Desktop Entry", "Keywords", NULL);
  g_autofree char *icon =
      g_key_file_get_string(keyfile, "Desktop Entry", "Icon", NULL);
  g_autofree char *exec =
      g_key_file_get_string(keyfile, "Desktop Entry", "Exec", NULL);

  LauncherItem *item = item_new(ITEM_APP, name,
                                generic && *generic ? generic : (exec ? exec : ""));

  item->icon = g_strdup(icon);
  item->action = g_strdup(path);

  item_add_search(item, generic);
  item_add_search(item, comment);
  item_add_search(item, keywords);
  item_add_search(item, filename);

  g_ptr_array_add(self->apps, item);
}

static gboolean has_extension(const char *name, const char *extension) {
  return g_str_has_suffix(name, extension);
}

static void scan_applications(WhelkLauncher *self, const char *directory) {
  GError *error = NULL;
  GDir *dir = g_dir_open(directory, 0, &error);

  if (!dir) {
    g_clear_error(&error);
    return;
  }

  /* One entry per desktop id, the first directory that has it winning: the
   * user's applications/ shadows the system's, which is what XDG says. */
  const char *name = NULL;

  while ((name = g_dir_read_name(dir)) != NULL) {
    if (!has_extension(name, ".desktop"))
      continue;

    gboolean duplicate = FALSE;

    for (guint i = 0; i < self->apps->len; i++) {
      LauncherItem *item = g_ptr_array_index(self->apps, i);
      const char *existing = strrchr(item->action, '/');

      if (existing && g_strcmp0(existing + 1, name) == 0) {
        duplicate = TRUE;
        break;
      }
    }

    if (!duplicate)
      load_app_from_file(self, directory, name);
  }

  g_dir_close(dir);
}

static void load_apps(WhelkLauncher *self) {
  if (self->apps_loaded)
    return;

  self->apps_loaded = TRUE;

  const char *user = g_get_user_data_dir();

  if (user) {
    g_autofree char *directory = g_build_filename(user, "applications", NULL);
    scan_applications(self, directory);
  }

  const char *const *system = g_get_system_data_dirs();

  for (guint i = 0; system && system[i]; i++) {
    g_autofree char *directory =
        g_build_filename(system[i], "applications", NULL);

    scan_applications(self, directory);
  }

  LOG_C_DEBUG("Launcher", "%u application(s) indexed", self->apps->len);
}

/* Every executable on PATH, once, for the run mode's completions. */
static void load_commands(WhelkLauncher *self) {
  if (self->commands_loaded)
    return;

  self->commands_loaded = TRUE;

  const char *path = g_getenv("PATH");

  if (!path)
    return;

  g_auto(GStrv) directories = g_strsplit(path, ":", -1);
  GHashTable *seen = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, NULL);

  for (guint i = 0; directories[i]; i++) {
    GError *error = NULL;
    GDir *dir = g_dir_open(directories[i], 0, &error);

    if (!dir) {
      g_clear_error(&error);
      continue;
    }

    const char *name = NULL;

    while ((name = g_dir_read_name(dir)) != NULL) {
      if (g_hash_table_contains(seen, name))
        continue;

      g_autofree char *full = g_build_filename(directories[i], name, NULL);

      if (!g_file_test(full, G_FILE_TEST_IS_EXECUTABLE) ||
          g_file_test(full, G_FILE_TEST_IS_DIR))
        continue;

      LauncherItem *item = item_new(ITEM_COMMAND, name, directories[i]);

      item->action = g_strdup(name);
      g_ptr_array_add(self->commands, item);
      g_hash_table_add(seen, g_strdup(name));
    }

    g_dir_close(dir);
  }

  g_hash_table_destroy(seen);

  LOG_C_DEBUG("Launcher", "%u command(s) on PATH", self->commands->len);
}

/* =========================================================================
 * WINDOWS
 * ========================================================================= */

static void on_windows(WhelkLauncher *self, const char *text) {
  g_autoptr(GError) error = NULL;
  WhelkJson *root = whelk_json_parse(text, &error);

  if (!root)
    return;

  g_ptr_array_set_size(self->windows, 0);

  for (guint i = 0; i < whelk_json_array_length(root); i++) {
    WhelkJson *window = whelk_json_array_get(root, i);
    const char *title = whelk_json_object_string(window, "title");
    const char *app_id = whelk_json_object_string(window, "app_id");
    const double id = whelk_json_object_number(window, "id", 0);

    if (id <= 0 || !title || *title == '\0')
      continue;

    LauncherItem *item = item_new(ITEM_WINDOW, title,
                                  (app_id && *app_id) ? app_id : "window");

    item->action = g_strdup_printf("%.0f", id);
    item_add_search(item, app_id);

    g_ptr_array_add(self->windows, item);
  }

  whelk_json_free(root);
}

static void on_windows_command(const char *text, gpointer user_data) {
  WhelkLauncher *self = user_data;

  on_windows(self, text);

  if (g_strcmp0(self->mode, "window") == 0)
    refresh_results(self);
}

static void refresh_windows(WhelkLauncher *self) {
  self->windows_generation++;

  whelk_command_run("niri msg -j windows", on_windows_command, self);
}

/* =========================================================================
 * THE RANKING
 * ========================================================================= */

static gint compare_ranked(gconstpointer a, gconstpointer b) {
  const Ranked *left = a;
  const Ranked *right = b;

  return right->score - left->score;
}

static void rank_into(WhelkLauncher *self, GPtrArray *items,
                      const char *query) {
  GArray *ranked = g_array_new(FALSE, FALSE, sizeof(Ranked));

  for (guint i = 0; i < items->len; i++) {
    LauncherItem *item = g_ptr_array_index(items, i);
    const int score = whelk_fuzzy_score(query, item->search);

    if (score < 0)
      continue;

    Ranked entry = {score, item};

    g_array_append_val(ranked, entry);
  }

  g_array_sort(ranked, compare_ranked);

  for (guint i = 0; i < ranked->len && i < WHELK_LAUNCHER_RESULT_LIMIT; i++)
    g_ptr_array_add(self->results, g_array_index(ranked, Ranked, i).item);

  g_array_unref(ranked);
}

static void add_result(WhelkLauncher *self, LauncherItem *item) {
  g_ptr_array_add(self->results, item);
}

/* =========================================================================
 * FILES
 * ========================================================================= */

static void on_files(WhelkLauncher *self, guint generation, const char *query,
                     const char *text) {
  if (generation != self->files_generation)
    return; /* a newer query has already replaced this one */

  g_ptr_array_set_size(self->files, 0);

  if (!query || *query == '\0' || !text)
    return;

  /* The walk hands back every path it met; the ranking decides which ones the
   * query actually meant. */
  g_autoptr(GPtrArray) paths = whelk_file_search_rank(
      query, text, WHELK_LAUNCHER_RESULT_LIMIT);

  for (guint i = 0; i < paths->len; i++) {
    const char *path = g_ptr_array_index(paths, i);
    const char *base = strrchr(path, '/');
    const gboolean is_directory = g_file_test(path, G_FILE_TEST_IS_DIR);
    g_autofree char *name = g_strdup_printf(
        "%s%s", base ? base + 1 : path, is_directory ? "/" : "");

    LauncherItem *item = item_new(ITEM_FILE, name, path);

    item->action = g_strdup(path);
    g_ptr_array_add(self->files, item);
  }
}

typedef struct {
  WhelkLauncher *self;
  guint generation;
  char *query;
} FileRequest;

static void on_files_command(const char *text, gpointer user_data) {
  FileRequest *request = user_data;

  on_files(request->self, request->generation, request->query, text);

  if (text) {
    g_free(request->self->files_walk);
    request->self->files_walk = g_strdup(text);
    request->self->files_walk_at = g_get_monotonic_time();
  }

  if (g_strcmp0(request->self->mode, "file") == 0 &&
      request->generation == request->self->files_generation)
    refresh_results(request->self);

  g_free(request->query);
  g_free(request);
}

static void search_files(WhelkLauncher *self, const char *query) {
  self->files_generation++;

  const guint generation = self->files_generation;

  g_ptr_array_set_size(self->files, 0);

  if (!query || *query == '\0')
    return;

  if (self->files_walk &&
      g_get_monotonic_time() - self->files_walk_at <
          WHELK_LAUNCHER_WALK_MAX_AGE_US) {
    /* Ranking a walk we already have: the caller refreshes the list from
     * self->files when this returns. */
    on_files(self, generation, query, self->files_walk);
    return;
  }

  /* Every file and directory under the home, minus the trees nobody means to
   * search.  The query is not part of the walk: it is ranked afterwards, so a
   * file buried deep is still found, and find order never decides the order of
   * the results. */
  g_autofree char *command = whelk_file_search_command(g_get_home_dir(), 6);

  FileRequest *request = g_new0(FileRequest, 1);

  request->self = self;
  request->generation = generation;
  request->query = g_strdup(query);

  whelk_command_run(command, on_files_command, request);
}

/* =========================================================================
 * THE MENU MODE
 * =========================================================================
 * "label<TAB>command" lines: the format a generated rofi menu already prints,
 * so an existing menu script becomes a whelk menu by dropping the rofi call.
 */

static void on_menu(WhelkLauncher *self, const char *text) {
  g_ptr_array_set_size(self->menu, 0);

  if (!text)
    return;

  g_auto(GStrv) lines = g_strsplit(text, "\n", -1);

  for (guint i = 0; lines[i]; i++) {
    if (*lines[i] == '\0')
      continue;

    g_auto(GStrv) parts = g_strsplit(lines[i], "\t", 2);

    LauncherItem *item =
        item_new(ITEM_MENU, parts[0], parts[1] ? parts[1] : "");

    item->action = g_strdup(parts[1] ? parts[1] : parts[0]);
    g_ptr_array_add(self->menu, item);
  }
}

static void on_menu_command(const char *text, gpointer user_data) {
  WhelkLauncher *self = user_data;

  on_menu(self, text);
  whelk_launcher_show(self, "menu");
}

/* =========================================================================
 * QUERYING
 * ========================================================================= */

static const char *effective_mode(WhelkLauncher *self, const char **query) {
  const char *text = gtk_editable_get_text(GTK_EDITABLE(self->entry));

  if (g_strcmp0(self->mode, "menu") == 0) {
    *query = text;
    return "menu";
  }

  /* A prefix is a mode you can type without knowing the modes exist. */
  switch (text[0]) {
  case '>':
    *query = text + 1;
    return "run";
  case '=':
    *query = text + 1;
    return "calc";
  case '/':
    *query = text + 1;
    return "file";
  case '?':
    *query = text + 1;
    return "web";
  case '@':
    *query = text + 1;
    return "window";
  default:
    break;
  }

  *query = text;
  return self->mode && *self->mode ? self->mode : "app";
}

static void clear_list(WhelkLauncher *self);

static void add_row(WhelkLauncher *self, LauncherItem *item) {
  GtkWidget *row = gtk_list_box_row_new();
  GtkWidget *box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 12);
  GtkWidget *text = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
  GtkWidget *name = gtk_label_new(NULL);
  GtkWidget *detail = gtk_label_new(NULL);

  gtk_widget_add_css_class(box, "whelk-launcher-row");
  gtk_label_set_xalign(GTK_LABEL(name), 0.0);
  gtk_label_set_xalign(GTK_LABEL(detail), 0.0);
  gtk_label_set_ellipsize(GTK_LABEL(name), PANGO_ELLIPSIZE_END);
  gtk_label_set_ellipsize(GTK_LABEL(detail), PANGO_ELLIPSIZE_MIDDLE);
  gtk_widget_add_css_class(name, "whelk-launcher-name");
  gtk_widget_add_css_class(detail, "whelk-launcher-detail");

  g_autofree char *escaped_name = g_markup_escape_text(item->name, -1);
  g_autofree char *escaped_detail = g_markup_escape_text(item->detail, -1);

  gtk_label_set_markup(GTK_LABEL(name), escaped_name);
  gtk_label_set_markup(GTK_LABEL(detail), escaped_detail);
  gtk_box_append(GTK_BOX(text), name);
  gtk_box_append(GTK_BOX(text), detail);

  if (item->icon && *item->icon) {
    GtkWidget *icon = g_path_is_absolute(item->icon)
                          ? gtk_image_new_from_file(item->icon)
                          : gtk_image_new_from_icon_name(item->icon);

    gtk_image_set_pixel_size(GTK_IMAGE(icon), 24);
    gtk_widget_set_valign(icon, GTK_ALIGN_CENTER);
    gtk_box_append(GTK_BOX(box), icon);
  }

  gtk_widget_set_hexpand(text, TRUE);
  gtk_box_append(GTK_BOX(box), text);
  gtk_list_box_row_set_child(GTK_LIST_BOX_ROW(row), box);
  g_object_set_data(G_OBJECT(row), "whelk-item", item);
  g_object_set_data(G_OBJECT(row), "whelk-owned", GINT_TO_POINTER(0));

  gtk_list_box_append(GTK_LIST_BOX(self->list), row);
}

static void clear_list(WhelkLauncher *self) {
  GtkWidget *row = NULL;

  while ((row = gtk_widget_get_first_child(self->list)) != NULL)
    gtk_list_box_remove(GTK_LIST_BOX(self->list), row);
}

static void update_mode_label(WhelkLauncher *self, const char *mode,
                              guint count) {
  static const char *const names[] = {"Applications", "Windows",
                                      "Run a command", "Calculator",
                                      "Files", "Web search", "Menu"};
  const char *label = names[0];

  if (g_strcmp0(mode, "window") == 0)
    label = names[1];
  else if (g_strcmp0(mode, "run") == 0)
    label = names[2];
  else if (g_strcmp0(mode, "calc") == 0)
    label = names[3];
  else if (g_strcmp0(mode, "file") == 0)
    label = names[4];
  else if (g_strcmp0(mode, "web") == 0)
    label = names[5];
  else if (g_strcmp0(mode, "menu") == 0)
    label = names[6];

  g_autofree char *text = g_strdup_printf("%s — %u", label, count);

  gtk_label_set_text(GTK_LABEL(self->mode_label), text);
}

static LauncherItem *selected_item(WhelkLauncher *self) {
  GtkListBoxRow *row = gtk_list_box_get_selected_row(GTK_LIST_BOX(self->list));

  if (!row)
    return NULL;

  return g_object_get_data(G_OBJECT(row), "whelk-item");
}

static gboolean wants_files(WhelkLauncher *self, const char *query) {
  /* Only a changed query costs a find; moving the selection does not. */
  if (g_strcmp0(self->last_file_query, query) == 0)
    return FALSE;

  g_free(self->last_file_query);
  self->last_file_query = g_strdup(query);

  return TRUE;
}

static void refresh_results(WhelkLauncher *self) {
  const char *query = NULL;
  const char *mode = effective_mode(self, &query);

  g_ptr_array_set_size(self->results, 0);

  if (g_strcmp0(mode, "run") == 0) {
    load_commands(self);

    if (query && *query) {
      LauncherItem *run = item_new(ITEM_COMMAND, query, "Run this command");

      run->action = g_strdup(query);
      add_result(self, run);
    }

    rank_into(self, self->commands, query ? query : "");
  } else if (g_strcmp0(mode, "calc") == 0) {
    double value = 0.0;

    if (query && *query && whelk_calc_eval(query, &value)) {
      g_autofree char *formatted = whelk_calc_format(value);
      g_autofree char *name = g_strdup_printf("= %s", formatted);
      LauncherItem *item = item_new(ITEM_CALC, name, "Enter copies the result");

      item->action = g_strdup(formatted);
      add_result(self, item);
    }
  } else if (g_strcmp0(mode, "web") == 0) {
    if (query && *query) {
      g_autofree char *name = g_strdup_printf("Search the web for “%s”", query);
      LauncherItem *item = item_new(ITEM_WEB, name, "Opens the default browser");

      item->action = g_strdup(query);
      add_result(self, item);
    }
  } else if (g_strcmp0(mode, "window") == 0) {
    rank_into(self, self->windows, query ? query : "");
  } else if (g_strcmp0(mode, "file") == 0) {
    if (query && *query && wants_files(self, query))
      search_files(self, query);

    /* The file search has already ranked what it found, by the whole path and
     * the name; ranking the basenames again here would undo that and drop the
     * files that matched further up their path. */
    for (guint i = 0; i < self->files->len; i++)
      add_result(self, g_ptr_array_index(self->files, i));
  } else if (g_strcmp0(mode, "menu") == 0) {
    rank_into(self, self->menu, query ? query : "");
  } else {
    load_apps(self);
    rank_into(self, self->apps, query ? query : "");
  }

  clear_list(self);

  for (guint i = 0; i < self->results->len; i++)
    add_row(self, g_ptr_array_index(self->results, i));

  update_mode_label(self, mode, self->results->len);

  GtkListBoxRow *first = gtk_list_box_get_row_at_index(GTK_LIST_BOX(self->list), 0);

  if (first)
    gtk_list_box_select_row(GTK_LIST_BOX(self->list), first);
}

static gboolean on_query_timeout(gpointer user_data) {
  WhelkLauncher *self = user_data;

  self->query_timeout = 0;
  refresh_results(self);

  return G_SOURCE_REMOVE;
}

static void schedule_refresh(WhelkLauncher *self, guint delay) {
  if (self->query_timeout != 0)
    g_source_remove(self->query_timeout);

  self->query_timeout = g_timeout_add(delay, on_query_timeout, self);
}

static void on_entry_changed(GtkEditable *editable, gpointer user_data) {
  (void)editable;

  WhelkLauncher *self = user_data;
  const char *text = gtk_editable_get_text(GTK_EDITABLE(self->entry));

  /* A file query is a find(1) run; it waits a little longer than a filter,
   * which is the difference between typing and waiting. */
  schedule_refresh(self, text[0] == '/' ||
                             g_strcmp0(self->mode, "file") == 0
                         ? WHELK_LAUNCHER_FILE_DEBOUNCE_MS
                         : WHELK_LAUNCHER_DEBOUNCE_MS);
}

/* =========================================================================
 * ACTIVATION
 * ========================================================================= */

static void copy_to_clipboard(const char *text) {
  g_autofree char *quoted = g_shell_quote(text);
  g_autofree char *command =
      g_strdup_printf("printf '%%s' %s | wl-copy", quoted);

  whelk_command_spawn(command);
}

static void open_uri(const char *uri) {
  g_autofree char *quoted = g_shell_quote(uri);
  g_autofree char *command = g_strdup_printf("xdg-open %s", quoted);

  whelk_command_spawn(command);
}

static void launch_app(const char *desktop_file) {
  g_autoptr(GDesktopAppInfo) info =
      g_desktop_app_info_new_from_filename(desktop_file);

  if (!info) {
    LOG_C_WARN("Launcher", "'%s' is not a launchable desktop entry",
               desktop_file);
    return;
  }

  g_autoptr(GError) error = NULL;

  if (!g_app_info_launch(G_APP_INFO(info), NULL, NULL, &error))
    LOG_C_WARN("Launcher", "'%s' did not start: %s", desktop_file,
               error->message);
}

static void activate_item(WhelkLauncher *self, LauncherItem *item) {
  if (!item)
    return;

  switch (item->kind) {
  case ITEM_APP:
    launch_app(item->action);
    break;
  case ITEM_WINDOW: {
    g_autofree char *command =
        g_strdup_printf("niri msg action focus-window --id %s", item->action);

    whelk_command_spawn(command);
    break;
  }
  case ITEM_COMMAND:
  case ITEM_MENU:
    whelk_command_spawn(item->action);
    break;
  case ITEM_FILE:
    open_uri(item->action);
    break;
  case ITEM_WEB: {
    const char *base = g_getenv("WHELK_WEB_SEARCH");

    if (!base || *base == '\0')
      base = WHELK_WEB_SEARCH_DEFAULT;

    g_autofree char *escaped = g_uri_escape_string(item->action, NULL, TRUE);
    g_autofree char *url = g_strdup_printf("%s%s", base, escaped);

    open_uri(url);
    break;
  }
  case ITEM_CALC:
    copy_to_clipboard(item->action);
    break;
  }

  whelk_launcher_hide(self);
}

static void on_entry_activate(GtkEntry *entry, gpointer user_data) {
  (void)entry;

  WhelkLauncher *self = user_data;

  activate_item(self, selected_item(self));
}

static void on_row_activated(GtkListBox *list, GtkListBoxRow *row,
                             gpointer user_data) {
  (void)list;

  WhelkLauncher *self = user_data;
  LauncherItem *item = g_object_get_data(G_OBJECT(row), "whelk-item");

  if (item)
    activate_item(self, item);
}

/* =========================================================================
 * KEYBOARD
 * ========================================================================= */

static void move_selection(WhelkLauncher *self, int delta) {
  GtkListBoxRow *current = gtk_list_box_get_selected_row(GTK_LIST_BOX(self->list));
  int index = current ? gtk_list_box_row_get_index(current) : -1;
  const int count = (int)self->results->len;

  index += delta;

  if (index < 0)
    index = count - 1;
  else if (index >= count)
    index = 0;

  GtkListBoxRow *row = gtk_list_box_get_row_at_index(GTK_LIST_BOX(self->list), index);

  if (row)
    gtk_list_box_select_row(GTK_LIST_BOX(self->list), row);
}

static void complete_selection(WhelkLauncher *self) {
  LauncherItem *item = selected_item(self);

  if (!item)
    return;

  gtk_editable_set_text(GTK_EDITABLE(self->entry), item->name);
  gtk_editable_set_position(GTK_EDITABLE(self->entry), -1);
}

static gboolean on_key_pressed(GtkEventControllerKey *controller,
                               guint keyval, guint keycode,
                               GdkModifierType state, gpointer user_data) {
  (void)controller;
  (void)keycode;
  (void)state;

  WhelkLauncher *self = user_data;

  switch (keyval) {
  case GDK_KEY_Escape:
    whelk_launcher_hide(self);
    return TRUE;
  case GDK_KEY_Down:
    move_selection(self, 1);
    return TRUE;
  case GDK_KEY_Up:
    move_selection(self, -1);
    return TRUE;
  case GDK_KEY_Tab:
    complete_selection(self);
    return TRUE;
  case GDK_KEY_Return:
  case GDK_KEY_KP_Enter:
    activate_item(self, selected_item(self));
    return TRUE;
  default:
    return FALSE;
  }
}

/* =========================================================================
 * PUBLIC API
 * ========================================================================= */

WhelkLauncher *whelk_launcher_new(GtkApplication *app,
                                  const WhelkBarConfig *style) {
  WhelkLauncher *self = g_object_new(WHELK_TYPE_LAUNCHER, NULL);

  self->app = app;
  self->style =
      style ? whelk_bar_config_copy(style) : whelk_bar_config_new("launcher");

  self->apps = g_ptr_array_new_with_free_func(item_free);
  self->commands = g_ptr_array_new_with_free_func(item_free);
  self->windows = g_ptr_array_new_with_free_func(item_free);
  self->files = g_ptr_array_new_with_free_func(item_free);
  self->menu = g_ptr_array_new_with_free_func(item_free);
  self->results = g_ptr_array_new();

  self->card = gtk_box_new(GTK_ORIENTATION_VERTICAL, 8);
  self->entry = gtk_entry_new();

  gtk_widget_add_css_class(self->card, "whelk-launcher");
  gtk_widget_add_css_class(self->entry, "whelk-launcher-entry");
  gtk_entry_set_placeholder_text(GTK_ENTRY(self->entry),
                                 "Search applications, windows, commands…");
  gtk_widget_set_size_request(self->card, WHELK_LAUNCHER_WIDTH, -1);

  self->mode_label = gtk_label_new("Applications");
  self->hint = gtk_label_new(
      "> run   = calc   / files   ? web   @ windows   ·   Tab completes   ·   "
      "Esc closes");

  gtk_widget_add_css_class(self->mode_label, "whelk-launcher-mode");
  gtk_widget_add_css_class(self->hint, "whelk-launcher-hint");
  gtk_label_set_xalign(GTK_LABEL(self->mode_label), 0.0);
  gtk_label_set_xalign(GTK_LABEL(self->hint), 0.0);
  gtk_label_set_wrap(GTK_LABEL(self->hint), TRUE);

  self->list = gtk_list_box_new();

  gtk_list_box_set_selection_mode(GTK_LIST_BOX(self->list),
                                  GTK_SELECTION_SINGLE);
  gtk_widget_add_css_class(self->list, "whelk-launcher-list");
  gtk_list_box_set_activate_on_single_click(GTK_LIST_BOX(self->list), TRUE);

  GtkWidget *scroller = gtk_scrolled_window_new();

  gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scroller), self->list);
  gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scroller),
                                 GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
  gtk_widget_set_size_request(scroller, -1, WHELK_LAUNCHER_LIST_HEIGHT);

  gtk_box_append(GTK_BOX(self->card), self->entry);
  gtk_box_append(GTK_BOX(self->card), self->mode_label);
  gtk_box_append(GTK_BOX(self->card), scroller);
  gtk_box_append(GTK_BOX(self->card), self->hint);

  GtkWidget *holder = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);

  gtk_widget_set_halign(self->card, GTK_ALIGN_CENTER);
  gtk_widget_set_valign(self->card, GTK_ALIGN_START);
  gtk_widget_set_margin_top(self->card, 120);
  gtk_widget_set_hexpand(self->card, FALSE);
  gtk_box_append(GTK_BOX(holder), self->card);

  self->surface = whelk_surface_new(app, "launcher");

  whelk_surface_set_edge(self->surface, GTK_LAYER_SHELL_EDGE_TOP);
  whelk_surface_set_layer(self->surface, GTK_LAYER_SHELL_LAYER_OVERLAY);
  whelk_surface_set_keyboard_mode(self->surface,
                                  GTK_LAYER_SHELL_KEYBOARD_MODE_EXCLUSIVE);
  whelk_surface_set_exclusive(self->surface, FALSE);
  whelk_surface_set_anchors(self->surface, TRUE, TRUE, TRUE, TRUE);
  whelk_surface_set_namespace(self->surface, "whelk-launcher");

  g_autofree char *widget_id = whelk_style_widget_id("launcher");

  whelk_surface_set_widget_id(self->surface, widget_id);
  self->skin = whelk_style_apply(widget_id, self->style, "whelk-launcher",
                                 NULL);
  whelk_surface_set_child(self->surface, holder);

  g_signal_connect(self->entry, "changed", G_CALLBACK(on_entry_changed), self);
  g_signal_connect(self->entry, "activate", G_CALLBACK(on_entry_activate),
                   self);
  g_signal_connect(self->list, "row-activated", G_CALLBACK(on_row_activated),
                   self);

  GtkEventController *keys = gtk_event_controller_key_new();
  g_signal_connect(keys, "key-pressed", G_CALLBACK(on_key_pressed), self);
  gtk_widget_add_controller(GTK_WIDGET(whelk_surface_get_window(self->surface)),
                            keys);

  whelk_surface_present(self->surface);

  GtkWindow *window = whelk_surface_get_window(self->surface);

  if (window)
    gtk_widget_set_visible(GTK_WIDGET(window), FALSE);

  return self;
}

void whelk_launcher_show(WhelkLauncher *self, const char *mode) {
  if (!self)
    return;

  g_free(self->mode);
  self->mode = g_strdup(mode && *mode ? mode : "app");

  LOG_C_DEBUG("Launcher", "showing mode '%s'", self->mode);

  gtk_editable_set_text(GTK_EDITABLE(self->entry), "");

  if (g_strcmp0(self->mode, "window") == 0)
    refresh_windows(self);

  load_apps(self);
  refresh_results(self);

  GtkWindow *window = whelk_surface_get_window(self->surface);

  if (window) {
    gtk_widget_set_visible(GTK_WIDGET(window), TRUE);
    gtk_window_present(window);
  }

  gtk_widget_grab_focus(self->entry);
}

void whelk_launcher_show_menu(WhelkLauncher *self, const char *command) {
  if (!self || !command || *command == '\0')
    return;

  g_free(self->menu_command);
  self->menu_command = g_strdup(command);

  g_free(self->mode);
  self->mode = g_strdup("menu");

  whelk_command_run(command, on_menu_command, self);
}

void whelk_launcher_hide(WhelkLauncher *self) {
  if (!self)
    return;

  GtkWindow *window = whelk_surface_get_window(self->surface);

  if (window)
    gtk_widget_set_visible(GTK_WIDGET(window), FALSE);
}

gboolean whelk_launcher_is_visible(WhelkLauncher *self) {
  if (!self)
    return FALSE;

  GtkWindow *window = whelk_surface_get_window(self->surface);

  return window && gtk_widget_get_visible(GTK_WIDGET(window));
}

void whelk_launcher_toggle(WhelkLauncher *self, const char *mode) {
  if (!self)
    return;

  if (whelk_launcher_is_visible(self))
    whelk_launcher_hide(self);
  else
    whelk_launcher_show(self, mode);
}

/* =========================================================================
 * LIFECYCLE
 * ========================================================================= */

static void whelk_launcher_dispose(GObject *object) {
  WhelkLauncher *self = WHELK_LAUNCHER(object);

  if (self->query_timeout != 0) {
    g_source_remove(self->query_timeout);
    self->query_timeout = 0;
  }

  g_clear_pointer(&self->results, g_ptr_array_unref);
  g_clear_pointer(&self->apps, g_ptr_array_unref);
  g_clear_pointer(&self->commands, g_ptr_array_unref);
  g_clear_pointer(&self->windows, g_ptr_array_unref);
  g_clear_pointer(&self->files, g_ptr_array_unref);
  g_clear_pointer(&self->menu, g_ptr_array_unref);

  if (self->skin)
    whelk_style_detach(self->skin, gdk_display_get_default());

  g_clear_object(&self->surface);
  g_clear_pointer(&self->style, whelk_bar_config_free);
  g_free(self->mode);
  g_free(self->menu_command);
  g_free(self->last_file_query);
  g_free(self->files_walk);

  G_OBJECT_CLASS(whelk_launcher_parent_class)->dispose(object);
}

static void whelk_launcher_class_init(WhelkLauncherClass *klass) {
  GObjectClass *object_class = G_OBJECT_CLASS(klass);

  object_class->dispose = whelk_launcher_dispose;
}

static void whelk_launcher_init(WhelkLauncher *self) { (void)self; }
