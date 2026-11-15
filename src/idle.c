// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "idle.h"

#include <gio/gio.h>
#include <string.h>
#include <unistd.h>

#include "logger.h"
#include "util/idlepolicy.h"
#include "util/inhibitors.h"

#define LOGIND_NAME "org.freedesktop.login1"
#define LOGIND_PATH "/org/freedesktop/login1"
#define LOGIND_MANAGER "org.freedesktop.login1.Manager"
#define LOGIND_SESSION "org.freedesktop.login1.Session"
#define INHIBIT_TIMEOUT_MS 5000
#define INHIBIT_REASON_DEFAULT "presentation mode"

enum {
  SIGNAL_DND_CHANGED,
  SIGNAL_LAST,
};

static guint signals[SIGNAL_LAST] = {0};

struct _WhelkIdle {
  GObject parent_instance;

  GDBusConnection *bus;
  char *session_path;
  guint session_subscription;
  gboolean started;

  gboolean idle;
  gint64 idle_since;
  /* Set when the presentation inhibitor is released: the elapsed idle time is
   * frozen while it is held, so the remaining delay is honoured afterwards. */
  gint64 idle_origin_us;
  gint64 inhibit_frozen_us;
  gboolean inhibit_froze_idle;
  int lock_after;
  guint timer_id;
  gboolean lock_requested;

  gboolean inhibited;
  char *inhibit_reason;
  int inhibit_fd;

  gboolean dnd;

  char *last_decision;

  WhelkIdleLockFunc lock_action;
  gpointer lock_data;
};

G_DEFINE_TYPE(WhelkIdle, whelk_idle, G_TYPE_OBJECT)

/* =========================================================================
 * THE TIMER
 * ========================================================================= */

static void cancel_timer(WhelkIdle *self) {
  if (self->timer_id != 0) {
    g_source_remove(self->timer_id);
    self->timer_id = 0;
  }
}

static gint64 idle_elapsed_us(WhelkIdle *self) {
  if (!self->idle)
    return 0;

  const gint64 origin =
      self->idle_origin_us > 0 ? self->idle_origin_us : self->idle_since;

  if (origin <= 0)
    return 0;

  const gint64 elapsed = g_get_monotonic_time() - origin;

  return elapsed > 0 ? elapsed : 0;
}

static gint64 idle_seconds(WhelkIdle *self) {
  return idle_elapsed_us(self) / G_USEC_PER_SEC;
}

static gboolean on_lock_timer(gpointer data) {
  WhelkIdle *self = data;

  self->timer_id = 0;

  if (!self->started || !self->idle || self->inhibited)
    return G_SOURCE_REMOVE;

  const gint64 idle_for = idle_seconds(self);
  const int remaining =
      whelk_idle_lock_delay(self->lock_after, self->idle, self->inhibited,
                            idle_for);

  if (remaining > 0) {
    g_free(self->last_decision);
    self->last_decision =
        g_strdup_printf("waiting: %d s of the %d s policy left", remaining,
                        self->lock_after);
    self->timer_id =
        g_timeout_add_seconds_full(G_PRIORITY_DEFAULT, (guint)remaining,
                                   on_lock_timer, g_object_ref(self),
                                   g_object_unref);
    return G_SOURCE_REMOVE;
  }

  self->lock_requested = TRUE;
  g_free(self->last_decision);
  self->last_decision = g_strdup_printf(
      "lock timer fired after %" G_GINT64_FORMAT " s idle", idle_for);
  LOG_C_INFO("Idle", "%s", self->last_decision);

  if (self->lock_action)
    self->lock_action(self->lock_data);

  return G_SOURCE_REMOVE;
}

/* Re-evaluates the policy against the current hint, inhibitor and clock. */
static void schedule_timer(WhelkIdle *self) {
  cancel_timer(self);

  const gint64 idle_for = idle_seconds(self);
  const int delay = whelk_idle_lock_delay(
      self->lock_after, self->idle, self->inhibited, idle_for);

  if (delay == WHELK_IDLE_NO_TIMER) {
    g_free(self->last_decision);

    if (!self->idle)
      self->last_decision = g_strdup("no timer: the session is active");
    else if (self->inhibited)
      self->last_decision =
          g_strdup("timer suspended: whelk holds the idle inhibitor");
    else
      self->last_decision = g_strdup("timed locking is disabled");

    LOG_C_DEBUG("Idle", "%s", self->last_decision);
    return;
  }

  g_free(self->last_decision);

  if (delay == 0) {
    self->last_decision =
        g_strdup(self->lock_after == 0
                     ? "locking as soon as logind reports idle"
                     : "the idle delay has already elapsed: locking now");
    self->timer_id = g_idle_add_full(G_PRIORITY_DEFAULT, on_lock_timer,
                                     g_object_ref(self), g_object_unref);
  } else {
    self->last_decision = g_strdup_printf(
        "lock timer armed: %d s left of the %d s policy", delay,
        self->lock_after);
    self->timer_id =
        g_timeout_add_seconds_full(G_PRIORITY_DEFAULT, (guint)delay,
                                   on_lock_timer, g_object_ref(self),
                                   g_object_unref);
  }

  LOG_C_INFO("Idle", "%s", self->last_decision);
}

/* =========================================================================
 * LOGIND
 * ========================================================================= */

static void apply_idle_hint(WhelkIdle *self, gboolean idle) {
  if (!self || idle == self->idle)
    return;

  self->idle = idle;

  if (idle) {
    self->idle_since = g_get_monotonic_time();
    self->idle_origin_us = 0;
    self->inhibit_frozen_us = 0;
    self->inhibit_froze_idle = FALSE;
    self->lock_requested = FALSE;
    LOG_C_INFO("Idle", "logind reports the session idle");
  } else {
    self->idle_since = 0;
    self->idle_origin_us = 0;
    self->inhibit_frozen_us = 0;
    self->inhibit_froze_idle = FALSE;
    self->lock_requested = FALSE;
    LOG_C_INFO("Idle", "logind reports the session active");
  }

  schedule_timer(self);
}

static void on_session_properties(GDBusConnection *connection,
                                  const char *sender, const char *path,
                                  const char *interface, const char *signal,
                                  GVariant *parameters, gpointer data) {
  (void)connection;
  (void)sender;
  (void)path;
  (void)interface;
  (void)signal;

  WhelkIdle *self = data;
  const char *changed_interface = NULL;
  g_autoptr(GVariant) changed = NULL;
  g_autoptr(GVariant) invalidated = NULL;
  gboolean idle = FALSE;

  if (!self || !parameters ||
      !g_variant_is_of_type(parameters, G_VARIANT_TYPE("(sa{sv}as)")))
    return;

  g_variant_get(parameters, "(&s@a{sv}@as)", &changed_interface, &changed,
                &invalidated);

  if (g_strcmp0(changed_interface, LOGIND_SESSION) != 0)
    return;

  if (!g_variant_lookup(changed, "IdleHint", "b", &idle))
    return;

  apply_idle_hint(self, idle);
}

static void on_session_path(GObject *source, GAsyncResult *result,
                            gpointer data) {
  WhelkIdle *self = data;

  (void)source;

  /* Every automatic cleanup this function owns is declared here, before the
   * first `goto done'.  A g_autoptr declared after the jump keeps whatever the
   * stack slot held, and the cleanup at `done' then unrefs garbage; that was
   * the startup crash on a machine where logind has no session for this
   * process, because that path returns through `done'. */
  g_autoptr(GError) error = NULL;
  g_autoptr(GVariant) reply = NULL;
  g_autoptr(GError) hint_error = NULL;
  g_autoptr(GVariant) current = NULL;
  const char *path = NULL;

  reply =
      g_dbus_connection_call_finish(G_DBUS_CONNECTION(source), result, &error);

  if (!self->started)
    goto done;

  if (!reply) {
    LOG_C_DEBUG("Idle",
                "could not find the logind session for the idle hint: %s",
                error ? error->message : "unknown error");
    goto done;
  }

  g_variant_get(reply, "(&o)", &path);

  if (!self->bus || !path || !*path)
    goto done;

  g_free(self->session_path);
  self->session_path = g_strdup(path);

  self->session_subscription = g_dbus_connection_signal_subscribe(
      self->bus, LOGIND_NAME, LOGIND_SESSION, "PropertiesChanged",
      self->session_path, NULL, G_DBUS_SIGNAL_FLAGS_NONE,
      on_session_properties, self, NULL);

  /* A shell restarted while the session is already idle has to learn the
   * current hint, not wait for the next transition. */
  current = g_dbus_connection_call_sync(
      self->bus, LOGIND_NAME, self->session_path,
      "org.freedesktop.DBus.Properties", "Get",
      g_variant_new("(ss)", LOGIND_SESSION, "IdleHint"),
      G_VARIANT_TYPE("(v)"), G_DBUS_CALL_FLAGS_NONE, INHIBIT_TIMEOUT_MS,
      NULL, &hint_error);

  if (current) {
    g_autoptr(GVariant) value = NULL;

    g_variant_get(current, "(v)", &value);

    if (value && g_variant_is_of_type(value, G_VARIANT_TYPE_BOOLEAN))
      apply_idle_hint(self, g_variant_get_boolean(value));
  } else {
    LOG_C_DEBUG("Idle", "could not read the current idle hint: %s",
                hint_error ? hint_error->message : "unknown error");
  }

done:
  g_object_unref(self);
}

static void on_system_bus(GObject *source, GAsyncResult *result,
                          gpointer data) {
  WhelkIdle *self = data;

  (void)source;

  g_autoptr(GError) error = NULL;
  GDBusConnection *bus = g_bus_get_finish(result, &error);

  if (!self->started) {
    g_clear_object(&bus);
    g_object_unref(self);
    return;
  }

  if (!bus) {
    LOG_C_WARN("Idle",
               "no system bus: the idle hint and logind inhibitor are "
               "unavailable: %s",
               error ? error->message : "unknown error");
    g_object_unref(self);
    return;
  }

  g_set_object(&self->bus, bus);
  g_object_unref(bus);

  g_dbus_connection_call(
      self->bus, LOGIND_NAME, LOGIND_PATH, LOGIND_MANAGER,
      "GetSessionByPID", g_variant_new("(u)", (guint)getpid()),
      G_VARIANT_TYPE("(o)"), G_DBUS_CALL_FLAGS_NONE, INHIBIT_TIMEOUT_MS, NULL,
      on_session_path, g_object_ref(self));

  g_object_unref(self);
}

/* =========================================================================
 * PUBLIC INTERFACE
 * ========================================================================= */

WhelkIdle *whelk_idle_new(void) {
  return g_object_new(WHELK_TYPE_IDLE, NULL);
}

void whelk_idle_start(WhelkIdle *self) {
  if (!self || self->started)
    return;

  self->started = TRUE;
  g_bus_get(G_BUS_TYPE_SYSTEM, NULL, on_system_bus, g_object_ref(self));
}

void whelk_idle_shutdown(WhelkIdle *self) {
  if (!self)
    return;

  self->started = FALSE;
  cancel_timer(self);

  if (self->bus && self->session_subscription != 0) {
    g_dbus_connection_signal_unsubscribe(self->bus,
                                         self->session_subscription);
    self->session_subscription = 0;
  }

  g_clear_pointer(&self->session_path, g_free);
  g_clear_object(&self->bus);

  /* Closing the handle is what releases a logind inhibitor; there is no
   * separate Release method for an fd-based lock. */
  if (self->inhibit_fd >= 0) {
    close(self->inhibit_fd);
    self->inhibit_fd = -1;
  }

  self->inhibited = FALSE;
  self->inhibit_froze_idle = FALSE;
  g_clear_pointer(&self->inhibit_reason, g_free);
}

void whelk_idle_set_lock_action(WhelkIdle *self, WhelkIdleLockFunc action,
                                gpointer user_data) {
  if (!self)
    return;

  self->lock_action = action;
  self->lock_data = user_data;
}

void whelk_idle_set_lock_after(WhelkIdle *self, int seconds) {
  if (!self)
    return;

  if (self->lock_after == seconds)
    return;

  self->lock_after = seconds;
  self->lock_requested = FALSE;
  LOG_C_INFO("Idle",
             "idle policy: %s",
             seconds < 0 ? "timed locking disabled"
                         : (seconds == 0
                                ? "lock as soon as logind reports idle"
                                : "lock after the idle delay"));
  schedule_timer(self);
}

int whelk_idle_lock_after(WhelkIdle *self) {
  return self ? self->lock_after : 0;
}

gboolean whelk_idle_is_idle(WhelkIdle *self) {
  return self ? self->idle : FALSE;
}

gboolean whelk_idle_is_inhibited(WhelkIdle *self) {
  return self ? self->inhibited : FALSE;
}

const char *whelk_idle_inhibit_reason(WhelkIdle *self) {
  return (self && self->inhibit_reason) ? self->inhibit_reason : "";
}

gboolean whelk_idle_set_inhibited(WhelkIdle *self, gboolean inhibited,
                                  const char *reason) {
  if (!self)
    return FALSE;

  if (self->inhibited == inhibited)
    return TRUE;

  if (inhibited) {
    if (!self->bus) {
      LOG_C_WARN("Idle",
                 "cannot take the idle inhibitor: no system bus yet");
      return FALSE;
    }

    const char *why = (reason && *reason) ? reason : INHIBIT_REASON_DEFAULT;
    g_autoptr(GError) error = NULL;
    g_autoptr(GVariant) reply = g_dbus_connection_call_sync(
        self->bus, LOGIND_NAME, LOGIND_PATH, LOGIND_MANAGER, "Inhibit",
        g_variant_new("(ssss)", "idle", "whelk", why, "block"),
        G_VARIANT_TYPE("(h)"), G_DBUS_CALL_FLAGS_NONE, INHIBIT_TIMEOUT_MS,
        NULL, &error);

    if (!reply) {
      LOG_C_WARN("Idle", "could not take the logind idle inhibitor: %s",
                 error ? error->message : "unknown error");
      return FALSE;
    }

    gint32 fd = -1;

    g_variant_get(reply, "(h)", &fd);

    if (fd < 0) {
      LOG_C_WARN("Idle", "logind returned no inhibitor handle");
      return FALSE;
    }

    self->inhibit_fd = fd;
    self->inhibited = TRUE;
    self->inhibit_frozen_us = idle_elapsed_us(self);
    self->inhibit_froze_idle = self->idle;
    g_free(self->inhibit_reason);
    self->inhibit_reason = g_strdup(why);
    LOG_C_INFO("Idle",
               "presentation inhibitor held (logind idle): %s", why);
  } else {
    if (self->inhibit_fd >= 0) {
      close(self->inhibit_fd);
      self->inhibit_fd = -1;
    }

    self->inhibited = FALSE;
    g_clear_pointer(&self->inhibit_reason, g_free);

    /* Resume the clock where it stopped, not where wall time has reached.
     * The frozen span can legitimately be zero, so the boolean, not the
     * number, says whether there was an idle session to resume. */
    if (self->idle && self->inhibit_froze_idle)
      self->idle_origin_us =
          g_get_monotonic_time() - self->inhibit_frozen_us;

    self->inhibit_frozen_us = 0;
    self->inhibit_froze_idle = FALSE;
    LOG_C_INFO("Idle", "presentation inhibitor released");
  }

  /* The timer is suspended while the inhibitor is held, and picks up again
   * with the remaining delay when it is released. */
  schedule_timer(self);
  return TRUE;
}

gboolean whelk_idle_has_dnd(WhelkIdle *self) {
  return self ? self->dnd : FALSE;
}

void whelk_idle_set_dnd(WhelkIdle *self, gboolean dnd) {
  if (!self || self->dnd == dnd)
    return;

  self->dnd = dnd;
  LOG_C_INFO("Idle", "do not disturb: %s (does not pause the lock timer)",
             dnd ? "on" : "off");
  g_signal_emit(self, signals[SIGNAL_DND_CHANGED], 0);
}

char *whelk_idle_describe(WhelkIdle *self) {
  GString *out = g_string_new(NULL);

  if (!self) {
    g_string_append(out, "whelk idle manager: not running\n");
    return g_string_free(out, FALSE);
  }

  const gint64 idle_for = idle_seconds(self);
  const int delay = whelk_idle_lock_delay(
      self->lock_after, self->idle, self->inhibited, idle_for);

  g_string_append(out, "whelk idle manager\n");
  g_string_append_printf(out, "  logind session: %s\n",
                         self->session_path
                             ? self->session_path
                             : "unavailable -- no system bus or session lookup");
  g_string_append_printf(out, "  logind IdleHint: %s", self->idle ? "idle" : "active");

  if (self->idle)
    g_string_append_printf(out, " (%" G_GINT64_FORMAT " s)",
                           idle_for);

  g_string_append(out, "\n");
  g_string_append_printf(
      out,
      "  lock-after: %d s (idle-settings #:lock-after; 0 locks immediately)\n",
      self->lock_after);

  g_string_append(out, "  lock timer: ");

  if (self->inhibited)
    g_string_append(out,
                    "suspended while the presentation inhibitor is held\n");
  else if (!self->idle)
    g_string_append(out, "not running: the session is active\n");
  else if (self->lock_requested)
    g_string_append(out, "fired; the lock has been requested\n");
  else if (delay == 0)
    g_string_append(out, "firing now\n");
  else if (delay == WHELK_IDLE_NO_TIMER)
    g_string_append(out, "disabled by the policy\n");
  else
    g_string_append_printf(out, "%d s remaining\n", delay);

  g_string_append_printf(
      out, "  last decision: %s\n",
      self->last_decision ? self->last_decision : "(none yet)");

  if (self->inhibited)
    g_string_append_printf(
        out,
        "  presentation inhibitor: on (%s) -- whelk holds logind's idle "
        "lock and its own timer is paused; the compositor's blanking is "
        "still the compositor's\n",
        self->inhibit_reason ? self->inhibit_reason : "no reason given");
  else
    g_string_append(out,
                    "  presentation inhibitor: off -- run `whelk --inhibit' "
                    "to hold one\n");

  g_string_append_printf(
      out,
      "  do not disturb: %s (DND silences toasts; it does not pause the lock "
      "timer)\n",
      self->dnd ? "on" : "off");

  g_string_append(out,
                  "  inhibitors on the system (including whelk's own):\n");

  g_autofree gchar *stdout_text = NULL;
  gint wait_status = 0;
  g_autoptr(GError) spawn_error = NULL;

  if (g_spawn_command_line_sync("systemd-inhibit --list --no-pager",
                                &stdout_text, NULL, &wait_status,
                                &spawn_error)) {
    g_autoptr(GPtrArray) list = whelk_inhibitors_parse(stdout_text);
    g_autofree char *description = whelk_inhibitors_describe(list);

    if (description) {
      g_auto(GStrv) lines = g_strsplit(description, "\n", -1);

      for (guint i = 0; lines && lines[i]; i++)
        g_string_append_printf(out, "    %s\n", lines[i]);
    } else {
      g_string_append(out, "    none held\n");
    }
  } else {
    g_string_append_printf(out, "    could not read systemd-inhibit: %s\n",
                           spawn_error ? spawn_error->message : "not installed");
  }

  g_string_append(
      out,
      "  Screen blanking itself belongs to the compositor and logind; whelk "
      "owns the lock timer and its own idle inhibitor.\n");

  return g_string_free(out, FALSE);
}

/* =========================================================================
 * LIFECYCLE
 * ========================================================================= */

static void whelk_idle_dispose(GObject *object) {
  WhelkIdle *self = WHELK_IDLE(object);

  whelk_idle_shutdown(self);

  G_OBJECT_CLASS(whelk_idle_parent_class)->dispose(object);
}

static void whelk_idle_finalize(GObject *object) {
  WhelkIdle *self = WHELK_IDLE(object);

  g_free(self->session_path);
  g_free(self->inhibit_reason);
  g_free(self->last_decision);
  g_clear_object(&self->bus);

  G_OBJECT_CLASS(whelk_idle_parent_class)->finalize(object);
}

static void whelk_idle_class_init(WhelkIdleClass *klass) {
  GObjectClass *object_class = G_OBJECT_CLASS(klass);

  object_class->dispose = whelk_idle_dispose;
  object_class->finalize = whelk_idle_finalize;

  /**
   * WhelkIdle::dnd-changed:
   *
   * Do not disturb was toggled.  The notification service re-emits this on its
   * own `dnd-changed' signal so a module that listens to notifications keeps
   * working, while the flag itself has one owner.
   */
  signals[SIGNAL_DND_CHANGED] = g_signal_new(
      "dnd-changed", G_TYPE_FROM_CLASS(klass), G_SIGNAL_RUN_LAST, 0, NULL, NULL,
      NULL, G_TYPE_NONE, 0);
}

static void whelk_idle_init(WhelkIdle *self) {
  self->lock_after = 0;
  self->inhibit_fd = -1;
}
