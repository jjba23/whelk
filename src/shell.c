// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "shell.h"

#include <string.h>

#include "bar.h"
#include "config.h"
#include "guile_bridge.h"
#include "launcher.h"
#include "logger.h"
#include "network_panel.h"
#include "notification_center.h"
#include "notifications.h"
#include "osd.h"
#include "util/menusearch.h"

/* =========================================================================
 * THE SHELL
 * ========================================================================= */

#define CONFIG_RELOAD_DEBOUNCE_MS 250

/* The one shell in the process: modules reach it through
 * whelk_shell_get_default() because a click handler is a plain C callback with
 * nowhere to carry it. */
static WhelkShell *default_shell = NULL;

struct _WhelkShell {
  GtkApplication *app;
  GPtrArray *bars; /* WhelkBar* */
  char *config_path;
  GtkCssProvider *base_css;
  GFileMonitor *config_monitor;
  guint reload_timeout;
  guint monitor_idle;
  gboolean building;

  WhelkBarConfig *style; /* the first bar's palette, for the shell's panels */
  WhelkNotifications *notifications;
  WhelkClipboard *clipboard;
  WhelkNotificationCenter *center;
  WhelkLauncher *launcher;
  WhelkNetworkPanel *network;
  WhelkOsd *osd;
};

static void build_bars(WhelkShell *shell);

/* =========================================================================
 * MONITORS
 * =========================================================================
 * "all" means one bar per output; a connector name means exactly one; "default"
 * (or a name no output answers to) means the compositor's own choice.
 */

static GdkMonitor *find_monitor_by_connector(const char *connector) {
  if (!connector || *connector == '\0')
    return NULL;

  GdkDisplay *display = gdk_display_get_default();

  if (!display)
    return NULL;

  GListModel *monitors = gdk_display_get_monitors(display);
  const guint count = monitors ? g_list_model_get_n_items(monitors) : 0;

  for (guint i = 0; i < count; i++) {
    GdkMonitor *monitor = GDK_MONITOR(g_list_model_get_item(monitors, i));
    const char *name = monitor ? gdk_monitor_get_connector(monitor) : NULL;

    if (name && g_strcmp0(name, connector) == 0)
      return monitor; /* the reference goes to the caller */

    g_clear_object(&monitor);
  }

  return NULL;
}

/* Builds the bars of one configuration on every output it asks for. */
static void build_config(WhelkShell *shell, const WhelkBarConfig *config) {
  GdkDisplay *display = gdk_display_get_default();
  GListModel *monitors = display ? gdk_display_get_monitors(display) : NULL;
  const guint count = monitors ? g_list_model_get_n_items(monitors) : 0;

  if (g_strcmp0(config->monitor, "all") == 0 && count > 0) {
    for (guint i = 0; i < count; i++) {
      GdkMonitor *monitor = GDK_MONITOR(g_list_model_get_item(monitors, i));

      if (!monitor)
        continue;

      g_ptr_array_add(shell->bars, whelk_bar_new(shell->app, config, monitor));
      g_object_unref(monitor);
    }

    return;
  }

  if (config->monitor && *config->monitor &&
      g_strcmp0(config->monitor, "all") != 0 &&
      g_strcmp0(config->monitor, "default") != 0) {
    GdkMonitor *monitor = find_monitor_by_connector(config->monitor);

    if (!monitor) {
      LOG_C_WARN("Shell",
                 "No monitor named '%s' for bar '%s': using the default one",
                 config->monitor, config->name);
    }

    g_ptr_array_add(shell->bars, whelk_bar_new(shell->app, config, monitor));
    g_clear_object(&monitor);
    return;
  }

  g_ptr_array_add(shell->bars, whelk_bar_new(shell->app, config, NULL));
}

static void build_bars(WhelkShell *shell) {
  if (!shell || shell->building)
    return;

  shell->building = TRUE;
  g_ptr_array_set_size(shell->bars, 0);

  /* The old bars' menu trees are about to be freed; until the new ones have
   * published, the menu search has nothing true to say. */
  whelk_menu_search_clear();

  GPtrArray *configs = whelk_bridge_bars(shell->config_path);

  if (!configs || configs->len == 0) {
    if (configs)
      whelk_config_free_bars(configs);

    LOG_C_WARN("Shell",
               "No bars in the configuration: drawing the built-in bar");
    configs = whelk_config_default_bars();
  }

  for (guint i = 0; i < configs->len; i++)
    build_config(shell, g_ptr_array_index(configs, i));

  /* The shell's panels wear the first bar's palette.  It is read once, on the
   * build that has one, so a palette edit shows in the panels on the next
   * start (the bars themselves do follow a reload). */
  if (!shell->style && configs->len > 0)
    shell->style = whelk_bar_config_copy(g_ptr_array_index(configs, 0));

  whelk_config_free_bars(configs);

  LOG_C_INFO("Shell", "%u bar(s) up", shell->bars->len);
  shell->building = FALSE;
}

/* =========================================================================
 * HOT RELOAD
 * ========================================================================= */

static gboolean on_reload_timeout(gpointer user_data) {
  WhelkShell *shell = user_data;

  shell->reload_timeout = 0;

  LOG_C_INFO("Shell", "Configuration changed: reloading %s", shell->config_path);

  if (!whelk_bridge_load(shell->config_path))
    LOG_C_WARN("Shell", "the settings file could not be evaluated");

  whelk_menudict_load(whelk_bridge_menudict());
  build_bars(shell);
  whelk_bridge_start_repl(shell->config_path);

  return G_SOURCE_REMOVE;
}

static void on_config_changed(GFileMonitor *monitor, GFile *file,
                              GFile *other_file, GFileMonitorEvent event_type,
                              gpointer user_data) {
  (void)monitor;
  (void)file;
  (void)other_file;
  (void)event_type;

  WhelkShell *shell = user_data;

  /* One save is several events on most backends: restart the countdown. */
  if (shell->reload_timeout != 0)
    g_source_remove(shell->reload_timeout);

  shell->reload_timeout =
      g_timeout_add(CONFIG_RELOAD_DEBOUNCE_MS, on_reload_timeout, shell);
}

static gboolean on_monitors_changed_idle(gpointer user_data) {
  WhelkShell *shell = user_data;

  shell->monitor_idle = 0;
  LOG_C_INFO("Shell", "Monitors changed: rebuilding the bars");
  build_bars(shell);

  return G_SOURCE_REMOVE;
}

static void on_monitors_changed(GListModel *monitors, guint position,
                                guint removed, guint added, gpointer user_data) {
  (void)monitors;
  (void)position;
  (void)removed;
  (void)added;

  WhelkShell *shell = user_data;

  /* Rebuilding while the model is still emitting is reentrancy-prone: a bar
   * that is being destroyed queries the monitor list. */
  if (shell->monitor_idle == 0)
    shell->monitor_idle = g_idle_add(on_monitors_changed_idle, shell);
}

/* =========================================================================
 * LIFECYCLE
 * ========================================================================= */

WhelkShell *whelk_shell_new(GtkApplication *app) {
  WhelkShell *shell = g_new0(WhelkShell, 1);

  shell->app = app;
  shell->bars =
      g_ptr_array_new_with_free_func((GDestroyNotify)g_object_unref);
  shell->config_path = whelk_bridge_config_path();
  default_shell = shell;

  return shell;
}

void whelk_shell_start(WhelkShell *shell) {
  if (!shell)
    return;

  /* The structural stylesheet: module layout, state colours, the shape of a
   * bar.  Each bar's skin is loaded on top of it. */
  shell->base_css = gtk_css_provider_new();
  gtk_css_provider_load_from_resource(
      shell->base_css, "/org/jointhefreeworld/whelk/css/whelk.css");

  GdkDisplay *display = gdk_display_get_default();

  if (display)
    gtk_style_context_add_provider_for_display(
        display, GTK_STYLE_PROVIDER(shell->base_css),
        GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);

  LOG_C_INFO("Shell", "whelk %s, configuration: %s", PACKAGE_VERSION,
             shell->config_path);

  /* The notification service comes up before the bars: a module that shows a
   * count subscribes to it while it is being built, and has to find it. */
  shell->notifications = whelk_notifications_new();

  g_autoptr(GError) notification_error = NULL;

  whelk_notifications_start(shell->notifications, &notification_error);

  /* The settings file is read before the clipboard policy: (clipboard-settings)
   * lives in the same configuration as the bars, and asking for it first would
   * see the empty default. */
  whelk_bridge_init();

  /* The history starts listening now, not when the picker opens: by the time
   * somebody asks what they copied, the copy has to already be in it. */
  shell->clipboard = whelk_clipboard_new(50);

  /* The policy comes from (clipboard-settings) when the settings file has one.
   * Older setups keep working through WHELK_CLIPBOARD_PERSIST: 1 for the
   * state-directory file, a path for a specific one. */
  g_autoptr(WhelkClipboardConfig) clipboard_config =
      whelk_bridge_clipboard(shell->config_path);

  whelk_clipboard_set_limits(shell->clipboard, clipboard_config->max_entries,
                             clipboard_config->max_entry_bytes,
                             clipboard_config->max_total_bytes);
  whelk_clipboard_set_ttl(shell->clipboard, clipboard_config->ttl_seconds);
  whelk_clipboard_set_persist_pinned_only(
      shell->clipboard, clipboard_config->persist_only_pinned);
  whelk_clipboard_set_clear_on_lock(shell->clipboard,
                                    clipboard_config->clear_on_lock);

  const char *persist = NULL;

  if (clipboard_config->configured) {
    if (clipboard_config->persist)
      persist = clipboard_config->persist_path ? clipboard_config->persist_path
                                               : "auto";
  } else {
    const char *env = g_getenv("WHELK_CLIPBOARD_PERSIST");

    if (env && *env && g_strcmp0(env, "0") != 0 &&
        g_ascii_strcasecmp(env, "false") != 0)
      persist = env;
  }

  if (persist && *persist) {
    g_autofree char *path = NULL;

    if (strchr(persist, '/')) {
      path = g_strdup(persist);
    } else {
      const char *state_home = g_getenv("XDG_STATE_HOME");

      path = (state_home && *state_home)
                 ? g_build_filename(state_home, "whelk", "clipboard.variant",
                                    NULL)
                 : g_build_filename(g_get_home_dir(), ".local", "state",
                                    "whelk", "clipboard.variant", NULL);
    }

    const gboolean restored =
        whelk_clipboard_set_persistence(shell->clipboard, path);

    LOG_C_INFO("Shell", "clipboard history %s in %s",
               restored ? "restored" : "will be kept", path);
  }

  whelk_clipboard_start(shell->clipboard);

  /* The dictionary is Scheme, and the settings file may have added to it or
   * overridden an entry: read it after the configuration, before the bars. */
  whelk_menudict_load(whelk_bridge_menudict());

  build_bars(shell);

  if (!shell->style) {
    GPtrArray *defaults = whelk_config_default_bars();

    shell->style = whelk_bar_config_copy(g_ptr_array_index(defaults, 0));
    whelk_config_free_bars(defaults);
  }

  shell->center = whelk_notification_center_new(shell->app, shell->notifications,
                                                shell->style);
  shell->launcher = whelk_launcher_new(shell->app, shell->style);
  shell->network = whelk_network_panel_new(shell->app, shell->style);
  shell->osd = whelk_osd_new(shell->app, shell->style);

  /* Watch the configuration directory, and make it first: a shell that only
   * watches a directory the user already made cannot see the file that creates
   * it. */
  g_autofree char *config_dir = g_path_get_dirname(shell->config_path);

  g_mkdir_with_parents(config_dir, 0700);

  g_autoptr(GFile) directory = g_file_new_for_path(config_dir);

  shell->config_monitor =
      g_file_monitor_directory(directory, G_FILE_MONITOR_NONE, NULL, NULL);

  if (shell->config_monitor)
    g_signal_connect(shell->config_monitor, "changed",
                     G_CALLBACK(on_config_changed), shell);

  if (display) {
    GListModel *monitors = gdk_display_get_monitors(display);

    if (monitors)
      g_signal_connect(monitors, "items-changed",
                       G_CALLBACK(on_monitors_changed), shell);
  }
}

void whelk_shell_reload(WhelkShell *shell) {
  if (!shell)
    return;

  if (!whelk_bridge_load(shell->config_path))
    LOG_C_WARN("Shell", "the settings file could not be evaluated");

  whelk_menudict_load(whelk_bridge_menudict());
  build_bars(shell);
  whelk_bridge_start_repl(shell->config_path);
}

void whelk_shell_free(WhelkShell *shell) {
  if (!shell)
    return;

  if (shell->reload_timeout != 0)
    g_source_remove(shell->reload_timeout);

  if (shell->monitor_idle != 0)
    g_source_remove(shell->monitor_idle);

  /* Shutdown is the last chance to persist the clipboard history: dispose
   * stops the watcher, but it must not throw away entries whose debounced
   * save has not run yet. */
  if (shell->clipboard)
    whelk_clipboard_flush(shell->clipboard);

  if (shell->config_monitor)
    g_file_monitor_cancel(shell->config_monitor);

  g_clear_pointer(&shell->bars, g_ptr_array_unref);
  g_clear_object(&shell->center);
  g_clear_object(&shell->launcher);
  g_clear_object(&shell->network);
  g_clear_object(&shell->osd);
  g_clear_object(&shell->notifications);
  g_clear_object(&shell->clipboard);
  g_clear_pointer(&shell->style, whelk_bar_config_free);

  if (default_shell == shell)
    default_shell = NULL;

  GdkDisplay *display = gdk_display_get_default();

  if (display && shell->base_css)
    gtk_style_context_remove_provider_for_display(
        display, GTK_STYLE_PROVIDER(shell->base_css));

  g_clear_object(&shell->base_css);
  g_clear_object(&shell->config_monitor);
  g_free(shell->config_path);
  g_free(shell);
}

/* =========================================================================
 * THE SHELL'S SURFACES
 * ========================================================================= */

WhelkShell *whelk_shell_get_default(void) { return default_shell; }

WhelkNotifications *whelk_shell_notifications(WhelkShell *shell) {
  return shell ? shell->notifications : NULL;
}

WhelkClipboard *whelk_shell_clipboard(WhelkShell *shell) {
  return shell ? shell->clipboard : NULL;
}

void whelk_shell_show_launcher(WhelkShell *shell, const char *mode) {
  if (shell && shell->launcher)
    whelk_launcher_show(shell->launcher, mode);
}

void whelk_shell_show_menu(WhelkShell *shell, const char *command) {
  if (shell && shell->launcher)
    whelk_launcher_show_menu(shell->launcher, command);
}

void whelk_shell_toggle_notifications(WhelkShell *shell) {
  if (shell && shell->center)
    whelk_notification_center_toggle(shell->center);
}

void whelk_shell_toggle_dnd(WhelkShell *shell) {
  if (!shell || !shell->notifications)
    return;

  whelk_notifications_set_dnd(
      shell->notifications, !whelk_notifications_has_dnd(shell->notifications));
}

void whelk_shell_show_network_panel(WhelkShell *shell, const char *page) {
  if (shell && shell->network)
    whelk_network_panel_show(shell->network, page);
}

WhelkOsd *whelk_shell_osd(WhelkShell *shell) { return shell ? shell->osd : NULL; }
