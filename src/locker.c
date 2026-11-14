// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "locker.h"

#include "logger.h"
#include "shell.h"
#include "surface.h"
#include "util/lockpolicy.h"

#include <string.h>

#ifdef HAVE_PAM
#include <security/pam_appl.h>
#endif

/* The lock is a process-wide state, like the shell itself.  There is one lock
 * screen, not one per bar, and a second `whelk --lock` must not replace the
 * password field somebody is already typing into. */

typedef enum {
  LOCK_AUTH_OK,
  LOCK_AUTH_DENIED,
  LOCK_AUTH_UNAVAILABLE,
} LockAuthResult;

typedef struct {
  gint refs;
  GtkApplication *app;
  GPtrArray *surfaces; /* WhelkSurface*, one per output */
  GPtrArray *clocks;   /* GtkLabel*, borrowed from the surfaces */
  GPtrArray *dates;    /* GtkLabel*, borrowed from the surfaces */
  GtkWidget *entry;    /* the primary output's password field, borrowed */
  GtkWidget *error;    /* its error line, borrowed */
  guint clock_id;
  guint retry_id;
  guint focus_id;
  guint rebuild_idle;
  gint failures;
  gboolean authenticating;
  gboolean closing;
  gboolean unlocked;
  GThread *auth_thread;
  GListModel *monitor_model; /* owned while the lock is up */
  gulong monitor_handler;
} WhelkLocker;

typedef struct {
  WhelkLocker *locker;
  char *password;
  LockAuthResult result;
} LockAuthJob;

static WhelkLocker *locker = NULL;
static GtkApplication *lock_app = NULL;
static GDBusConnection *system_bus = NULL;
static guint lock_subscription = 0;
static gboolean watching_logind = FALSE;
static gboolean bus_requested = FALSE;

/* The compiler must not optimize a wipe away: this is the one place where a
 * password's lifetime is deliberately ended. */
static void secure_wipe(char *text) {
  if (!text)
    return;

  volatile char *p = text;

  while (*p)
    *p++ = '\0';
}

static void locker_ref(WhelkLocker *self) {
  g_atomic_int_inc(&self->refs);
}

static void locker_unref(WhelkLocker *self);

static void stop_timer(guint *id) {
  if (*id != 0) {
    g_source_remove(*id);
    *id = 0;
  }
}

static void destroy_surfaces(WhelkLocker *self) {
  stop_timer(&self->focus_id);

  self->entry = NULL;
  self->error = NULL;

  if (self->clocks)
    g_ptr_array_set_size(self->clocks, 0);

  if (self->dates)
    g_ptr_array_set_size(self->dates, 0);

  if (self->surfaces)
    g_ptr_array_set_size(self->surfaces, 0);
}

static void locker_free(WhelkLocker *self) {
  stop_timer(&self->clock_id);
  stop_timer(&self->retry_id);
  stop_timer(&self->focus_id);

  if (self->rebuild_idle != 0) {
    g_source_remove(self->rebuild_idle);
    self->rebuild_idle = 0;
  }

  if (self->monitor_model && self->monitor_handler != 0)
    g_signal_handler_disconnect(self->monitor_model, self->monitor_handler);

  g_clear_object(&self->monitor_model);

  if (self->auth_thread) {
    g_thread_join(self->auth_thread);
    self->auth_thread = NULL;
  }

  g_clear_pointer(&self->surfaces, g_ptr_array_unref);
  g_clear_pointer(&self->clocks, g_ptr_array_unref);
  g_clear_pointer(&self->dates, g_ptr_array_unref);
  g_free(self);
}

static void locker_unref(WhelkLocker *self) {
  if (self && g_atomic_int_dec_and_test(&self->refs))
    locker_free(self);
}

/* =========================================================================
 * THE CLOCK
 * ========================================================================= */

static gboolean on_clock_tick(gpointer data) {
  WhelkLocker *self = data;

  if (self->closing || self->unlocked)
    return G_SOURCE_REMOVE;

  g_autoptr(GDateTime) now = g_date_time_new_now_local();

  if (!now)
    return G_SOURCE_CONTINUE;

  g_autofree char *time = g_date_time_format(now, "%H:%M");
  g_autofree char *date = g_date_time_format(now, "%A, %d %B");

  for (guint i = 0; i < self->clocks->len; i++)
    gtk_label_set_text(g_ptr_array_index(self->clocks, i), time);

  for (guint i = 0; i < self->dates->len; i++)
    gtk_label_set_text(g_ptr_array_index(self->dates, i), date);

  return G_SOURCE_CONTINUE;
}

static gboolean focus_entry(gpointer data) {
  WhelkLocker *self = data;

  self->focus_id = 0;

  if (!self->closing && !self->unlocked && self->entry)
    gtk_widget_grab_focus(self->entry);

  return G_SOURCE_REMOVE;
}

/* =========================================================================
 * AUTHENTICATION
 * =========================================================================
 * The password is copied out of the GTK buffer, the GTK buffer is cleared at
 * once, and the copy lives only for the worker's call into PAM.  PAM is asked
 * about g_get_user_name(): the lock screen never accepts a different account.
 */

#ifdef HAVE_PAM
typedef struct {
  const char *password;
  gboolean supplied;
} LockConversation;

static int lock_conversation(int count, const struct pam_message **messages,
                             struct pam_response **responses, void *data) {
  LockConversation *conversation = data;

  if (count <= 0 || count > 16 || !messages || !responses || !conversation)
    return PAM_CONV_ERR;

  struct pam_response *answers = g_new0(struct pam_response, (gsize)count);

  for (int i = 0; i < count; i++) {
    const struct pam_message *message = messages[i];

    if (!message)
      goto failed;

    if (message->msg_style == PAM_PROMPT_ECHO_OFF) {
      if (conversation->supplied)
        goto failed;

      answers[i].resp = g_strdup(conversation->password);
      conversation->supplied = TRUE;
    } else if (message->msg_style == PAM_PROMPT_ECHO_ON) {
      answers[i].resp = g_strdup("");
    } else if (message->msg_style != PAM_ERROR_MSG &&
               message->msg_style != PAM_TEXT_INFO) {
      goto failed;
    }

    if ((message->msg_style == PAM_PROMPT_ECHO_OFF ||
         message->msg_style == PAM_PROMPT_ECHO_ON) &&
        !answers[i].resp)
      goto failed;
  }

  *responses = answers;
  return PAM_SUCCESS;

failed:
  for (int i = 0; i < count; i++) {
    secure_wipe(answers[i].resp);
    g_free(answers[i].resp);
  }

  g_free(answers);
  return PAM_CONV_ERR;
}

static LockAuthResult pam_authenticate_user(const char *password) {
  LockConversation conversation = {
      .password = password,
      .supplied = FALSE,
  };
  struct pam_conv conv = {
      .conv = lock_conversation,
      .appdata_ptr = &conversation,
  };
  pam_handle_t *handle = NULL;
  int status = pam_start("whelk-lock", g_get_user_name(), &conv, &handle);

  /* A dedicated stack is the narrow policy a lock screen wants.  Falling back
   * to `login' is for a machine that has not installed one; a whelk-lock stack
   * that exists but refuses is never bypassed by asking login instead. */
  if (status != PAM_SUCCESS) {
    if (handle)
      pam_end(handle, status);

    handle = NULL;
    status = pam_start("login", g_get_user_name(), &conv, &handle);
  }

  if (status != PAM_SUCCESS) {
    LOG_C_WARN("Locker", "PAM is not available (status %d)", status);
    if (handle)
      pam_end(handle, status);
    return LOCK_AUTH_UNAVAILABLE;
  }

  status =
      pam_authenticate(handle, PAM_SILENT | PAM_DISALLOW_NULL_AUTHTOK);

  if (status == PAM_SUCCESS)
    status = pam_acct_mgmt(handle, PAM_SILENT);

  if (status == PAM_SUCCESS) {
    pam_end(handle, PAM_SUCCESS);
    return LOCK_AUTH_OK;
  }

  LOG_C_DEBUG("Locker", "PAM refused authentication (status %d): %s", status,
              pam_strerror(handle, status));

  const LockAuthResult result =
      (status == PAM_AUTH_ERR || status == PAM_USER_UNKNOWN ||
       status == PAM_MAXTRIES || status == PAM_NEW_AUTHTOK_REQD)
          ? LOCK_AUTH_DENIED
          : LOCK_AUTH_UNAVAILABLE;

  pam_end(handle, status);
  return result;
}
#else
static LockAuthResult pam_authenticate_user(const char *password) {
  (void)password;
  return LOCK_AUTH_UNAVAILABLE;
}
#endif

static void locker_show_error(WhelkLocker *self, const char *text) {
  if (self->error)
    gtk_label_set_text(GTK_LABEL(self->error), text);
}

static gboolean on_retry_ready(gpointer data) {
  WhelkLocker *self = data;

  self->retry_id = 0;

  if (self->closing || self->unlocked)
    return G_SOURCE_REMOVE;

  if (self->entry) {
    gtk_widget_set_sensitive(self->entry, TRUE);
    gtk_editable_set_text(GTK_EDITABLE(self->entry), "");
    gtk_widget_grab_focus(self->entry);
  }

  locker_show_error(self, "Try again.");

  return G_SOURCE_REMOVE;
}

static void locker_fail(WhelkLocker *self, LockAuthResult result) {
  self->failures++;

  if (self->entry)
    gtk_widget_set_sensitive(self->entry, FALSE);

  locker_show_error(self,
                    result == LOCK_AUTH_UNAVAILABLE
                        ? "Authentication is unavailable. The screen stays locked."
                        : "Incorrect password.");

  stop_timer(&self->retry_id);

  const guint delay = whelk_lock_retry_delay_ms((guint)self->failures);
  self->retry_id = g_timeout_add(delay, on_retry_ready, self);
}

static void locker_unlock(WhelkLocker *self) {
  if (!self || self->unlocked)
    return;

  self->unlocked = TRUE;

  stop_timer(&self->clock_id);
  stop_timer(&self->retry_id);
  destroy_surfaces(self);

  if (locker == self) {
    locker = NULL;
    locker_unref(self); /* the process-wide reference */
  }
}

static gboolean on_auth_delivered(gpointer data) {
  LockAuthJob *job = data;
  WhelkLocker *self = job->locker;

  /* The worker scheduled this callback before it returned, so joining here
   * cannot block for longer than the PAM call already took. */
  if (self->auth_thread) {
    g_thread_join(self->auth_thread);
    self->auth_thread = NULL;
  }

  if (!self->closing && !self->unlocked) {
    self->authenticating = FALSE;

    if (job->result == LOCK_AUTH_OK)
      locker_unlock(self);
    else
      locker_fail(self, job->result);
  }

  secure_wipe(job->password);
  g_free(job->password);
  g_free(job);
  locker_unref(self);

  return G_SOURCE_REMOVE;
}

static gpointer lock_auth_worker(gpointer data) {
  LockAuthJob *job = data;

  job->result = pam_authenticate_user(job->password);

  secure_wipe(job->password);
  g_main_context_invoke(NULL, on_auth_delivered, job);

  return NULL;
}

static void submit_password(WhelkLocker *self) {
  if (!self || self->closing || self->unlocked || self->authenticating ||
      !self->entry)
    return;

  const char *buffer = gtk_editable_get_text(GTK_EDITABLE(self->entry));
  g_autofree char *password = g_strdup(buffer ? buffer : "");

  gtk_editable_set_text(GTK_EDITABLE(self->entry), "");

  if (*password == '\0') {
    locker_show_error(self, "Enter your password.");
    gtk_widget_grab_focus(self->entry);
    return;
  }

  self->authenticating = TRUE;
  gtk_widget_set_sensitive(self->entry, FALSE);
  locker_show_error(self, "Checking…");

  LockAuthJob *job = g_new0(LockAuthJob, 1);

  job->locker = self;
  locker_ref(self);
  job->password = g_strdup(password);
  secure_wipe(password);

  self->auth_thread =
      g_thread_new("whelk-lock-auth", lock_auth_worker, job);
}

static void on_password_activate(GtkPasswordEntry *entry, gpointer data) {
  (void)entry;

  submit_password(data);
}

static void on_unlock_clicked(GtkWidget *button, gpointer data) {
  (void)button;

  submit_password(data);
}

/* =========================================================================
 * THE SURFACES
 * ========================================================================= */

static void locker_rebuild(WhelkLocker *self);

static gboolean on_close_request(GtkWindow *window, gpointer data) {
  (void)window;
  (void)data;

  /* There is no window-manager close, no Escape and no click outside that ends
   * a lock.  The compositor can kill the process; it cannot ask this window to
   * go away. */
  return TRUE;
}

static GtkWidget *build_lock_content(WhelkLocker *self, gboolean primary) {
  GtkWidget *overlay = gtk_overlay_new();
  GtkWidget *background = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);

  gtk_widget_add_css_class(background, "whelk-lock-background");
  gtk_widget_set_hexpand(background, TRUE);
  gtk_widget_set_vexpand(background, TRUE);
  gtk_overlay_set_child(GTK_OVERLAY(overlay), background);

  GtkWidget *panel = gtk_box_new(GTK_ORIENTATION_VERTICAL, 10);

  gtk_widget_add_css_class(panel, "whelk-lock-panel");
  gtk_widget_set_halign(panel, GTK_ALIGN_CENTER);
  gtk_widget_set_valign(panel, GTK_ALIGN_CENTER);
  gtk_overlay_add_overlay(GTK_OVERLAY(overlay), panel);

  GtkWidget *clock = gtk_label_new(NULL);

  gtk_widget_add_css_class(clock, "whelk-lock-clock");
  gtk_box_append(GTK_BOX(panel), clock);
  g_ptr_array_add(self->clocks, clock);

  GtkWidget *date = gtk_label_new(NULL);

  gtk_widget_add_css_class(date, "whelk-lock-date");
  gtk_box_append(GTK_BOX(panel), date);
  g_ptr_array_add(self->dates, date);

  GtkWidget *message = gtk_label_new(
      primary ? "Enter your password" : "Locked");

  gtk_widget_add_css_class(message, "whelk-lock-message");
  gtk_box_append(GTK_BOX(panel), message);

  if (primary) {
    GtkWidget *entry = gtk_password_entry_new();

    gtk_password_entry_set_show_peek_icon(GTK_PASSWORD_ENTRY(entry), FALSE);
    gtk_widget_set_size_request(entry, 320, -1);
    gtk_widget_set_halign(entry, GTK_ALIGN_CENTER);
    g_signal_connect(entry, "activate", G_CALLBACK(on_password_activate),
                     self);
    gtk_box_append(GTK_BOX(panel), entry);
    self->entry = entry;

    GtkWidget *unlock = gtk_button_new_with_label("Unlock");

    gtk_widget_add_css_class(unlock, "whelk-lock-unlock");
    gtk_widget_set_halign(unlock, GTK_ALIGN_CENTER);
    g_signal_connect(unlock, "clicked", G_CALLBACK(on_unlock_clicked), self);
    gtk_box_append(GTK_BOX(panel), unlock);

    GtkWidget *error = gtk_label_new(NULL);

    gtk_widget_add_css_class(error, "whelk-lock-error");
    gtk_box_append(GTK_BOX(panel), error);
    self->error = error;
  }

  return overlay;
}

static void create_lock_surface(WhelkLocker *self, GdkMonitor *monitor,
                                gboolean primary) {
  WhelkSurface *surface = whelk_surface_new(self->app, "lock");

  whelk_surface_set_edge(surface, GTK_LAYER_SHELL_EDGE_TOP);
  whelk_surface_set_layer(surface, GTK_LAYER_SHELL_LAYER_OVERLAY);
  whelk_surface_set_keyboard_mode(
      surface, primary ? GTK_LAYER_SHELL_KEYBOARD_MODE_EXCLUSIVE
                       : GTK_LAYER_SHELL_KEYBOARD_MODE_NONE);
  whelk_surface_set_exclusive(surface, FALSE);
  whelk_surface_set_thickness(surface, 0);
  whelk_surface_set_anchors(surface, TRUE, TRUE, TRUE, TRUE);
  whelk_surface_set_namespace(surface, "whelk-lock");
  whelk_surface_set_widget_id(surface, "whelk-lock");

  if (monitor)
    whelk_surface_set_monitor(surface, monitor);

  whelk_surface_set_child(surface, build_lock_content(self, primary));

  GtkWindow *window = whelk_surface_get_window(surface);

  gtk_window_set_deletable(window, FALSE);
  g_signal_connect(window, "close-request", G_CALLBACK(on_close_request),
                   self);

  g_ptr_array_add(self->surfaces, surface);
  whelk_surface_present(surface);

  if (primary)
    self->focus_id = g_idle_add(focus_entry, self);
}

static void build_surfaces(WhelkLocker *self) {
  destroy_surfaces(self);

  GdkDisplay *display = gdk_display_get_default();

  if (!display) {
    LOG_C_WARN("Locker",
               "no display to cover: the session is locked without a visible "
               "surface");
    return;
  }

  GListModel *monitors = gdk_display_get_monitors(display);
  const guint count = monitors ? g_list_model_get_n_items(monitors) : 0;

  if (count == 0) {
    create_lock_surface(self, NULL, TRUE);
    return;
  }

  for (guint i = 0; i < count; i++) {
    GdkMonitor *monitor = GDK_MONITOR(g_list_model_get_item(monitors, i));

    create_lock_surface(self, monitor, i == 0);
    g_clear_object(&monitor);
  }
}

static gboolean on_rebuild_idle(gpointer data) {
  WhelkLocker *self = data;

  self->rebuild_idle = 0;

  if (!self->closing && !self->unlocked)
    build_surfaces(self);

  return G_SOURCE_REMOVE;
}

static void locker_rebuild(WhelkLocker *self) {
  if (!self || self->closing || self->unlocked || self->rebuild_idle != 0)
    return;

  /* Rebuilding while the monitor model is emitting is reentrant: a surface
   * asks the model for its monitor.  The idle is the same shape the bars use. */
  self->rebuild_idle = g_idle_add(on_rebuild_idle, self);
}

static void on_monitors_changed(GListModel *monitors, guint position,
                                guint removed, guint added, gpointer data) {
  (void)monitors;
  (void)position;
  (void)removed;
  (void)added;

  locker_rebuild(data);
}

/* =========================================================================
 * LOGIND
 * ========================================================================= */

static void on_logind_lock(GDBusConnection *connection, const char *sender,
                           const char *path, const char *interface,
                           const char *signal, GVariant *parameters,
                           gpointer data) {
  (void)connection;
  (void)sender;
  (void)path;
  (void)interface;
  (void)signal;
  (void)parameters;
  (void)data;

  if (!watching_logind)
    return;

  WhelkShell *shell = whelk_shell_get_default();

  if (shell)
    whelk_shell_lock(shell);
  else
    whelk_locker_lock();
}

static void on_system_bus(GObject *source, GAsyncResult *result,
                          gpointer data) {
  (void)source;
  (void)data;

  g_autoptr(GError) error = NULL;
  GDBusConnection *bus = g_bus_get_finish(result, &error);

  if (!bus) {
    LOG_C_DEBUG("Locker", "no system bus for logind: %s",
                error ? error->message : "unknown error");
    return;
  }

  if (!watching_logind) {
    g_object_unref(bus);
    return;
  }

  g_set_object(&system_bus, bus);
  lock_subscription = g_dbus_connection_signal_subscribe(
      bus, "org.freedesktop.login1", "org.freedesktop.login1.Manager", "Lock",
      "/org/freedesktop/login1", NULL, G_DBUS_SIGNAL_FLAGS_NONE,
      on_logind_lock, NULL, NULL);
  g_object_unref(bus);
}

/* =========================================================================
 * PUBLIC INTERFACE
 * ========================================================================= */

void whelk_locker_init(GtkApplication *app) {
  if (!app)
    return;

  lock_app = app;
  watching_logind = TRUE;

  if (bus_requested)
    return;

  bus_requested = TRUE;
  g_bus_get(G_BUS_TYPE_SYSTEM, NULL, on_system_bus, NULL);
}

gboolean whelk_locker_is_locked(void) {
  return locker && !locker->unlocked && !locker->closing;
}

void whelk_locker_lock(void) {
  if (whelk_locker_is_locked())
    return;

  if (!lock_app) {
    LOG_C_WARN("Locker", "lock requested before the shell initialized it");
    return;
  }

  WhelkLocker *self = g_new0(WhelkLocker, 1);

  self->refs = 1;
  self->app = lock_app;
  self->surfaces =
      g_ptr_array_new_with_free_func((GDestroyNotify)g_object_unref);
  self->clocks = g_ptr_array_new();
  self->dates = g_ptr_array_new();
  locker = self;

  GdkDisplay *display = gdk_display_get_default();

  if (display) {
    GListModel *monitors = gdk_display_get_monitors(display);

    if (monitors) {
      self->monitor_model = g_object_ref(monitors);
      self->monitor_handler = g_signal_connect(
          monitors, "items-changed", G_CALLBACK(on_monitors_changed), self);
    }
  }

  build_surfaces(self);
  self->clock_id = g_timeout_add_seconds(1, on_clock_tick, self);

  /* Fill the labels immediately instead of waiting for the first tick. */
  on_clock_tick(self);

  LOG_C_INFO("Locker", "screen locked");
}

void whelk_locker_shutdown(void) {
  watching_logind = FALSE;

  if (lock_subscription != 0) {
    g_dbus_connection_signal_unsubscribe(system_bus, lock_subscription);
    lock_subscription = 0;
  }

  g_clear_object(&system_bus);
  bus_requested = FALSE;

  if (locker) {
    WhelkLocker *self = locker;

    locker = NULL;
    self->closing = TRUE;

    if (self->auth_thread) {
      g_thread_join(self->auth_thread);
      self->auth_thread = NULL;
    }

    stop_timer(&self->clock_id);
    stop_timer(&self->retry_id);
    destroy_surfaces(self);
    locker_unref(self);
  }

  lock_app = NULL;
}
