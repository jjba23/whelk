// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "locker.h"

#include "logger.h"
#include "shell.h"
#include "surface.h"
#include "util/lockpolicy.h"

#include <errno.h>
#include <gtk4-session-lock.h>
#include <glib/gstdio.h>
#include <string.h>
#include <unistd.h>

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
  GPtrArray *session_views; /* LockView*, borrowed, ext-session-lock only */
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
  GtkSessionLockInstance *session_lock; /* owned while the protocol is in use */
  gboolean session_has_primary;
  guint protocol_unlock_id;
  guint session_rebuild_id;
  gboolean rebuilding_session_lock;
} WhelkLocker;

typedef struct {
  WhelkLocker *locker;
  char *password;
  LockAuthResult result;
} LockAuthJob;

/* One protocol lock window.  The session-lock library owns the GtkWindow and
 * destroys it when the lock ends or its monitor disappears; keep the widget
 * pointers here so the clock and the password field can be detached before
 * the window's children are freed. */
typedef struct {
  WhelkLocker *locker;
  GtkWidget *clock;
  GtkWidget *date;
  GtkWidget *entry;
  GtkWidget *error;
  GtkWidget *window;
  gboolean primary;
} LockView;

static WhelkLocker *locker = NULL;
static GtkApplication *lock_app = NULL;
static GDBusConnection *system_bus = NULL;
static guint lock_subscription = 0;
static guint sleep_subscription = 0;
static guint session_subscription = 0;
static guint session_props_subscription = 0;
static char *session_path = NULL;
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

/* The UI process is restarted by whelk's supervisor after a crash.  Keep the
 * failed-attempt count in the user's runtime directory so that restart cannot
 * reset the delay.  This file is only a local throttle; a site that wants a
 * system-wide lockout should add pam_faillock to its whelk-lock stack. */
static char *lock_failure_path(void) {
  const char *override = g_getenv("WHELK_LOCK_STATE_FILE");

  if (override && *override)
    return g_strdup(override);

  const char *runtime = g_get_user_runtime_dir();

  if (!runtime || !*runtime)
    return NULL;

  return g_build_filename(runtime, "whelk", "lock-failures", NULL);
}

static gint load_lock_failures(void) {
  g_autofree char *path = lock_failure_path();
  g_autofree char *contents = NULL;

  if (!path || !g_file_get_contents(path, &contents, NULL, NULL))
    return 0;

  const gint64 parsed = g_ascii_strtoll(contents, NULL, 10);

  return parsed <= 0 ? 0 : (parsed > G_MAXINT ? G_MAXINT : (gint)parsed);
}

static void save_lock_failures(gint failures) {
  g_autofree char *path = lock_failure_path();
  g_autofree char *directory = path ? g_path_get_dirname(path) : NULL;
  g_autofree char *contents = g_strdup_printf("%d\n", failures);
  g_autoptr(GError) error = NULL;

  if (!path)
    return;

  if (g_mkdir_with_parents(directory, 0700) != 0) {
    LOG_C_DEBUG("Locker", "could not create lock-state directory: %s",
                g_strerror(errno));
    return;
  }

  if (!g_file_set_contents(path, contents, -1, &error)) {
    LOG_C_DEBUG("Locker", "could not persist lock failures: %s",
                error ? error->message : "unknown error");
    return;
  }

  /* The runtime directory is private, but do not leave the file readable by
   * anything that later gets access to it. */
  if (g_chmod(path, 0600) != 0)
    LOG_C_DEBUG("Locker", "could not restrict lock-state file: %s",
                g_strerror(errno));
}

static void clear_lock_failures(void) {
  g_autofree char *path = lock_failure_path();

  if (path)
    g_unlink(path);
}

static void locker_ref(WhelkLocker *self) {
  g_atomic_int_inc(&self->refs);
}

static void locker_unref(WhelkLocker *self);
static void queue_session_rebuild(WhelkLocker *self);
static gboolean on_session_rebuild_idle(gpointer data);

static void stop_timer(guint *id) {
  if (*id != 0) {
    g_source_remove(*id);
    *id = 0;
  }
}

static void locker_clear_session_lock(WhelkLocker *self) {
  if (!self->session_lock)
    return;

  /* A pending library signal must not reach a locker that is being freed. */
  g_signal_handlers_disconnect_by_data(self->session_lock, self);
  g_clear_object(&self->session_lock);
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
  stop_timer(&self->protocol_unlock_id);
  stop_timer(&self->session_rebuild_id);

  locker_clear_session_lock(self);

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
  g_clear_pointer(&self->session_views, g_ptr_array_unref);
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
  save_lock_failures(self->failures);

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

/* A restart must not give the next guess a free attempt.  This is called once
 * the primary surface exists and also after a monitor rebuild. */
static void locker_apply_backoff(WhelkLocker *self) {
  if (!self || self->failures <= 0 || self->retry_id != 0 || self->closing ||
      self->unlocked)
    return;

  if (self->entry)
    gtk_widget_set_sensitive(self->entry, FALSE);

  locker_show_error(self, "Wait before trying again.");
  self->retry_id = g_timeout_add(
      whelk_lock_retry_delay_ms((guint)self->failures), on_retry_ready, self);
}

/* End the local lock state.  `request_unlock` is FALSE when the compositor
 * already ended the session lock; asking it again would be redundant. */
static void locker_release(WhelkLocker *self, gboolean request_unlock) {
  if (!self || self->unlocked)
    return;

  self->unlocked = TRUE;
  clear_lock_failures();

  stop_timer(&self->clock_id);
  stop_timer(&self->retry_id);
  stop_timer(&self->protocol_unlock_id);

  if (request_unlock && self->session_lock)
    gtk_session_lock_instance_unlock(self->session_lock);

  destroy_surfaces(self);
  locker_clear_session_lock(self);

  if (locker == self) {
    locker = NULL;
    locker_unref(self); /* the process-wide reference */
  }
}

static void locker_unlock(WhelkLocker *self) {
  locker_release(self, TRUE);
}

static gboolean on_protocol_unlocked_idle(gpointer data) {
  WhelkLocker *self = data;

  self->protocol_unlock_id = 0;
  locker_release(self, FALSE);

  return G_SOURCE_REMOVE;
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
  if (!self || self->closing || self->unlocked || !self->entry ||
      !whelk_lock_submission_allowed(
          self->authenticating, self->retry_id,
          gtk_widget_get_sensitive(self->entry)))
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

static GtkWidget *build_lock_content(WhelkLocker *self, gboolean primary,
                                     GtkWidget **out_clock,
                                     GtkWidget **out_date,
                                     GtkWidget **out_entry,
                                     GtkWidget **out_error) {
  if (out_clock)
    *out_clock = NULL;
  if (out_date)
    *out_date = NULL;
  if (out_entry)
    *out_entry = NULL;
  if (out_error)
    *out_error = NULL;

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

  if (out_clock)
    *out_clock = clock;
  if (out_date)
    *out_date = date;

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

    if (out_entry)
      *out_entry = entry;

    GtkWidget *unlock = gtk_button_new_with_label("Unlock");

    gtk_widget_add_css_class(unlock, "whelk-lock-unlock");
    gtk_widget_set_halign(unlock, GTK_ALIGN_CENTER);
    g_signal_connect(unlock, "clicked", G_CALLBACK(on_unlock_clicked), self);
    gtk_box_append(GTK_BOX(panel), unlock);

    GtkWidget *error = gtk_label_new(NULL);

    gtk_widget_add_css_class(error, "whelk-lock-error");
    gtk_box_append(GTK_BOX(panel), error);
    self->error = error;

    if (out_error)
      *out_error = error;
  }

  return overlay;
}

/* The session-lock library destroys a lock window when the lock ends or when
 * its output disappears.  Detach its widgets from the locker's borrowed lists
 * before GTK frees the children. */
static void lock_view_detach(LockView *view) {
  if (!view || !view->locker)
    return;

  WhelkLocker *self = view->locker;

  if (view->window) {
    g_ptr_array_remove_fast(self->session_views, view);
    view->window = NULL;
  }

  if (view->clock) {
    g_ptr_array_remove_fast(self->clocks, view->clock);
    view->clock = NULL;
  }

  if (view->date) {
    g_ptr_array_remove_fast(self->dates, view->date);
    view->date = NULL;
  }

  if (view->entry && self->entry == view->entry)
    self->entry = NULL;

  if (view->error && self->error == view->error)
    self->error = NULL;

  view->entry = NULL;
  view->error = NULL;

  if (view->primary) {
    self->session_has_primary = FALSE;

    /* The primary output was removed.  The remaining protocol window is a
     * non-primary view with no password field, so rebuild all windows from the
     * current monitor model as soon as the destroy handler returns. */
    queue_session_rebuild(self);
  }
}

static void lock_view_free(gpointer data) {
  LockView *view = data;

  lock_view_detach(view);
  locker_unref(view->locker);
  g_free(view);
}

static void on_lock_window_destroy(GtkWidget *window, gpointer data) {
  (void)window;
  lock_view_detach(data);
}

/* A lock window can be assigned before it is mapped.  Focus the entry when
 * the window actually appears as well as in the idle below: this is what makes
 * a physical keyboard reach the password field on every compositor. */
static void on_lock_window_map(GtkWidget *window, gpointer data) {
  (void)window;

  WhelkLocker *self = data;

  if (!self->closing && !self->unlocked && self->entry)
    gtk_widget_grab_focus(self->entry);
}

static void create_session_lock_window(WhelkLocker *self, GdkMonitor *monitor,
                                       gboolean primary) {
  GtkWidget *clock = NULL;
  GtkWidget *date = NULL;
  GtkWidget *entry = NULL;
  GtkWidget *error = NULL;
  GtkWidget *content = build_lock_content(self, primary, &clock, &date, &entry,
                                          &error);
  GtkWindow *window = GTK_WINDOW(gtk_application_window_new(self->app));

  gtk_window_set_title(window, "whelk lock");
  gtk_window_set_child(window, content);
  gtk_window_set_deletable(window, FALSE);
  g_signal_connect(window, "close-request", G_CALLBACK(on_close_request),
                   self);
  g_signal_connect(window, "map", G_CALLBACK(on_lock_window_map), self);

  LockView *view = g_new0(LockView, 1);

  view->locker = self;
  view->clock = clock;
  view->date = date;
  view->entry = entry;
  view->error = error;
  view->window = GTK_WIDGET(window);
  view->primary = primary;
  locker_ref(self);

  g_ptr_array_add(self->session_views, view);
  if (primary)
    self->session_has_primary = TRUE;

  g_signal_connect(window, "destroy", G_CALLBACK(on_lock_window_destroy), view);
  g_object_set_data_full(G_OBJECT(window), "whelk-lock-view", view,
                         lock_view_free);

  /* The library presents the window and gives the surface its role; calling
   * gtk_window_present() here would realize it too early. */
  gtk_session_lock_instance_assign_window_to_monitor(self->session_lock, window,
                                                     monitor);

  if (primary)
    self->focus_id = g_idle_add(focus_entry, self);

  on_clock_tick(self);

  if (primary)
    locker_apply_backoff(self);
}

static void rebuild_session_lock(WhelkLocker *self) {
  if (!self || !self->session_lock || self->closing || self->unlocked)
    return;

  self->rebuilding_session_lock = TRUE;
  self->session_has_primary = FALSE;
  self->entry = NULL;
  self->error = NULL;

  /* Destroy through refs: the destroy handlers remove the borrowed views from
   * session_views while this loop is running. */
  g_autoptr(GPtrArray) windows =
      g_ptr_array_new_with_free_func((GDestroyNotify)g_object_unref);

  for (guint i = 0; i < self->session_views->len; i++) {
    LockView *view = g_ptr_array_index(self->session_views, i);

    if (view && view->window)
      g_ptr_array_add(windows, g_object_ref(view->window));
  }

  for (guint i = 0; i < windows->len; i++)
    gtk_window_destroy(GTK_WINDOW(g_ptr_array_index(windows, i)));

  GdkDisplay *display = gdk_display_get_default();
  GListModel *monitors = display ? gdk_display_get_monitors(display) : NULL;
  const guint count = monitors ? g_list_model_get_n_items(monitors) : 0;

  for (guint i = 0; i < count; i++) {
    GdkMonitor *monitor = GDK_MONITOR(g_list_model_get_item(monitors, i));

    create_session_lock_window(self, monitor, i == 0);
    g_clear_object(&monitor);
  }

  self->rebuilding_session_lock = FALSE;
}

static gboolean on_session_rebuild_idle(gpointer data) {
  WhelkLocker *self = data;

  self->session_rebuild_id = 0;
  rebuild_session_lock(self);

  return G_SOURCE_REMOVE;
}

static void queue_session_rebuild(WhelkLocker *self) {
  if (!self || self->rebuilding_session_lock || self->closing ||
      self->unlocked || !self->session_lock || self->session_rebuild_id != 0)
    return;

  self->session_rebuild_id = g_idle_add(on_session_rebuild_idle, self);
}

static void on_session_lock_monitor(GtkSessionLockInstance *instance,
                                    GdkMonitor *monitor, gpointer data) {
  (void)instance;

  WhelkLocker *self = data;

  if (!self->session_lock || self->closing || self->unlocked ||
      self->protocol_unlock_id != 0 || self->rebuilding_session_lock)
    return;

  const gboolean primary = !self->session_has_primary;

  if (primary)
    self->session_has_primary = TRUE;

  create_session_lock_window(self, monitor, primary);
}

static void on_session_lock_locked(GtkSessionLockInstance *instance,
                                   gpointer data) {
  (void)instance;

  WhelkLocker *self = data;

  if (!self->closing && !self->unlocked)
    LOG_C_INFO("Locker", "screen locked (ext-session-lock-v1)");
}

static void on_session_lock_failed(GtkSessionLockInstance *instance,
                                   gpointer data) {
  (void)instance;

  WhelkLocker *self = data;

  if (!self->closing && !self->unlocked)
    LOG_C_WARN("Locker",
               "ext-session-lock-v1 refused the lock; the session stays "
               "locked");
}

static void on_session_lock_unlocked(GtkSessionLockInstance *instance,
                                     gpointer data) {
  (void)instance;

  WhelkLocker *self = data;

  if (self->closing || self->unlocked || self->protocol_unlock_id != 0)
    return;

  self->protocol_unlock_id = g_idle_add(on_protocol_unlocked_idle, self);
}

/* Pick the compositor-enforced path whenever the compositor advertises it.
 * A refusal does not fall through to the weaker overlay: another locker may
 * already own the session, and an ordinary layer surface would not be the
 * compositor's lock. */
static gboolean start_session_lock(WhelkLocker *self) {
  if (!gtk_session_lock_is_supported())
    return FALSE;

  self->session_lock = gtk_session_lock_instance_new();
  g_signal_connect(self->session_lock, "monitor",
                   G_CALLBACK(on_session_lock_monitor), self);
  g_signal_connect(self->session_lock, "locked",
                   G_CALLBACK(on_session_lock_locked), self);
  g_signal_connect(self->session_lock, "failed",
                   G_CALLBACK(on_session_lock_failed), self);
  g_signal_connect(self->session_lock, "unlocked",
                   G_CALLBACK(on_session_lock_unlocked), self);

  if (!gtk_session_lock_instance_lock(self->session_lock))
    LOG_C_WARN("Locker",
               "ext-session-lock-v1 could not start; no weaker lock surface "
               "will cover an already-locked session");

  return TRUE;
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

  whelk_surface_set_child(
      surface, build_lock_content(self, primary, NULL, NULL, NULL, NULL));

  GtkWindow *window = whelk_surface_get_window(surface);

  gtk_window_set_deletable(window, FALSE);
  g_signal_connect(window, "close-request", G_CALLBACK(on_close_request),
                   self);
  g_signal_connect(window, "map", G_CALLBACK(on_lock_window_map), self);

  g_ptr_array_add(self->surfaces, surface);
  whelk_surface_present(surface);

  if (primary) {
    self->focus_id = g_idle_add(focus_entry, self);
    locker_apply_backoff(self);
  }
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
  if (!self || self->session_lock || self->closing || self->unlocked ||
      self->rebuild_idle != 0)
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

  WhelkLocker *self = data;

  /* The session-lock library reports new outputs itself and owns the window
   * for each one; only the layer-shell fallback rebuilds from the model. */
  if (self->session_lock)
    return;

  locker_rebuild(self);
}

/* =========================================================================
 * LOGIND
 * ========================================================================= */

static void request_logind_lock(void) {
  if (!watching_logind)
    return;

  WhelkShell *shell = whelk_shell_get_default();

  if (shell)
    whelk_shell_lock(shell);
  else
    whelk_locker_lock();
}

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

  request_logind_lock();
}

/* logind emits PrepareForSleep(true) before suspend; locking here covers lid
 * close and sleep even when no external idle daemon is installed. */
static void on_logind_prepare_for_sleep(GDBusConnection *connection,
                                        const char *sender, const char *path,
                                        const char *interface,
                                        const char *signal,
                                        GVariant *parameters,
                                        gpointer data) {
  (void)connection;
  (void)sender;
  (void)path;
  (void)interface;
  (void)signal;
  (void)data;

  gboolean sleeping = FALSE;

  if (parameters &&
      g_variant_is_of_type(parameters, G_VARIANT_TYPE("(b)"))) {
    g_variant_get(parameters, "(b)", &sleeping);

    if (sleeping)
      request_logind_lock();
  }
}

/* A VT switch makes the session inactive.  Lock before another session can
 * use the seat, so returning to this session already shows the lock screen. */
static void on_session_properties(GDBusConnection *connection,
                                  const char *sender, const char *path,
                                  const char *interface, const char *signal,
                                  GVariant *parameters, gpointer data) {
  (void)connection;
  (void)sender;
  (void)path;
  (void)interface;
  (void)signal;
  (void)data;

  const char *changed_interface = NULL;
  g_autoptr(GVariant) changed = NULL;
  g_autoptr(GVariant) invalidated = NULL;
  gboolean active = TRUE;
  gboolean idle = FALSE;

  if (!parameters ||
      !g_variant_is_of_type(parameters, G_VARIANT_TYPE("(sa{sv}as)")))
    return;

  g_variant_get(parameters, "(&s@a{sv}@as)", &changed_interface, &changed,
                &invalidated);

  if (g_strcmp0(changed_interface, "org.freedesktop.login1.Session") != 0)
    return;

  /* Active=false is the VT switch.  IdleHint is set by a session manager (or
   * an idle daemon) that has decided the seat is idle; locking on it covers
   * idle policies without making whelk poll every input event itself. */
  if (g_variant_lookup(changed, "Active", "b", &active) && !active)
    request_logind_lock();
  else if (g_variant_lookup(changed, "IdleHint", "b", &idle) && idle)
    request_logind_lock();
}

static void on_session_path(GObject *source, GAsyncResult *result,
                            gpointer data) {
  (void)data;

  g_autoptr(GError) error = NULL;
  g_autoptr(GVariant) reply = g_dbus_connection_call_finish(
      G_DBUS_CONNECTION(source), result, &error);

  if (!reply) {
    LOG_C_DEBUG("Locker", "could not find the logind session: %s",
                error ? error->message : "unknown error");
    return;
  }

  const char *path = NULL;

  g_variant_get(reply, "(&o)", &path);

  if (!watching_logind || !system_bus || !path || !*path)
    return;

  g_free(session_path);
  session_path = g_strdup(path);

  session_subscription = g_dbus_connection_signal_subscribe(
      system_bus, "org.freedesktop.login1", "org.freedesktop.login1.Session",
      "Lock", session_path, NULL, G_DBUS_SIGNAL_FLAGS_NONE, on_logind_lock,
      NULL, NULL);
  session_props_subscription = g_dbus_connection_signal_subscribe(
      system_bus, "org.freedesktop.login1", "org.freedesktop.login1.Session",
      "PropertiesChanged", session_path, NULL, G_DBUS_SIGNAL_FLAGS_NONE,
      on_session_properties, NULL, NULL);
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
  sleep_subscription = g_dbus_connection_signal_subscribe(
      bus, "org.freedesktop.login1", "org.freedesktop.login1.Manager",
      "PrepareForSleep", "/org/freedesktop/login1", NULL,
      G_DBUS_SIGNAL_FLAGS_NONE, on_logind_prepare_for_sleep, NULL, NULL);

  /* The manager-level Lock is global; the session object adds the per-session
   * Lock and the Active transition a VT switch produces. */
  g_dbus_connection_call(
      bus, "org.freedesktop.login1", "/org/freedesktop/login1",
      "org.freedesktop.login1.Manager", "GetSessionByPID",
      g_variant_new("(u)", (guint)getpid()), G_VARIANT_TYPE("(o)"),
      G_DBUS_CALL_FLAGS_NONE, -1, NULL, on_session_path, NULL);
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
  self->session_views = g_ptr_array_new();
  self->failures = load_lock_failures();
  locker = self;

  /* Prefer the compositor's own lock.  The layer-shell overlay is only the
   * compatibility path for X11 and compositors without ext-session-lock-v1. */
  const gboolean using_session_lock = start_session_lock(self);

  if (!using_session_lock) {
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
  }

  self->clock_id = g_timeout_add_seconds(1, on_clock_tick, self);

  /* Fill the labels immediately instead of waiting for the first tick. */
  on_clock_tick(self);

  if (using_session_lock)
    LOG_C_INFO("Locker", "screen lock requested (ext-session-lock-v1)");
  else
    LOG_C_INFO("Locker", "screen locked (layer-shell fallback)");
}

void whelk_locker_shutdown(void) {
  watching_logind = FALSE;

  if (system_bus) {
    if (lock_subscription != 0)
      g_dbus_connection_signal_unsubscribe(system_bus, lock_subscription);
    if (sleep_subscription != 0)
      g_dbus_connection_signal_unsubscribe(system_bus, sleep_subscription);
    if (session_subscription != 0)
      g_dbus_connection_signal_unsubscribe(system_bus, session_subscription);
    if (session_props_subscription != 0)
      g_dbus_connection_signal_unsubscribe(system_bus, session_props_subscription);
  }

  lock_subscription = 0;
  sleep_subscription = 0;
  session_subscription = 0;
  session_props_subscription = 0;
  g_clear_pointer(&session_path, g_free);
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
    stop_timer(&self->protocol_unlock_id);

    /* Do not call gtk_session_lock_instance_unlock() here.  SIGTERM, SIGINT
     * and SIGHUP all reach this shutdown path; only PAM success (or the
     * compositor ending the lock) may release the protocol lock.  The
     * compositor keeps the session locked when this process exits. */
    destroy_surfaces(self);
    locker_clear_session_lock(self);
    locker_unref(self);
  }

  lock_app = NULL;
}
