// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The idle manager's interesting half is the decision, and that half is pure:
// a hint, a policy, an inhibitor and a clock.  The object's DND flag and its
// status report are checked without a system bus, which is also how the shell
// behaves on a machine where logind is absent.

#include "idle.h"
#include "util/idlepolicy.h"

#include <gio/gio.h>
#include <glib.h>
#include <string.h>

static void test_lock_delay(void) {
  /* Active session: no timer, whatever the policy. */
  g_assert_cmpint(whelk_idle_lock_delay(300, FALSE, FALSE, 0), ==,
                  WHELK_IDLE_NO_TIMER);

  /* Zero is the compatible policy: lock as soon as idle is reported. */
  g_assert_cmpint(whelk_idle_lock_delay(0, TRUE, FALSE, 0), ==, 0);

  /* A positive policy counts down from the first idle hint. */
  g_assert_cmpint(whelk_idle_lock_delay(300, TRUE, FALSE, 0), ==, 300);
  g_assert_cmpint(whelk_idle_lock_delay(300, TRUE, FALSE, 42), ==, 258);
  g_assert_cmpint(whelk_idle_lock_delay(300, TRUE, FALSE, 300), ==, 0);
  g_assert_cmpint(whelk_idle_lock_delay(300, TRUE, FALSE, 999), ==, 0);

  /* An inhibitor suspends the timer, and releasing it resumes from the
   * elapsed time rather than starting again. */
  g_assert_cmpint(whelk_idle_lock_delay(300, TRUE, TRUE, 42), ==,
                  WHELK_IDLE_NO_TIMER);

  /* A negative policy turns timed locking off even when idle. */
  g_assert_cmpint(whelk_idle_lock_delay(-1, TRUE, FALSE, 42), ==,
                  WHELK_IDLE_NO_TIMER);

  g_assert_cmpint(whelk_idle_lock_delay(300, TRUE, FALSE, -5), ==, 300);
}

static guint dnd_changes;

static void on_dnd_changed(WhelkIdle *idle, gpointer data) {
  (void)idle;
  (void)data;

  dnd_changes++;
}

static void test_dnd_ownership(void) {
  WhelkIdle *idle = whelk_idle_new();

  g_assert_false(whelk_idle_has_dnd(idle));

  dnd_changes = 0;
  g_signal_connect(idle, "dnd-changed", G_CALLBACK(on_dnd_changed), NULL);

  whelk_idle_set_dnd(idle, TRUE);
  g_assert_true(whelk_idle_has_dnd(idle));
  g_assert_cmpuint(dnd_changes, ==, 1);

  /* Setting the same value is not a change, and must not re-notify. */
  whelk_idle_set_dnd(idle, TRUE);
  g_assert_cmpuint(dnd_changes, ==, 1);

  whelk_idle_set_dnd(idle, FALSE);
  g_assert_false(whelk_idle_has_dnd(idle));
  g_assert_cmpuint(dnd_changes, ==, 2);

  /* The manager is the one place the flag lives, even with no bus. */
  g_assert_false(whelk_idle_is_inhibited(idle));

  whelk_idle_set_lock_after(idle, 300);
  g_assert_cmpint(whelk_idle_lock_after(idle), ==, 300);
  whelk_idle_set_lock_after(idle, -1);
  g_assert_cmpint(whelk_idle_lock_after(idle), ==, -1);

  g_object_unref(idle);
}

static void test_status_explains(void) {
  WhelkIdle *idle = whelk_idle_new();

  whelk_idle_set_lock_after(idle, 300);
  whelk_idle_set_dnd(idle, TRUE);

  g_autofree char *status = whelk_idle_describe(idle);

  g_assert_nonnull(status);
  g_assert_nonnull(strstr(status, "whelk idle manager"));
  g_assert_nonnull(strstr(status, "logind session: unavailable"));
  g_assert_nonnull(strstr(status, "logind IdleHint:"));
  g_assert_nonnull(strstr(status, "lock-after: 300 s"));
  g_assert_nonnull(strstr(status, "do not disturb: on"));
  g_assert_nonnull(strstr(status, "does not pause the lock timer"));
  g_assert_nonnull(strstr(status, "inhibitors on the system"));
  g_assert_nonnull(strstr(status, "Screen blanking itself"));

  g_object_unref(idle);
}

static gboolean quit_loop(gpointer data) {
  g_main_loop_quit(data);

  return G_SOURCE_REMOVE;
}

/* The crash this pins: on a machine where logind has no session for whelk's
 * process - a shell started from a terminal in a session scope it cannot see -
 * GetSessionByPID answers with an error, and the callback returned through a
 * label that ran the cleanup of two g_autoptrs whose declarations the jump had
 * skipped.  The uninitialised GVariant was unref'd, and glibc aborted with
 * "free(): invalid pointer" in the middle of startup.  The private bus below
 * has no logind, so the same path is taken without touching the host's
 * session. */
static void test_logind_absent(void) {
  if (!g_find_program_in_path("dbus-daemon")) {
    g_test_skip("dbus-daemon is not installed");
    return;
  }

  WhelkIdle *idle = whelk_idle_new();

  whelk_idle_set_lock_after(idle, 300);
  whelk_idle_start(idle);

  /* Let the failed lookup arrive and be handled. */
  GMainLoop *loop = g_main_loop_new(NULL, FALSE);

  g_timeout_add(300, quit_loop, loop);
  g_main_loop_run(loop);
  g_main_loop_unref(loop);

  /* A manager that survived it still answers, and still releases cleanly. */
  g_autofree char *status = whelk_idle_describe(idle);

  g_assert_nonnull(status);
  g_assert_nonnull(strstr(status, "logind IdleHint:"));
  g_assert_false(whelk_idle_is_inhibited(idle));

  whelk_idle_shutdown(idle);
  g_object_unref(idle);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  /* The idle manager asks logind on the system bus.  A private bus with no
   * logind on it is the machine where whelk was started from a terminal that
   * belongs to no session: GetSessionByPID fails, and the manager has to
   * survive that answer instead of walking off the end of its error path. */
  GTestDBus *bus = NULL;

  if (g_find_program_in_path("dbus-daemon")) {
    bus = g_test_dbus_new(G_TEST_DBUS_NONE);
    g_test_dbus_up(bus);
    g_setenv("DBUS_SYSTEM_BUS_ADDRESS", g_test_dbus_get_bus_address(bus), TRUE);
  } else {
    g_test_message("dbus-daemon is not installed: the logind tests skip");
  }

  g_test_add_func("/idle/lock-delay", test_lock_delay);
  g_test_add_func("/idle/dnd-ownership", test_dnd_ownership);
  g_test_add_func("/idle/status", test_status_explains);
  g_test_add_func("/idle/logind-absent", test_logind_absent);

  const int status = g_test_run();

  if (bus) {
    g_test_dbus_down(bus);
    g_object_unref(bus);
  }

  return status;
}
