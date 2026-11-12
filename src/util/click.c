// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/click.h"

#include "util/command.h"

/* The command a gesture carries, owned by the closure that reads it. */
typedef struct {
  char *command;
} ClickAction;

/* The two directions of a scroll wheel, owned by the scroll closure. */
typedef struct {
  char *up;
  char *down;
} ScrollAction;

/* GClosureNotify: the signature the closure wants, so no cast is needed. */
static void click_action_free(gpointer data, GClosure *closure) {
  ClickAction *action = data;

  (void)closure;

  if (!action)
    return;

  g_free(action->command);
  g_free(action);
}

static void scroll_action_free(gpointer data, GClosure *closure) {
  ScrollAction *action = data;

  (void)closure;

  if (!action)
    return;

  g_free(action->up);
  g_free(action->down);
  g_free(action);
}

static void on_click_pressed(GtkGestureClick *gesture, int n_press, double x,
                             double y, gpointer user_data) {
  (void)gesture;
  (void)n_press;
  (void)x;
  (void)y;

  ClickAction *action = user_data;

  whelk_command_spawn(action->command);
}

static gboolean on_scroll(GtkEventControllerScroll *controller, double dx,
                          double dy, gpointer user_data) {
  (void)controller;
  (void)dx;

  ScrollAction *action = user_data;

  if (dy < 0.0 && action->up) {
    whelk_command_spawn(action->up);
    return TRUE;
  }

  if (dy > 0.0 && action->down) {
    whelk_command_spawn(action->down);
    return TRUE;
  }

  return FALSE;
}

/* Installs one click gesture for @button running @command (when configured). */
static void attach_button(GtkWidget *widget, const WhelkModuleSpec *spec,
                          const char *option, guint button) {
  const char *command = whelk_spec_str(spec, option, NULL);

  if (!command || *command == '\0')
    return;

  ClickAction *action = g_new0(ClickAction, 1);
  action->command = g_strdup(command);

  GtkGesture *gesture = gtk_gesture_click_new();
  gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture), button);
  g_signal_connect_data(gesture, "pressed", G_CALLBACK(on_click_pressed), action,
                        click_action_free, 0);
  gtk_widget_add_controller(widget, GTK_EVENT_CONTROLLER(gesture));
}

/* Installs the scroll controller when either direction is configured. */
static void attach_scroll(GtkWidget *widget, const WhelkModuleSpec *spec) {
  const char *up = whelk_spec_str(spec, "on-scroll-up", NULL);
  const char *down = whelk_spec_str(spec, "on-scroll-down", NULL);

  if ((!up || *up == '\0') && (!down || *down == '\0'))
    return;

  ScrollAction *action = g_new0(ScrollAction, 1);
  action->up = (up && *up != '\0') ? g_strdup(up) : NULL;
  action->down = (down && *down != '\0') ? g_strdup(down) : NULL;

  GtkEventController *controller =
      gtk_event_controller_scroll_new(GTK_EVENT_CONTROLLER_SCROLL_VERTICAL);
  g_signal_connect_data(controller, "scroll", G_CALLBACK(on_scroll), action,
                        scroll_action_free, 0);
  gtk_widget_add_controller(widget, controller);
}

void whelk_click_attach(GtkWidget *widget, const WhelkModuleSpec *spec) {
  if (!widget || !spec)
    return;

  attach_button(widget, spec, "on-click", GDK_BUTTON_PRIMARY);
  attach_button(widget, spec, "on-click-middle", GDK_BUTTON_MIDDLE);
  attach_button(widget, spec, "on-click-right", GDK_BUTTON_SECONDARY);
  attach_scroll(widget, spec);

  if (whelk_spec_has(spec, "on-click") ||
      whelk_spec_has(spec, "on-click-right") ||
      whelk_spec_has(spec, "on-click-middle"))
    gtk_widget_set_cursor_from_name(widget, "pointer");
}
