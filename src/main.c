// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "guile_bridge.h"
#include "clipboard.h"
#include "logger.h"
#include "shell.h"

#include <errno.h>
#include <glib-unix.h>
#include <libguile.h>
#include <locale.h>
#include <signal.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

#ifdef __linux__
#include <sys/prctl.h>
#endif

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

/* =========================================================================
 * THE SUPERVISOR
 * =========================================================================
 * A GTK application cannot safely recover from a segmentation fault inside a
 * module: the fault is in this process, and unwinding it would be a lie.  What
 * it can do is put the UI in a child process and keep the smallest possible
 * parent beside it.  The parent owns no widgets and no configuration; it waits
 * for the child and starts it again after an abnormal exit.
 *
 * The two directions matter:
 *
 *   - a child crash (a module, GTK, a configuration bug) is logged and the
 *     shell comes back without the compositor restarting every session;
 *   - `kill <the whelk process>` reaches this parent, which forwards the
 *     signal to the child and waits for it.  The child's
 *     signal handler quits the GApplication, which runs the same shutdown path
 *     as `whelk --quit`: clipboard flushed, clipboard watcher stopped, REPL
 *     service disposed, notification bus name released, surfaces closed.
 *
 * A SIGKILL cannot be caught, so Linux's PR_SET_PDEATHSIG gives the child a
 * SIGTERM when the supervisor itself is killed.  That keeps the common
 * "kill the main process" case clean; an unavoidable SIGKILL of both processes
 * is a compositor restart, not a resource leak we can prevent from user space.
 */

#define WHELK_MAX_RESTARTS 5

static volatile sig_atomic_t g_supervisor_stop = 0;
static volatile sig_atomic_t g_supervisor_child = 0;

static void on_supervisor_signal(int signal_number) {
  const pid_t child = (pid_t)g_supervisor_child;

  g_supervisor_stop = 1;

  if (child > 0) {
    /* Only the UI is ours to stop.  A command a module launched may be a GUI
     * application the user wants to keep; killing its process group on quit
     * would close that application too.  The clipboard watcher and the other
     * services are stopped by the child's own shutdown path. */
    kill(child, signal_number);
  }
}

static void install_supervisor_signals(void) {
  struct sigaction action = {0};

  sigemptyset(&action.sa_mask);
  action.sa_handler = on_supervisor_signal;
  action.sa_flags = 0; /* no SA_RESTART: waitpid must return on a signal */

  sigaction(SIGTERM, &action, NULL);
  sigaction(SIGINT, &action, NULL);
  sigaction(SIGHUP, &action, NULL);
}

static void exec_supervised_child(char *const argv[], const char *executable,
                                  pid_t parent) {
  /* A process group of its own keeps the terminal's foreground signals from
   * reaching the UI twice: the supervisor receives them and forwards one
   * deliberate copy.  It is not a group to sweep on quit, because it also
   * contains applications a module command started. */
  setpgid(0, 0);

#ifdef __linux__
  /* If the supervisor is SIGKILLed, the kernel sends SIGTERM here.  Check the
   * parent afterwards: a parent that died between fork and prctl would
   * otherwise leave this child orphaned. */
  prctl(PR_SET_PDEATHSIG, SIGTERM);
#endif

  if (getppid() != parent)
    _exit(0);

  setenv("WHELK_SUPERVISED", "1", TRUE);

  execv(executable, argv);

  fprintf(stderr, "whelk: could not start the UI process: %s\n",
          g_strerror(errno));
  _exit(127);
}

/* The UI should leave its GApplication on SIGTERM, but a module can be stuck
 * in code that never returns to the main loop.  Do not let a kill of the main
 * process hang the supervisor forever: give the child five seconds, then make
 * the signal unavoidable. */
static void wait_for_child_stop(pid_t child) {
  const gint64 deadline =
      g_get_monotonic_time() + 5 * G_TIME_SPAN_SECOND;
  int status = 0;

  for (;;) {
    const pid_t waited = waitpid(child, &status, WNOHANG);

    if (waited == child || (waited < 0 && errno == ECHILD))
      return;
    if (waited < 0 && errno != EINTR)
      return;
    if (g_get_monotonic_time() >= deadline)
      break;

    g_usleep(100 * 1000);
  }

  kill(child, SIGKILL);

  while (waitpid(child, &status, 0) < 0 && errno == EINTR)
    ;
}

/* Returns -1 when supervision is disabled, otherwise the process exit status. */
static int run_supervisor(int argc, char **argv) {
  (void)argc;

  const char *disabled = g_getenv("WHELK_NO_SUPERVISE");

  if (disabled && *disabled && g_strcmp0(disabled, "0") != 0)
    return -1;

  if (g_getenv("WHELK_SUPERVISED"))
    return -1;

  g_autofree char *executable = g_file_read_link("/proc/self/exe", NULL);

  if (!executable && argv[0] && strchr(argv[0], '/'))
    executable = g_strdup(argv[0]);
  if (!executable && argv[0])
    executable = g_find_program_in_path(argv[0]);

  if (!executable) {
    fprintf(stderr, "whelk: could not find its own executable to supervise\n");
    return 127;
  }

  install_supervisor_signals();

  const pid_t supervisor = getpid();
  guint failures = 0;

  for (;;) {
    if (g_supervisor_stop)
      return 0;

    pid_t child = fork();

    if (child < 0) {
      fprintf(stderr, "whelk: fork failed: %s\n", g_strerror(errno));
      return 1;
    }

    if (child == 0)
      exec_supervised_child(argv, executable, supervisor);

    g_supervisor_child = child;
    setpgid(child, child); /* both sides race safely; EACCES/ESRCH is fine */
    const gint64 started = g_get_monotonic_time();

    /* A signal can arrive between fork and publishing the child's pid. */
    if (g_supervisor_stop)
      kill(child, SIGTERM);

    int status = 0;
    pid_t waited;

    do {
      waited = waitpid(child, &status, 0);
    } while (waited < 0 && errno == EINTR && !g_supervisor_stop);

    if (g_supervisor_stop) {
      /* The handler has already sent the signal.  Wait for the child's own
       * shutdown rather than returning while it still holds the display. */
      wait_for_child_stop(child);

      g_supervisor_child = 0;
      return 0;
    }

    g_supervisor_child = 0;

    if (waited < 0) {
      fprintf(stderr, "whelk: waitpid failed: %s\n", g_strerror(errno));
      return 1;
    }

    if (WIFEXITED(status) && WEXITSTATUS(status) == 0)
      return 0;

    if (WIFEXITED(status) && WEXITSTATUS(status) == 127)
      return 127;

    /* A shell that ran for a while and then crashed is a new incident, not a
     * continuation of a crash loop.  Only rapid repetitions count toward the
     * restart limit. */
    if (g_get_monotonic_time() - started > 30 * G_TIME_SPAN_SECOND)
      failures = 0;

    failures++;

    if (failures >= WHELK_MAX_RESTARTS) {
      fprintf(stderr,
              "whelk: the UI stopped %u times; leaving it stopped so a broken "
              "configuration is not restarted forever (WHELK_NO_SUPERVISE=1 "
              "runs without the supervisor)\n",
              failures);
      return 1;
    }

    const guint delay = MIN(4u, 1u << (failures - 1));

    if (WIFSIGNALED(status))
      fprintf(stderr,
              "whelk: the UI was killed by signal %d; restarting in %u s\n",
              WTERMSIG(status), delay);
    else
      fprintf(stderr, "whelk: the UI stopped (status %d); restarting in %u s\n",
              WEXITSTATUS(status), delay);
    g_usleep((gulong)delay * G_USEC_PER_SEC);
  }
}

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
  whelk_bridge_stop_repl();
}

/* --- entry -------------------------------------------------------------- */

/* A terminal's Ctrl-C, systemd's SIGTERM and a session's SIGHUP all take the
 * same road as the menu's Quit: leave the GApplication normally, so shutdown
 * emits and whelk_shell_free() runs.  This is also what the supervisor's
 * SIGTERM reaches when the shell is killed. */
static gboolean quit_on_signal(gpointer user_data) {
  LOG_C_INFO("Main", "termination signal received: shutting down");
  g_application_quit(G_APPLICATION(user_data));

  return G_SOURCE_REMOVE;
}

static void inner_main(void *data, int argc, char **argv) {
  (void)data;

  GtkApplication *app = gtk_application_new(
      "org.jointhefreeworld.whelk",
      G_APPLICATION_HANDLES_COMMAND_LINE | G_APPLICATION_DEFAULT_FLAGS);

  add_options(app);

  g_signal_connect(app, "activate", G_CALLBACK(activate), NULL);
  g_signal_connect(app, "command-line", G_CALLBACK(on_command_line), NULL);
  g_signal_connect(app, "shutdown", G_CALLBACK(on_shutdown), NULL);

  g_unix_signal_add(SIGTERM, quit_on_signal, app);
  g_unix_signal_add(SIGINT, quit_on_signal, app);
  g_unix_signal_add(SIGHUP, quit_on_signal, app);

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

  const int supervised = run_supervisor(argc, argv);

  if (supervised >= 0)
    return supervised;

  LOG_C_INFO("Main", "whelk %s starting", PACKAGE_VERSION);

  /* Boots the Guile runtime, initializes its garbage collector and calls
   * inner_main with a live interpreter on the stack. */
  scm_boot_guile(argc, argv, inner_main, NULL);

  return 0;
}
