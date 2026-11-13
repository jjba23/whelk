// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "guile_bridge.h"
#include "clipboard.h"
#include "logger.h"
#include "shell.h"

#include <libguile.h>
#include <locale.h>
#include <stdlib.h>

/* =========================================================================
 * THE PROCESS
 * =========================================================================
 * Guile boots first and GTK is created inside it, the arrangement lambdock
 * uses: `scm_boot_guile' gives the interpreter a live stack to work on before
 * any GObject exists, and every Scheme primitive registered below therefore runs
 * with a working evaluator.  The GTK main loop is then entered from inside
 * Guile and owns the process until the shell quits.
 *
 * The command line is how a keybinding talks to the running shell.  The second
 * `whelk' is not a second shell: GApplication forwards its arguments to the
 * first one, so `whelk --launcher' on a niri binding opens the launcher that is
 * already up, `whelk --notifications' toggles the centre, and `whelk --menu
 * "<command>"' opens a generated menu -- which is what replaces the rofi calls
 * in a keybinding file.
 */

static WhelkShell *g_shell = NULL;

/* --- the primitives Scheme may call ---------------------------------------- */

static gboolean reload_idle(gpointer user_data) {
  (void)user_data;

  if (g_shell)
    whelk_shell_reload(g_shell);

  return G_SOURCE_REMOVE;
}

/* Scheme: (reload-shell!) -- redraw every bar from the settings file. */
static SCM scm_reload_shell(void) {
  g_idle_add(reload_idle, NULL);
  return SCM_UNSPECIFIED;
}

/* Defines the primitives inside (whelk core), where configuration code resolves
 * them: `scm_c_use_module' only imports the module into the current one, so
 * defining without switching first would put them in (guile-user). */
static void setup_guile_primitives(void) {
  SCM module = scm_c_resolve_module("whelk core");
  SCM previous = scm_set_current_module(module);

  scm_c_define_gsubr("reload-shell!", 0, 0, 0,
                     (scm_t_subr)(intptr_t)scm_reload_shell);
  scm_c_export("reload-shell!", NULL);

  scm_set_current_module(previous);
}

/* --- GTK ---------------------------------------------------------------- */

static void on_quit_action(GSimpleAction *action, GVariant *parameter,
                           gpointer user_data) {
  (void)action;
  (void)parameter;

  g_application_quit(G_APPLICATION(user_data));
}

static void on_reload_action(GSimpleAction *action, GVariant *parameter,
                             gpointer user_data) {
  (void)action;
  (void)parameter;
  (void)user_data;

  if (g_shell)
    whelk_shell_reload(g_shell);
}

static void on_launcher_action(GSimpleAction *action, GVariant *parameter,
                               gpointer user_data) {
  (void)action;
  (void)user_data;

  const char *mode =
      parameter ? g_variant_get_string(parameter, NULL) : NULL;

  if (g_shell)
    whelk_shell_show_launcher(g_shell, (mode && *mode) ? mode : NULL);
}

static void on_menu_action(GSimpleAction *action, GVariant *parameter,
                           gpointer user_data) {
  (void)action;
  (void)user_data;

  if (!parameter || !g_variant_is_of_type(parameter, G_VARIANT_TYPE_STRING))
    return;

  if (g_shell)
    whelk_shell_show_menu(g_shell, g_variant_get_string(parameter, NULL));
}

static void on_notifications_action(GSimpleAction *action, GVariant *parameter,
                                    gpointer user_data) {
  (void)action;
  (void)parameter;
  (void)user_data;

  if (g_shell)
    whelk_shell_toggle_notifications(g_shell);
}

static void on_dnd_action(GSimpleAction *action, GVariant *parameter,
                          gpointer user_data) {
  (void)action;
  (void)parameter;
  (void)user_data;

  if (g_shell)
    whelk_shell_toggle_dnd(g_shell);
}

static void on_network_action(GSimpleAction *action, GVariant *parameter,
                              gpointer user_data) {
  (void)action;
  (void)user_data;

  const char *page =
      parameter ? g_variant_get_string(parameter, NULL) : NULL;

  if (g_shell)
    whelk_shell_show_network_panel(g_shell, (page && *page) ? page : NULL);
}

static void add_actions(GtkApplication *app) {
  static const GActionEntry entries[] = {
      {"quit", on_quit_action, NULL, NULL, NULL, {0, 0, 0}},
      {"reload", on_reload_action, NULL, NULL, NULL, {0, 0, 0}},
      {"launcher", on_launcher_action, "s", NULL, NULL, {0, 0, 0}},
      {"menu", on_menu_action, "s", NULL, NULL, {0, 0, 0}},
      {"notifications", on_notifications_action, NULL, NULL, NULL, {0, 0, 0}},
      {"dnd", on_dnd_action, NULL, NULL, NULL, {0, 0, 0}},
      {"network", on_network_action, "s", NULL, NULL, {0, 0, 0}},
  };

  g_action_map_add_action_entries(G_ACTION_MAP(app), entries,
                                  G_N_ELEMENTS(entries), app);
}

static void activate(GtkApplication *app, gpointer user_data) {
  (void)user_data;

  if (g_shell)
    return;

  whelk_bridge_setup_load_path();

  if (!whelk_bridge_use_core())
    LOG_C_WARN("Main",
               "the Scheme configuration engine did not load: running on the "
               "C defaults");

  setup_guile_primitives();
  add_actions(app);

  g_shell = whelk_shell_new(app);
  whelk_shell_start(g_shell);
}

/* --- the command line ---------------------------------------------------- */

static gboolean option_flag(GApplicationCommandLine *command_line,
                            const char *name) {
  gboolean value = FALSE;
  GVariantDict *options =
      g_application_command_line_get_options_dict(command_line);

  g_variant_dict_lookup(options, name, "b", &value);

  return value;
}

static const char *option_text(GApplicationCommandLine *command_line,
                               const char *name) {
  const char *value = NULL;
  GVariantDict *options =
      g_application_command_line_get_options_dict(command_line);

  g_variant_dict_lookup(options, name, "&s", &value);

  return value;
}

static int on_command_line(GApplication *application,
                           GApplicationCommandLine *command_line,
                           gpointer user_data) {
  (void)user_data;

  /* With handled command lines GApplication does not activate for us: the
   * first invocation is the one that has to bring the shell up. */
  if (!g_shell)
    activate(GTK_APPLICATION(application), NULL);

  if (!g_shell)
    return 0;

  if (option_flag(command_line, "quit")) {
    g_application_quit(application);
    return 0;
  }

  if (option_flag(command_line, "reload"))
    whelk_shell_reload(g_shell);

  const char *menu = option_text(command_line, "menu");

  if (menu && *menu)
    whelk_shell_show_menu(g_shell, menu);
  else if (option_flag(command_line, "launcher"))
    whelk_shell_show_launcher(g_shell, option_text(command_line, "launcher-mode"));
  else if (option_flag(command_line, "clipboard"))
    whelk_shell_show_launcher(g_shell, "clipboard");

  WhelkClipboard *clipboard = whelk_shell_clipboard(g_shell);

  if (clipboard && option_flag(command_line, "clipboard-status")) {
    g_application_command_line_print(
        command_line,
        "%s; %u entr%s; %" G_GSIZE_FORMAT " bytes\n",
        whelk_clipboard_status(clipboard), whelk_clipboard_count(clipboard),
        whelk_clipboard_count(clipboard) == 1 ? "y" : "ies",
        whelk_clipboard_bytes(clipboard));
  }

  if (clipboard && option_flag(command_line, "clipboard-pause")) {
    whelk_clipboard_set_paused(clipboard, !whelk_clipboard_is_paused(clipboard));
  }

  if (clipboard && option_flag(command_line, "clipboard-clear"))
    whelk_clipboard_clear(clipboard);

  if (clipboard && option_flag(command_line, "clipboard-clear-unpinned"))
    whelk_clipboard_clear_unpinned(clipboard);

  if (clipboard && option_flag(command_line, "clipboard-restart"))
    whelk_clipboard_restart(clipboard);

  if (option_flag(command_line, "notifications"))
    whelk_shell_toggle_notifications(g_shell);

  if (option_flag(command_line, "dnd"))
    whelk_shell_toggle_dnd(g_shell);

  const char *panel = option_text(command_line, "network");

  if (panel && *panel)
    whelk_shell_show_network_panel(g_shell, panel);

  return 0;
}

static void add_options(GtkApplication *app) {
  g_application_add_main_option(
      G_APPLICATION(app), "launcher", 'l', G_OPTION_FLAG_NONE,
      G_OPTION_ARG_NONE, "Show the launcher overlay", NULL);
  g_application_add_main_option(
      G_APPLICATION(app), "launcher-mode", 0, G_OPTION_FLAG_NONE,
      G_OPTION_ARG_STRING,
      "Launcher mode: app, window, run, calc, file, web, menu, ssh, quit, "
      "clipboard, menuitem",
      "MODE");
  g_application_add_main_option(
      G_APPLICATION(app), "clipboard", 0, G_OPTION_FLAG_NONE,
      G_OPTION_ARG_NONE, "Show the clipboard history", NULL);
  g_application_add_main_option(
      G_APPLICATION(app), "clipboard-status", 0, G_OPTION_FLAG_NONE,
      G_OPTION_ARG_NONE, "Print clipboard availability, count and size", NULL);
  g_application_add_main_option(
      G_APPLICATION(app), "clipboard-pause", 0, G_OPTION_FLAG_NONE,
      G_OPTION_ARG_NONE, "Pause or resume clipboard capture", NULL);
  g_application_add_main_option(
      G_APPLICATION(app), "clipboard-clear", 0, G_OPTION_FLAG_NONE,
      G_OPTION_ARG_NONE, "Delete the whole clipboard history", NULL);
  g_application_add_main_option(
      G_APPLICATION(app), "clipboard-clear-unpinned", 0, G_OPTION_FLAG_NONE,
      G_OPTION_ARG_NONE, "Delete every clipboard entry that is not pinned",
      NULL);
  g_application_add_main_option(
      G_APPLICATION(app), "clipboard-restart", 0, G_OPTION_FLAG_NONE,
      G_OPTION_ARG_NONE, "Restart the clipboard listener", NULL);
  g_application_add_main_option(
      G_APPLICATION(app), "menu", 'm', G_OPTION_FLAG_NONE, G_OPTION_ARG_STRING,
      "Show a menu from a command printing \"label<TAB>command\" lines",
      "COMMAND");
  g_application_add_main_option(
      G_APPLICATION(app), "notifications", 'n', G_OPTION_FLAG_NONE,
      G_OPTION_ARG_NONE, "Open or close the notification centre", NULL);
  g_application_add_main_option(G_APPLICATION(app), "dnd", 0,
                                G_OPTION_FLAG_NONE, G_OPTION_ARG_NONE,
                                "Turn do-not-disturb on or off", NULL);
  g_application_add_main_option(
      G_APPLICATION(app), "network", 0, G_OPTION_FLAG_NONE, G_OPTION_ARG_STRING,
      "Show the network panel: wifi or bluetooth", "PAGE");
  g_application_add_main_option(G_APPLICATION(app), "reload", 'r',
                                G_OPTION_FLAG_NONE, G_OPTION_ARG_NONE,
                                "Re-read the settings file", NULL);
  g_application_add_main_option(G_APPLICATION(app), "quit", 'q',
                                G_OPTION_FLAG_NONE, G_OPTION_ARG_NONE,
                                "Stop the running shell", NULL);
}

static void on_shutdown(GApplication *application, gpointer user_data) {
  (void)application;
  (void)user_data;

  g_clear_pointer(&g_shell, whelk_shell_free);
}

/* --- entry -------------------------------------------------------------- */

static void inner_main(void *data, int argc, char **argv) {
  (void)data;

  GtkApplication *app = gtk_application_new(
      "org.jointhefreeworld.whelk",
      G_APPLICATION_HANDLES_COMMAND_LINE | G_APPLICATION_DEFAULT_FLAGS);

  add_options(app);

  g_signal_connect(app, "activate", G_CALLBACK(activate), NULL);
  g_signal_connect(app, "command-line", G_CALLBACK(on_command_line), NULL);
  g_signal_connect(app, "shutdown", G_CALLBACK(on_shutdown), NULL);

  g_application_run(G_APPLICATION(app), argc, argv);

  g_object_unref(app);
}

int main(int argc, char **argv) {
  /* The clipboard helper is not the shell: it is the short-lived program
   * wl-paste --watch runs, and it must not try to become the GApplication (the
   * running shell would answer instead of the helper reading the selection). */
  for (int i = 1; i < argc; i++) {
    if (g_strcmp0(argv[i], "--clipboard-store") == 0)
      return whelk_clipboard_helper();
  }

#ifdef DEFAULT_GSK_RENDERER
  g_setenv("GSK_RENDERER", DEFAULT_GSK_RENDERER, FALSE);
#endif

  setlocale(LC_ALL, "");

  LOG_C_INFO("Main", "whelk %s starting", PACKAGE_VERSION);

  /* Boots the Guile runtime, initializes its garbage collector and calls
   * inner_main with a live interpreter on the stack. */
  scm_boot_guile(argc, argv, inner_main, NULL);

  return 0;
}
