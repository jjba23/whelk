// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "notification_center.h"

#include <pango/pango.h>

#include "logger.h"
#include "notifications.h"
#include "style.h"
#include "surface.h"

/* =========================================================================
 * THE CENTRE
 * =========================================================================
 * Notifications are grouped by the application that sent them, because that is
 * the only grouping a notification actually has: five messages from a chat app
 * are one conversation, and three from a build are one build.  Within a group
 * the newest is on top; the count says how many are behind it.
 *
 * The toast stack holds the three most recent.  It never scrolls and never
 * grows: a notification that is not read from the corner is not lost, it is in
 * the centre.
 */

#define WHELK_PANEL_WIDTH 380
#define WHELK_TOAST_WIDTH 380
/* mako's max-visible, which the reference bar's configuration sets to 5. */
#define WHELK_TOAST_LIMIT 5

typedef struct {
  GtkWidget *cards; /* the group's card column */
  GtkWidget *count; /* the "×3" label */
  guint n;
} Group;

struct _WhelkNotificationCenter {
  GObject parent_instance;

  GtkApplication *app;
  WhelkNotifications *notifications;
  WhelkBarConfig *style;

  WhelkSurface *panel;
  GtkWidget *groups;
  GtkToggleButton *dnd_button;
  GtkWidget *summary_label;

  WhelkSurface *toast;
  GtkWidget *toast_box;
  GArray *toast_ids;         /* guint, newest first */
  GHashTable *toast_timers;  /* id -> GSource id */

  GtkCssProvider *panel_skin;
  GtkCssProvider *toast_skin;

  gboolean updating;

  gulong changed_id;
  guint posted_id;
  guint closed_id;
};

G_DEFINE_TYPE(WhelkNotificationCenter, whelk_notification_center, G_TYPE_OBJECT)

static void on_dnd_toggled(GtkToggleButton *button, gpointer user_data);

/* A closure's data dies with the widget that made it. */
static void free_closure_data(gpointer data, GClosure *closure) {
  (void)closure;

  g_free(data);
}

/* =========================================================================
 * SMALL HELPERS
 * ========================================================================= */

static void clear_box(GtkWidget *box) {
  GtkWidget *child = NULL;

  while ((child = gtk_widget_get_first_child(box)) != NULL)
    gtk_box_remove(GTK_BOX(box), child);
}

/* A notification body may carry markup -- the capability list advertises
 * body-markup -- but only when it parses.  A body that does not is text, and
 * showing it as text is better than showing nothing. */
static void set_body(GtkLabel *label, const char *body) {
  PangoAttrList *attributes = NULL;
  char *plain = NULL;
  GError *error = NULL;

  if (body && *body &&
      pango_parse_markup(body, -1, 0, &attributes, &plain, NULL, &error)) {
    gtk_label_set_markup(label, body);
  } else {
    g_clear_error(&error);
    gtk_label_set_text(label, body ? body : "");
  }

  g_clear_pointer(&attributes, pango_attr_list_unref);
  g_free(plain);
}

static GtkWidget *panel_button(const char *icon, const char *tooltip) {
  GtkWidget *button = gtk_button_new_from_icon_name(icon);

  gtk_widget_add_css_class(button, "whelk-panel-button");
  gtk_widget_set_tooltip_text(button, tooltip);

  return button;
}

static GtkWidget *panel_toggle_button(const char *icon, const char *tooltip) {
  GtkWidget *button = gtk_toggle_button_new();

  gtk_button_set_child(GTK_BUTTON(button),
                       gtk_image_new_from_icon_name(icon));
  gtk_widget_add_css_class(button, "whelk-panel-button");
  gtk_widget_set_tooltip_text(button, tooltip);

  return button;
}

static WhelkNotification *by_id(WhelkNotificationCenter *self, guint id) {
  GPtrArray *snapshot = whelk_notifications_snapshot(self->notifications);

  if (!snapshot)
    return NULL;

  for (guint i = 0; i < snapshot->len; i++) {
    WhelkNotification *notification = g_ptr_array_index(snapshot, i);

    if (whelk_notification_id(notification) == id)
      return notification;
  }

  return NULL;
}

/* =========================================================================
 * CARDS
 * ========================================================================= */

typedef struct {
  WhelkNotificationCenter *self;
  guint id;
  gboolean toast;
} CardClick;

static void card_click_free(CardClick *click) { g_free(click); }

static void invoke_default(WhelkNotificationCenter *self,
                           WhelkNotification *notification) {
  const guint actions = whelk_notification_action_count(notification);

  for (guint i = 0; i < actions; i++) {
    if (g_strcmp0(whelk_notification_action_id(notification, i), "default") ==
        0) {
      whelk_notifications_invoke(self->notifications, notification, "default");
      return;
    }
  }

  /* No default action: the card's click means "show me this", which is the
   * centre. */
  whelk_notification_center_show(self);
}

static void on_card_clicked(GtkGestureClick *gesture, int n_press, double x,
                            double y, gpointer user_data) {
  (void)gesture;
  (void)n_press;
  (void)x;
  (void)y;

  CardClick *click = user_data;
  WhelkNotification *notification = by_id(click->self, click->id);

  if (!notification)
    return;

  if (click->toast) {
    whelk_notification_center_show(click->self);
    return;
  }

  invoke_default(click->self, notification);
}

static void attach_card_click(WhelkNotificationCenter *self, GtkWidget *card,
                              guint id, gboolean toast) {
  GtkGesture *gesture = gtk_gesture_click_new();

  CardClick *click = g_new0(CardClick, 1);

  click->self = self;
  click->id = id;
  click->toast = toast;

  gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture),
                                GDK_BUTTON_PRIMARY);
  g_signal_connect(gesture, "pressed", G_CALLBACK(on_card_clicked), click);

  /* The click data lives exactly as long as the gesture that uses it. */
  g_object_set_data_full(G_OBJECT(gesture), "whelk-card-click", click,
                         (GDestroyNotify)card_click_free);

  gtk_widget_add_controller(card, GTK_EVENT_CONTROLLER(gesture));
  gtk_widget_set_cursor_from_name(card, "pointer");
}

typedef struct {
  WhelkNotificationCenter *self;
  guint id;
} CloseClick;

static void on_close_clicked(GtkButton *button, gpointer user_data) {
  (void)button;

  CloseClick *close = user_data;

  whelk_notifications_close(close->self->notifications, close->id);
}

static void on_action_clicked(GtkButton *button, gpointer user_data) {
  (void)button;

  CloseClick *close = user_data;
  WhelkNotification *notification = by_id(close->self, close->id);
  const char *action = g_object_get_data(G_OBJECT(button), "whelk-action-id");

  if (!notification)
    return;

  whelk_notifications_invoke(close->self->notifications, notification, action);
}

static GtkWidget *build_card(WhelkNotificationCenter *self,
                             WhelkNotification *notification,
                             gboolean toast) {
  const guint id = whelk_notification_id(notification);

  GtkWidget *card = gtk_box_new(GTK_ORIENTATION_VERTICAL, 4);
  GtkWidget *head = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
  GtkWidget *summary = gtk_label_new(NULL);
  GtkWidget *body = gtk_label_new(NULL);
  GtkWidget *footer = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
  GtkWidget *close = panel_button("window-close-symbolic", "Dismiss");

  gtk_widget_add_css_class(card, "whelk-notification");
  gtk_widget_add_css_class(card, "whelk-card");

  if (whelk_notification_urgency(notification) >= 2)
    gtk_widget_add_css_class(card, "critical");

  gtk_label_set_markup(GTK_LABEL(summary),
                       g_markup_escape_text(whelk_notification_summary(
                                                notification),
                                            -1));
  gtk_widget_add_css_class(summary, "whelk-notification-summary");
  gtk_label_set_xalign(GTK_LABEL(summary), 0.0);
  gtk_label_set_ellipsize(GTK_LABEL(summary), PANGO_ELLIPSIZE_END);
  gtk_widget_set_hexpand(summary, TRUE);

  /* The application's own mark, the way a notification daemon draws one: the
   * image the notification carried, else its icon name, else its desktop
   * entry's icon.  A notification without one keeps its text and gains no
   * empty square. */
  const char *icon = whelk_notification_icon(notification);

  if (icon && *icon) {
    GtkWidget *image = g_path_is_absolute(icon)
                           ? gtk_image_new_from_file(icon)
                           : gtk_image_new_from_icon_name(icon);

    gtk_image_set_pixel_size(GTK_IMAGE(image), toast ? 32 : 36);
    gtk_widget_set_valign(image, GTK_ALIGN_START);
    gtk_widget_add_css_class(image, "whelk-notification-icon");
    gtk_box_append(GTK_BOX(head), image);
  }

  gtk_box_append(GTK_BOX(head), summary);
  gtk_box_append(GTK_BOX(head), close);
  gtk_box_append(GTK_BOX(card), head);

  const char *text = whelk_notification_body(notification);

  if (text && *text) {
    set_body(GTK_LABEL(body), text);
    gtk_widget_add_css_class(body, "whelk-notification-body");
    gtk_label_set_xalign(GTK_LABEL(body), 0.0);
    gtk_label_set_wrap(GTK_LABEL(body), TRUE);
    gtk_label_set_wrap_mode(GTK_LABEL(body), PANGO_WRAP_WORD_CHAR);
    gtk_label_set_lines(GTK_LABEL(body), toast ? 2 : 4);
    gtk_label_set_ellipsize(GTK_LABEL(body), PANGO_ELLIPSIZE_END);
    gtk_box_append(GTK_BOX(card), body);
  }

  const guint actions = whelk_notification_action_count(notification);

  if (!toast) {
    g_autofree char *time = whelk_notification_time(notification);
    GtkWidget *stamp = gtk_label_new(time);

    gtk_widget_add_css_class(stamp, "whelk-notification-time");
    gtk_label_set_xalign(GTK_LABEL(stamp), 0.0);
    gtk_widget_set_hexpand(stamp, TRUE);
    gtk_box_append(GTK_BOX(footer), stamp);
  }

  /* A toast carries its actions too: a Reply button that only exists after
   * opening the centre is a button that arrives too late.  Two at most, because
   * a bubble is not a dialog. */
  for (guint i = 0; i < actions && (!toast || i < 2); i++) {
    const char *label = whelk_notification_action_label(notification, i);
    const char *action = whelk_notification_action_id(notification, i);

    if (!label || *label == '\0')
      continue;

    GtkWidget *button = gtk_button_new_with_label(label);

    gtk_widget_add_css_class(button, "whelk-notification-action");
    g_object_set_data_full(G_OBJECT(button), "whelk-action-id",
                           g_strdup(action), g_free);
    g_signal_connect(button, "clicked", G_CALLBACK(on_action_clicked),
                     close);
    gtk_box_append(GTK_BOX(footer), button);
  }

  if (gtk_widget_get_first_child(footer))
    gtk_box_append(GTK_BOX(card), footer);

  attach_card_click(self, card, id, toast);

  CloseClick *close_data = g_new0(CloseClick, 1);

  close_data->self = self;
  close_data->id = id;
  g_signal_connect_data(close, "clicked", G_CALLBACK(on_close_clicked),
                        close_data, free_closure_data, 0);

  return card;
}

/* =========================================================================
 * THE PANEL
 * ========================================================================= */

static void rebuild_panel(WhelkNotificationCenter *self) {
  if (!self->groups)
    return;

  clear_box(self->groups);

  GHashTable *by_app = g_hash_table_new_full(g_str_hash, g_str_equal, g_free,
                                             NULL);
  GPtrArray *order =
      g_ptr_array_new_with_free_func((GDestroyNotify)g_free);

  GPtrArray *snapshot = whelk_notifications_snapshot(self->notifications);

  for (guint i = 0; snapshot && i < snapshot->len; i++) {
    WhelkNotification *notification = g_ptr_array_index(snapshot, i);
    const char *app = whelk_notification_app(notification);

    if (!app || *app == '\0')
      app = "Notifications";

    Group *group = g_hash_table_lookup(by_app, app);

    if (!group) {
      group = g_new0(Group, 1);

      GtkWidget *container = gtk_box_new(GTK_ORIENTATION_VERTICAL, 4);
      GtkWidget *header = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
      GtkWidget *title = gtk_label_new(NULL);

      /* The app's own mark, so a column of groups reads like a list of apps
       * rather than a column of names. */
      const char *app_icon = whelk_notification_icon(notification);

      if (app_icon && *app_icon) {
        GtkWidget *mark = g_path_is_absolute(app_icon)
                              ? gtk_image_new_from_file(app_icon)
                              : gtk_image_new_from_icon_name(app_icon);

        gtk_image_set_pixel_size(GTK_IMAGE(mark), 18);
        gtk_widget_add_css_class(mark, "whelk-group-icon");
        gtk_box_append(GTK_BOX(header), mark);
      }

      gtk_label_set_markup(GTK_LABEL(title),
                           g_markup_escape_text(app, -1));
      gtk_widget_add_css_class(title, "whelk-group-title");
      gtk_label_set_xalign(GTK_LABEL(title), 0.0);
      gtk_widget_set_hexpand(title, TRUE);

      group->count = gtk_label_new(NULL);
      gtk_widget_add_css_class(group->count, "whelk-group-count");

      gtk_box_append(GTK_BOX(header), title);
      gtk_box_append(GTK_BOX(header), group->count);

      group->cards = gtk_box_new(GTK_ORIENTATION_VERTICAL, 6);

      gtk_box_append(GTK_BOX(container), header);
      gtk_box_append(GTK_BOX(container), group->cards);
      gtk_box_append(GTK_BOX(self->groups), container);

      g_hash_table_insert(by_app, g_strdup(app), group);
      g_ptr_array_add(order, group);
    }

    gtk_box_prepend(GTK_BOX(group->cards),
                    build_card(self, notification, FALSE));
    group->n++;
  }

  for (guint i = 0; i < order->len; i++) {
    Group *group = g_ptr_array_index(order, i);
    g_autofree char *count = g_strdup_printf("%u", group->n);

    gtk_label_set_text(GTK_LABEL(group->count), count);
  }

  g_hash_table_destroy(by_app);
  g_ptr_array_unref(order);

  const guint total = whelk_notifications_count(self->notifications);

  /* The empty state is part of the rebuilt list, not a widget kept across
   * rebuilds: clearing the box takes its children with it. */
  if (total == 0) {
    GtkWidget *empty = gtk_box_new(GTK_ORIENTATION_VERTICAL, 6);
    GtkWidget *mark = gtk_image_new_from_icon_name("notifications-symbolic");
    GtkWidget *label = gtk_label_new("You are all caught up.");
    GtkWidget *hint =
        gtk_label_new("Notifications show up here, newest first.");

    gtk_widget_add_css_class(empty, "whelk-panel-empty");
    gtk_image_set_pixel_size(GTK_IMAGE(mark), 44);
    gtk_widget_add_css_class(mark, "whelk-panel-empty-icon");
    gtk_widget_add_css_class(label, "whelk-panel-empty-title");
    gtk_widget_add_css_class(hint, "whelk-panel-empty-hint");
    gtk_widget_set_halign(mark, GTK_ALIGN_CENTER);
    gtk_widget_set_halign(label, GTK_ALIGN_CENTER);
    gtk_widget_set_halign(hint, GTK_ALIGN_CENTER);
    gtk_label_set_wrap(GTK_LABEL(hint), TRUE);
    gtk_label_set_justify(GTK_LABEL(hint), GTK_JUSTIFY_CENTER);
    gtk_box_append(GTK_BOX(empty), mark);
    gtk_box_append(GTK_BOX(empty), label);
    gtk_box_append(GTK_BOX(empty), hint);
    gtk_box_append(GTK_BOX(self->groups), empty);
  }

  if (self->summary_label) {
    g_autofree char *summary =
        total == 0 ? g_strdup("Nothing new")
                   : g_strdup_printf("%u notification%s", total,
                                     total == 1 ? "" : "s");

    gtk_label_set_text(GTK_LABEL(self->summary_label), summary);
  }
}

/* =========================================================================
 * THE TOASTS
 * ========================================================================= */

static void cancel_timer(WhelkNotificationCenter *self, guint id) {
  gpointer value = g_hash_table_lookup(self->toast_timers,
                                       GUINT_TO_POINTER(id));

  if (!value)
    return;

  const guint source = GPOINTER_TO_UINT(value);

  if (source != 0)
    g_source_remove(source);

  g_hash_table_remove(self->toast_timers, GUINT_TO_POINTER(id));
}

static void rebuild_toasts(WhelkNotificationCenter *self) {
  if (!self->toast_box)
    return;

  clear_box(self->toast_box);

  for (guint i = 0; i < self->toast_ids->len;) {
    const guint id = g_array_index(self->toast_ids, guint, i);
    WhelkNotification *notification = by_id(self, id);

    if (!notification) {
      g_array_remove_index(self->toast_ids, i);
      continue;
    }

    gtk_box_append(GTK_BOX(self->toast_box),
                   build_card(self, notification, TRUE));
    i++;
  }

  if (self->toast) {
    GtkWindow *window = whelk_surface_get_window(self->toast);

    if (window)
      gtk_widget_set_visible(GTK_WIDGET(window), self->toast_ids->len > 0);
  }
}

/* The timer needs both the centre and the id, and must find the id still
 * present when it fires: the notification may have been dismissed meanwhile. */
typedef struct {
  WhelkNotificationCenter *self;
  guint id;
} ToastTimer;

static gboolean on_toast_timeout(gpointer user_data) {
  ToastTimer *timer = user_data;
  WhelkNotificationCenter *self = timer->self;

  for (guint i = 0; i < self->toast_ids->len; i++) {
    if (g_array_index(self->toast_ids, guint, i) == timer->id) {
      g_array_remove_index(self->toast_ids, i);
      break;
    }
  }

  g_hash_table_remove(self->toast_timers, GUINT_TO_POINTER(timer->id));

  /* A transient notification is gone once its toast is: the application said
   * it was about the moment, not about the list. */
  WhelkNotification *notification = by_id(self, timer->id);

  if (notification && whelk_notification_is_transient(notification))
    whelk_notifications_expire(self->notifications, timer->id);

  rebuild_toasts(self);

  g_free(timer);
  return G_SOURCE_REMOVE;
}

static void schedule_expiry(WhelkNotificationCenter *self,
                            WhelkNotification *notification) {
  const guint id = whelk_notification_id(notification);
  const int timeout = whelk_notification_timeout(notification);

  cancel_timer(self, id);

  if (timeout <= 0)
    return;

  ToastTimer *timer = g_new0(ToastTimer, 1);

  timer->self = self;
  timer->id = id;

  const guint source = g_timeout_add((guint)timeout, on_toast_timeout, timer);

  g_hash_table_insert(self->toast_timers, GUINT_TO_POINTER(id),
                      GUINT_TO_POINTER(source));
}

static void clear_toasts(WhelkNotificationCenter *self) {
  for (guint i = 0; i < self->toast_ids->len; i++)
    cancel_timer(self, g_array_index(self->toast_ids, guint, i));

  g_array_set_size(self->toast_ids, 0);
  rebuild_toasts(self);
}

static void on_posted(WhelkNotifications *notifications,
                      WhelkNotification *notification, gpointer user_data) {
  (void)notifications;

  WhelkNotificationCenter *self = user_data;
  const guint id = whelk_notification_id(notification);

  /* The centre is already showing this notification, grouped and in place: a
   * toast on top of it would be the same notification twice, in one corner. */
  if (whelk_notification_center_is_visible(self)) {
    rebuild_panel(self);
    return;
  }

  /* Replacing an id that is already up: move it to the front rather than show
   * it twice. */
  for (guint i = 0; i < self->toast_ids->len; i++) {
    if (g_array_index(self->toast_ids, guint, i) == id) {
      g_array_remove_index(self->toast_ids, i);
      break;
    }
  }

  g_array_prepend_val(self->toast_ids, id);

  while (self->toast_ids->len > WHELK_TOAST_LIMIT) {
    const guint dropped =
        g_array_index(self->toast_ids, guint, self->toast_ids->len - 1);

    cancel_timer(self, dropped);
    g_array_remove_index(self->toast_ids, self->toast_ids->len - 1);
  }

  rebuild_toasts(self);
  schedule_expiry(self, notification);
}

static void on_closed(WhelkNotifications *notifications, guint id,
                      gpointer user_data) {
  (void)notifications;

  WhelkNotificationCenter *self = user_data;

  cancel_timer(self, id);

  for (guint i = 0; i < self->toast_ids->len; i++) {
    if (g_array_index(self->toast_ids, guint, i) == id) {
      g_array_remove_index(self->toast_ids, i);
      break;
    }
  }

  rebuild_toasts(self);
}

static void on_changed(WhelkNotifications *notifications, gpointer user_data) {
  WhelkNotificationCenter *self = user_data;

  rebuild_panel(self);
  rebuild_toasts(self);

  const gboolean dnd = whelk_notifications_has_dnd(notifications);

  if (self->dnd_button) {
    self->updating = TRUE;
    gtk_toggle_button_set_active(self->dnd_button, dnd);
    self->updating = FALSE;
  }
}

/* =========================================================================
 * ACTIONS
 * ========================================================================= */

static void on_dnd_toggled(GtkToggleButton *button, gpointer user_data) {
  WhelkNotificationCenter *self = user_data;

  /* Redrawing the button from the store must not look like a click on it. */
  if (self->updating)
    return;

  whelk_notifications_set_dnd(self->notifications,
                              gtk_toggle_button_get_active(button));
}

static void on_clear_clicked(GtkButton *button, gpointer user_data) {
  (void)button;

  WhelkNotificationCenter *self = user_data;

  whelk_notifications_close_all(self->notifications);
}

static void on_close_panel_clicked(GtkButton *button, gpointer user_data) {
  (void)button;

  whelk_notification_center_hide(user_data);
}

/* =========================================================================
 * BUILDING
 * ========================================================================= */

static void build_panel(WhelkNotificationCenter *self) {
  GtkWidget *panel = gtk_box_new(GTK_ORIENTATION_VERTICAL, 8);
  GtkWidget *header = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
  GtkWidget *title = gtk_label_new("Notifications");
  GtkWidget *scroller = gtk_scrolled_window_new();

  gtk_widget_add_css_class(panel, "whelk-panel");
  gtk_widget_set_size_request(panel, WHELK_PANEL_WIDTH, -1);
  gtk_widget_set_margin_start(panel, 12);
  gtk_widget_set_margin_end(panel, 12);
  gtk_widget_set_margin_top(panel, 12);
  gtk_widget_set_margin_bottom(panel, 12);

  gtk_widget_add_css_class(title, "whelk-panel-title");
  gtk_label_set_xalign(GTK_LABEL(title), 0.0);

  /* The count used to be written to a label that was never built.  It is the
   * panel's second line now: "Notifications" says what this is, the line under
   * it says how much of it there is. */
  GtkWidget *heading = gtk_box_new(GTK_ORIENTATION_VERTICAL, 1);

  self->summary_label = gtk_label_new(NULL);
  gtk_widget_add_css_class(self->summary_label, "whelk-panel-subtitle");
  gtk_label_set_xalign(GTK_LABEL(self->summary_label), 0.0);
  gtk_widget_set_hexpand(heading, TRUE);
  gtk_box_append(GTK_BOX(heading), title);
  gtk_box_append(GTK_BOX(heading), self->summary_label);

  self->dnd_button = GTK_TOGGLE_BUTTON(panel_toggle_button(
      "notifications-disabled-symbolic", "Do not disturb"));

  gtk_widget_add_css_class(GTK_WIDGET(self->dnd_button), "whelk-dnd");

  GtkWidget *clear = panel_button("user-trash-symbolic", "Clear all");
  GtkWidget *close = panel_button("window-close-symbolic", "Close");

  g_signal_connect(self->dnd_button, "toggled", G_CALLBACK(on_dnd_toggled),
                   self);
  g_signal_connect(clear, "clicked", G_CALLBACK(on_clear_clicked), self);
  g_signal_connect(close, "clicked", G_CALLBACK(on_close_panel_clicked), self);

  gtk_box_append(GTK_BOX(header), heading);
  gtk_box_append(GTK_BOX(header), GTK_WIDGET(self->dnd_button));
  gtk_box_append(GTK_BOX(header), clear);
  gtk_box_append(GTK_BOX(header), close);

  self->groups = gtk_box_new(GTK_ORIENTATION_VERTICAL, 14);
  gtk_widget_add_css_class(self->groups, "whelk-notification-groups");

  gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scroller), self->groups);
  gtk_widget_set_vexpand(scroller, TRUE);
  gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scroller),
                                 GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);

  gtk_box_append(GTK_BOX(panel), header);
  gtk_box_append(GTK_BOX(panel), scroller);

  self->panel = whelk_surface_new(self->app, "notifications");

  whelk_surface_set_edge(self->panel, GTK_LAYER_SHELL_EDGE_RIGHT);
  whelk_surface_set_layer(self->panel, GTK_LAYER_SHELL_LAYER_OVERLAY);
  whelk_surface_set_exclusive(self->panel, FALSE);
  whelk_surface_set_margins(self->panel, 8, 8, 8, 8);
  whelk_surface_set_namespace(self->panel, "whelk-notifications");

  g_autofree char *widget_id = whelk_style_widget_id("notifications");

  whelk_surface_set_widget_id(self->panel, widget_id);
  self->panel_skin = whelk_style_apply(widget_id, self->style, "whelk-panel",
                                       self->style->css);
  whelk_surface_set_child(self->panel, panel);
  whelk_surface_present(self->panel);

  GtkWindow *window = whelk_surface_get_window(self->panel);

  if (window)
    gtk_widget_set_visible(GTK_WIDGET(window), FALSE);
}

static void update_toast_margins(WhelkNotificationCenter *self) {
  if (!self->toast)
    return;

  whelk_surface_set_margins(
      self->toast,
      self->style && self->style->edge == GTK_LAYER_SHELL_EDGE_TOP
          ? self->style->height + 8
          : 8,
      8, 8, 8);
}

static void build_toasts(WhelkNotificationCenter *self) {
  GtkWidget *stack = gtk_box_new(GTK_ORIENTATION_VERTICAL, 8);

  gtk_widget_add_css_class(stack, "whelk-toast-stack");
  gtk_widget_set_size_request(stack, WHELK_TOAST_WIDTH, -1);

  self->toast_box = stack;

  self->toast = whelk_surface_new(self->app, "toasts");

  whelk_surface_set_edge(self->toast, GTK_LAYER_SHELL_EDGE_TOP);
  whelk_surface_set_layer(self->toast, GTK_LAYER_SHELL_LAYER_OVERLAY);
  whelk_surface_set_exclusive(self->toast, FALSE);
  /* Clear the bar when it is on the top edge; on any other edge the toasts
   * only need the screen's own margin. */
  update_toast_margins(self);
  whelk_surface_set_anchors(self->toast, TRUE, TRUE, FALSE, FALSE);
  whelk_surface_set_namespace(self->toast, "whelk-toasts");

  g_autofree char *widget_id = whelk_style_widget_id("toasts");

  whelk_surface_set_widget_id(self->toast, widget_id);
  /* The surface's material is the stack box ("whelk-toast-stack"): naming a
   * class no widget wears would leave the toasts unstyled. */
  self->toast_skin =
      whelk_style_apply(widget_id, self->style, "whelk-toast-stack", NULL);
  whelk_surface_set_child(self->toast, stack);
  whelk_surface_present(self->toast);

  GtkWindow *window = whelk_surface_get_window(self->toast);

  if (window)
    gtk_widget_set_visible(GTK_WIDGET(window), FALSE);
}

/* =========================================================================
 * PUBLIC API
 * ========================================================================= */

WhelkNotificationCenter *
whelk_notification_center_new(GtkApplication *app,
                              WhelkNotifications *notifications,
                              const WhelkBarConfig *style) {
  WhelkNotificationCenter *self =
      g_object_new(WHELK_TYPE_NOTIFICATION_CENTER, NULL);

  self->app = app;
  self->notifications = g_object_ref(notifications);
  self->style = style ? whelk_bar_config_copy(style) : whelk_bar_config_new("default");
  self->toast_ids = g_array_new(FALSE, FALSE, sizeof(guint));
  self->toast_timers =
      g_hash_table_new_full(g_direct_hash, g_direct_equal, NULL, NULL);

  build_panel(self);
  build_toasts(self);

  self->changed_id =
      g_signal_connect(notifications, "changed", G_CALLBACK(on_changed), self);
  self->posted_id =
      g_signal_connect(notifications, "posted", G_CALLBACK(on_posted), self);
  self->closed_id =
      g_signal_connect(notifications, "closed", G_CALLBACK(on_closed), self);

  rebuild_panel(self);

  return self;
}

void whelk_notification_center_set_style(WhelkNotificationCenter *self,
                                         const WhelkBarConfig *style) {
  if (!self || !style)
    return;

  GdkDisplay *display = gdk_display_get_default();

  if (self->panel_skin)
    whelk_style_detach(self->panel_skin, display);
  if (self->toast_skin)
    whelk_style_detach(self->toast_skin, display);

  self->panel_skin = NULL;
  self->toast_skin = NULL;

  self->panel_skin = NULL;
  self->toast_skin = NULL;

  g_clear_pointer(&self->style, whelk_bar_config_free);
  self->style = whelk_bar_config_copy(style);

  g_autofree char *panel_id = whelk_style_widget_id("notifications");

  self->panel_skin = whelk_style_apply(panel_id, self->style, "whelk-panel",
                                       self->style->css);

  g_autofree char *toast_id = whelk_style_widget_id("toasts");

  self->toast_skin =
      whelk_style_apply(toast_id, self->style, "whelk-toast-stack", NULL);

  update_toast_margins(self);

  GtkWindow *panel_window = whelk_surface_get_window(self->panel);
  GtkWindow *toast_window = whelk_surface_get_window(self->toast);

  if (panel_window)
    gtk_widget_queue_draw(GTK_WIDGET(panel_window));
  if (toast_window)
    gtk_widget_queue_draw(GTK_WIDGET(toast_window));
}

void whelk_notification_center_show(WhelkNotificationCenter *self) {
  if (!self || !self->panel)
    return;

  LOG_C_DEBUG("Notifications", "centre shown (%u stored)",
              whelk_notifications_count(self->notifications));

  /* The centre is the fuller view of the same notifications: the corner goes
   * quiet while it is open. */
  clear_toasts(self);
  rebuild_panel(self);

  GtkWindow *window = whelk_surface_get_window(self->panel);

  if (window)
    gtk_widget_set_visible(GTK_WIDGET(window), TRUE);
}

void whelk_notification_center_hide(WhelkNotificationCenter *self) {
  if (!self || !self->panel)
    return;

  GtkWindow *window = whelk_surface_get_window(self->panel);

  if (window)
    gtk_widget_set_visible(GTK_WIDGET(window), FALSE);
}

gboolean whelk_notification_center_is_visible(WhelkNotificationCenter *self) {
  if (!self || !self->panel)
    return FALSE;

  GtkWindow *window = whelk_surface_get_window(self->panel);

  return window && gtk_widget_get_visible(GTK_WIDGET(window));
}

void whelk_notification_center_toggle(WhelkNotificationCenter *self) {
  if (!self)
    return;

  if (whelk_notification_center_is_visible(self))
    whelk_notification_center_hide(self);
  else
    whelk_notification_center_show(self);
}

/* =========================================================================
 * LIFECYCLE
 * ========================================================================= */

static void whelk_notification_center_dispose(GObject *object) {
  WhelkNotificationCenter *self = WHELK_NOTIFICATION_CENTER(object);

  if (self->notifications) {
    g_signal_handler_disconnect(self->notifications, self->changed_id);
    g_signal_handler_disconnect(self->notifications, self->posted_id);
    g_signal_handler_disconnect(self->notifications, self->closed_id);
    g_clear_object(&self->notifications);
  }

  if (self->toast_timers) {
    GHashTableIter iter;
    gpointer value;

    g_hash_table_iter_init(&iter, self->toast_timers);
    while (g_hash_table_iter_next(&iter, NULL, &value)) {
      const guint source = GPOINTER_TO_UINT(value);

      if (source != 0)
        g_source_remove(source);
    }

    g_clear_pointer(&self->toast_timers, g_hash_table_unref);
  }

  g_clear_pointer(&self->toast_ids, g_array_unref);

  GdkDisplay *display = gdk_display_get_default();

  if (self->panel_skin)
    whelk_style_detach(self->panel_skin, display);
  if (self->toast_skin)
    whelk_style_detach(self->toast_skin, display);

  g_clear_object(&self->panel);
  g_clear_object(&self->toast);
  g_clear_pointer(&self->style, whelk_bar_config_free);

  G_OBJECT_CLASS(whelk_notification_center_parent_class)->dispose(object);
}

static void
whelk_notification_center_class_init(WhelkNotificationCenterClass *klass) {
  GObjectClass *object_class = G_OBJECT_CLASS(klass);

  object_class->dispose = whelk_notification_center_dispose;
}

static void whelk_notification_center_init(WhelkNotificationCenter *self) {
  (void)self;
}
