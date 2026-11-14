// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "widgets.h"

#include "surface.h"
#include "style.h"
#include "logger.h"
#include "util/click.h"
#include "util/command.h"
#include "util/json.h"
#include "util/sysfs.h"
#include "util/widgetstate.h"

#include <gio/gio.h>
#include <math.h>
#include <string.h>

/* =========================================================================
 * DESKTOP WIDGETS
 * =========================================================================
 * The cards macOS puts on a desktop, drawn with GTK4 and described in Scheme.
 * A widget owns its layer surface, its refresh source and, for Weather and
 * Stocks, one asynchronous request at a time.  Nothing here blocks the main
 * loop: a command or a URL answers later, and a guard says whether the card
 * that asked is still alive.
 */

typedef struct _WidgetInstance WidgetInstance;

typedef gboolean (*WidgetBuildFunc)(WidgetInstance *widget);
typedef void (*WidgetUpdateFunc)(WidgetInstance *widget);

static void clock_destroy(WidgetInstance *widget);
static void calendar_destroy(WidgetInstance *widget);
static void weather_destroy(WidgetInstance *widget);
static void battery_destroy(WidgetInstance *widget);
static void notes_destroy(WidgetInstance *widget);
static void reminders_destroy(WidgetInstance *widget);
static void world_clock_destroy(WidgetInstance *widget);
static void stocks_destroy(WidgetInstance *widget);
static void custom_destroy(WidgetInstance *widget);
static void reminders_render(WidgetInstance *widget);
static void reminders_toggle_row(GtkButton *button, gpointer user_data);
static GdkMonitor *widget_monitor(const char *connector);

/* One liveness token per widget.  An asynchronous answer can outlive the card
 * that asked; the token is killed before the widget is freed, so the callback
 * frees its own context and touches nothing else. */
typedef struct {
  gint refs;
  gboolean alive;
} WidgetGuard;

static WidgetGuard *widget_guard_new(void) {
  WidgetGuard *guard = g_new0(WidgetGuard, 1);

  guard->refs = 1;
  guard->alive = TRUE;
  return guard;
}

static WidgetGuard *widget_guard_ref(WidgetGuard *guard) {
  guard->refs++;
  return guard;
}

static void widget_guard_unref(WidgetGuard *guard) {
  if (!guard)
    return;

  guard->refs--;
  if (guard->refs == 0)
    g_free(guard);
}

static void widget_guard_kill(WidgetGuard *guard) {
  if (guard)
    guard->alive = FALSE;
}

struct _WidgetInstance {
  GtkApplication *app; /* borrowed from the manager */
  const WhelkBarConfig *style; /* borrowed for the length of a build */
  WhelkWidgetConfig *config;
  WhelkSurface *surface;
  GtkWidget *card;
  GtkCssProvider *skin;
  GdkDisplay *display; /* owned reference */
  char *widget_id;
  char *state_path; /* owned copy of the manager's widget state path */

  int drag_origin_x;
  int drag_origin_y;

  WidgetGuard *guard;
  guint timer_id;
  guint interval;
  gboolean busy;
  GCancellable *cancel;

  WidgetUpdateFunc update;
  void (*destroy)(WidgetInstance *widget);
  gpointer data;
};

struct _WhelkWidgets {
  GtkApplication *app;
  char *state_path; /* widget positions and per-widget runtime state */
  GPtrArray *instances; /* WidgetInstance* */
};

/* =========================================================================
 * SMALL HELPERS
 * ========================================================================= */

static const char *widget_option(WidgetInstance *widget, const char *key,
                                 const char *fallback) {
  return whelk_spec_str(widget->config->spec, key, fallback);
}

static int widget_option_int(WidgetInstance *widget, const char *key,
                             int fallback) {
  return whelk_spec_int(widget->config->spec, key, fallback);
}

static gboolean widget_option_bool(WidgetInstance *widget, const char *key,
                                   gboolean fallback) {
  return whelk_spec_bool(widget->config->spec, key, fallback);
}

static GtkWidget *widget_label(const char *text, const char *css_class,
                               float xalign) {
  GtkWidget *label = gtk_label_new(text);

  gtk_label_set_xalign(GTK_LABEL(label), xalign);
  gtk_widget_set_halign(label, xalign < 0.5f
                                   ? GTK_ALIGN_START
                                   : (xalign > 0.5f ? GTK_ALIGN_END
                                                    : GTK_ALIGN_CENTER));

  if (css_class && *css_class)
    gtk_widget_add_css_class(label, css_class);

  return label;
}

/* The area the widget may be dragged across: the monitor it names, the one its
 * surface is on, or the display's first monitor.  A display with no monitor
 * model (the headless test renderer) leaves the fallback untouched. */
static void widget_monitor_area(WidgetInstance *widget, int *width,
                                int *height) {
  *width = 1920;
  *height = 1080;

  GdkMonitor *monitor = widget_monitor(widget->config->monitor);

  if (!monitor && widget->display) {
    GtkWindow *window = whelk_surface_get_window(widget->surface);
    GdkSurface *surface =
        window ? gtk_native_get_surface(GTK_NATIVE(window)) : NULL;

    if (surface) {
      /* This accessor returns a borrowed monitor; the drag path owns the
       * reference for as long as it needs the geometry. */
      GdkMonitor *at_surface =
          gdk_display_get_monitor_at_surface(widget->display, surface);

      if (at_surface)
        monitor = g_object_ref(at_surface);
    }
  }

  if (!monitor && widget->display) {
    GListModel *monitors = gdk_display_get_monitors(widget->display);

    if (monitors && g_list_model_get_n_items(monitors) > 0)
      monitor = GDK_MONITOR(g_list_model_get_item(monitors, 0));
  }

  if (monitor) {
    GdkRectangle geometry;

    gdk_monitor_get_geometry(monitor, &geometry);

    if (geometry.width > 0 && geometry.height > 0) {
      *width = geometry.width;
      *height = geometry.height;
    }

    g_object_unref(monitor);
  }
}

static void widget_drag_begin(GtkGestureDrag *gesture, double start_x,
                              double start_y, gpointer user_data) {
  (void)start_x;
  (void)start_y;

  WidgetInstance *widget = user_data;
  GtkWidget *handle = gtk_event_controller_get_widget(
      GTK_EVENT_CONTROLLER(gesture));

  widget->drag_origin_x = widget->config->x;
  widget->drag_origin_y = widget->config->y;
  gtk_widget_add_css_class(widget->card, "dragging");

  if (handle)
    gtk_widget_set_cursor_from_name(handle, "grabbing");
}

static void widget_drag_update(GtkGestureDrag *gesture, double offset_x,
                               double offset_y, gpointer user_data) {
  (void)gesture;

  WidgetInstance *widget = user_data;
  int area_width = 0;
  int area_height = 0;

  widget_monitor_area(widget, &area_width, &area_height);

  int x = whelk_widget_snap((int)lround(widget->drag_origin_x + offset_x),
                            WHELK_WIDGET_GRID);
  int y = whelk_widget_snap((int)lround(widget->drag_origin_y + offset_y),
                            WHELK_WIDGET_GRID);

  whelk_widget_clamp(&x, &y, widget->config->width, widget->config->height,
                     area_width, area_height);
  widget->config->x = x;
  widget->config->y = y;
  whelk_surface_set_position(widget->surface, x, y);
}

static void widget_drag_end(GtkGestureDrag *gesture, double offset_x,
                            double offset_y, gpointer user_data) {
  (void)offset_x;
  (void)offset_y;

  WidgetInstance *widget = user_data;
  GtkWidget *handle = gtk_event_controller_get_widget(
      GTK_EVENT_CONTROLLER(gesture));

  gtk_widget_remove_css_class(widget->card, "dragging");

  if (handle)
    gtk_widget_set_cursor_from_name(handle, "grab");

  if (widget->state_path && widget->widget_id)
    whelk_widget_state_save_position(widget->state_path, widget->widget_id,
                                     widget->config->x, widget->config->y);
}

/* The grip is the drag handle, not the whole card: a card's own on-click and
 * its calendar or reminder buttons keep their clicks, and a desktop widget is
 * moved the way a window is -- by the small piece that says so. */
static GtkWidget *widget_grip(WidgetInstance *widget) {
  GtkWidget *grip = widget_label("⠿", "whelk-widget-grip", 1.0);
  GtkGesture *drag = gtk_gesture_drag_new();

  gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(drag), GDK_BUTTON_PRIMARY);
  g_signal_connect(drag, "drag-begin", G_CALLBACK(widget_drag_begin), widget);
  g_signal_connect(drag, "drag-update", G_CALLBACK(widget_drag_update), widget);
  g_signal_connect(drag, "drag-end", G_CALLBACK(widget_drag_end), widget);
  gtk_widget_add_controller(grip, GTK_EVENT_CONTROLLER(drag));
  gtk_widget_set_cursor_from_name(grip, "grab");
  gtk_widget_set_tooltip_text(grip, "Drag to move this widget");

  return grip;
}

static GtkWidget *widget_header(WidgetInstance *widget, const char *glyph,
                                const char *default_title) {
  GtkWidget *row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
  const char *title = widget_option(widget, "title", default_title);
  GtkWidget *fill = widget_label("", "whelk-widget-header-fill", 0);

  gtk_widget_add_css_class(row, "whelk-widget-header");
  gtk_box_append(GTK_BOX(row), widget_label(glyph, "whelk-widget-glyph", 0));
  gtk_box_append(GTK_BOX(row), widget_label(title, "whelk-widget-title", 0));
  gtk_widget_set_hexpand(fill, TRUE);
  gtk_box_append(GTK_BOX(row), fill);
  gtk_box_append(GTK_BOX(row), widget_grip(widget));

  return row;
}

static GdkRGBA widget_ink(WidgetInstance *widget) {
  GdkRGBA ink = {0.92, 0.92, 0.94, 1.0};

  if (widget->style && widget->style->foreground &&
      *widget->style->foreground)
    gdk_rgba_parse(&ink, widget->style->foreground);

  return ink;
}

static gboolean widget_tick(gpointer user_data) {
  WidgetInstance *widget = user_data;

  if (!widget->guard->alive)
    return G_SOURCE_REMOVE;

  if (widget->update)
    widget->update(widget);

  return G_SOURCE_CONTINUE;
}

/* =========================================================================
 * ASYNCHRONOUS DATA
 * ========================================================================= */

typedef void (*WidgetTextFunc)(WidgetInstance *widget, const char *text);

typedef struct {
  WidgetGuard *guard;
  WidgetInstance *widget;
  WidgetTextFunc done;
} FetchJob;

static void on_url_loaded(GObject *source, GAsyncResult *result,
                          gpointer user_data) {
  FetchJob *job = user_data;
  GFile *file = G_FILE(source);
  g_autofree char *contents = NULL;
  gsize length = 0;
  g_autoptr(GError) error = NULL;
  const gboolean ok = g_file_load_contents_finish(
      file, result, &contents, &length, NULL, &error);

  (void)length;

  if (job->guard->alive) {
    job->widget->busy = FALSE;
    g_clear_object(&job->widget->cancel);

    if (!ok)
      g_clear_pointer(&contents, g_free);

    job->done(job->widget, contents);
  }

  widget_guard_unref(job->guard);
  g_free(job);
}

static void widget_fetch(WidgetInstance *widget, const char *url,
                         WidgetTextFunc done) {
  if (!widget || widget->busy || !url || *url == '\0')
    return;

  widget->busy = TRUE;
  widget->cancel = g_cancellable_new();

  FetchJob *job = g_new0(FetchJob, 1);

  job->guard = widget_guard_ref(widget->guard);
  job->widget = widget;
  job->done = done;

  GFile *file = g_file_new_for_uri(url);

  g_file_load_contents_async(file, widget->cancel, on_url_loaded, job);
  g_object_unref(file);
}

typedef struct {
  WidgetGuard *guard;
  WidgetInstance *widget;
  WidgetTextFunc done;
} CommandJob;

static void on_widget_command(const char *text, gpointer user_data) {
  CommandJob *job = user_data;

  if (job->guard->alive) {
    job->widget->busy = FALSE;
    job->done(job->widget, text);
  }

  widget_guard_unref(job->guard);
  g_free(job);
}

static void widget_command(WidgetInstance *widget, const char *command,
                           WidgetTextFunc done) {
  if (!widget || widget->busy || !command || *command == '\0')
    return;

  widget->busy = TRUE;

  CommandJob *job = g_new0(CommandJob, 1);

  job->guard = widget_guard_ref(widget->guard);
  job->widget = widget;
  job->done = done;

  whelk_command_run(command, on_widget_command, job);
}

/* =========================================================================
 * CLOCK
 * ========================================================================= */

typedef struct {
  GtkWidget *face;
  GtkWidget *time;
  GtkWidget *date;
  gboolean analog;
  gboolean seconds;
  GdkRGBA ink;
} ClockWidget;

static void draw_clock_face(GtkDrawingArea *area, cairo_t *cr, int width,
                            int height, gpointer user_data) {
  (void)area;

  ClockWidget *clock = user_data;
  g_autoptr(GDateTime) now = g_date_time_new_now_local();

  if (!now)
    return;

  const double cx = width / 2.0;
  const double cy = height / 2.0;
  const double radius = MAX(1.0, MIN(width, height) / 2.0 - 7.0);

  cairo_set_source_rgba(cr, clock->ink.red, clock->ink.green, clock->ink.blue,
                        0.82);
  cairo_set_line_width(cr, 1.2);
  cairo_arc(cr, cx, cy, radius, 0.0, 2.0 * G_PI);
  cairo_stroke(cr);

  for (int tick = 0; tick < 60; tick++) {
    const double angle = tick * G_PI / 30.0 - G_PI / 2.0;
    const double inner = radius - (tick % 5 == 0 ? 10.0 : 5.0);
    const double outer = radius - 3.0;

    cairo_set_line_width(cr, tick % 5 == 0 ? 2.0 : 0.8);
    cairo_move_to(cr, cx + cos(angle) * inner, cy + sin(angle) * inner);
    cairo_line_to(cr, cx + cos(angle) * outer, cy + sin(angle) * outer);
    cairo_stroke(cr);
  }

  const double hour =
      (g_date_time_get_hour(now) % 12) + g_date_time_get_minute(now) / 60.0;
  const double minute = g_date_time_get_minute(now) +
                        g_date_time_get_second(now) / 60.0;
  const double second = g_date_time_get_second(now);

  cairo_set_line_cap(cr, CAIRO_LINE_CAP_ROUND);
  cairo_set_line_width(cr, 5.0);
  cairo_move_to(cr, cx, cy);
  cairo_line_to(cr, cx + cos(hour * G_PI / 6.0 - G_PI / 2.0) * radius * 0.48,
                cy + sin(hour * G_PI / 6.0 - G_PI / 2.0) * radius * 0.48);
  cairo_stroke(cr);

  cairo_set_line_width(cr, 3.0);
  cairo_move_to(cr, cx, cy);
  cairo_line_to(
      cr, cx + cos(minute * G_PI / 30.0 - G_PI / 2.0) * radius * 0.70,
      cy + sin(minute * G_PI / 30.0 - G_PI / 2.0) * radius * 0.70);
  cairo_stroke(cr);

  if (clock->seconds) {
    cairo_set_source_rgba(cr, clock->ink.red, clock->ink.green, clock->ink.blue,
                          0.55);
    cairo_set_line_width(cr, 1.2);
    cairo_move_to(cr, cx, cy);
    cairo_line_to(cr,
                  cx + cos(second * G_PI / 30.0 - G_PI / 2.0) * radius * 0.82,
                  cy + sin(second * G_PI / 30.0 - G_PI / 2.0) * radius * 0.82);
    cairo_stroke(cr);
  }

  cairo_set_source_rgba(cr, clock->ink.red, clock->ink.green, clock->ink.blue,
                        0.92);
  cairo_arc(cr, cx, cy, 3.0, 0.0, 2.0 * G_PI);
  cairo_fill(cr);
}

static void clock_update(WidgetInstance *widget) {
  ClockWidget *clock = widget->data;
  g_autoptr(GDateTime) now = g_date_time_new_now_local();

  if (!now) {
    gtk_label_set_text(GTK_LABEL(clock->time), "--:--");
    return;
  }

  g_autofree char *time_text = g_date_time_format(now, "%H:%M");
  g_autofree char *date_text = g_date_time_format(now, "%A, %d %B");

  gtk_label_set_text(GTK_LABEL(clock->time), time_text);
  gtk_label_set_text(GTK_LABEL(clock->date), date_text);

  if (clock->face)
    gtk_widget_queue_draw(clock->face);
}

static gboolean clock_build(WidgetInstance *widget) {
  ClockWidget *clock = g_new0(ClockWidget, 1);

  clock->analog =
      g_strcmp0(widget_option(widget, "style", "analog"), "digital") != 0;
  clock->seconds = widget_option_bool(widget, "show-seconds?", TRUE);
  clock->ink = widget_ink(widget);

  gtk_box_append(GTK_BOX(widget->card), widget_header(widget, "◷", "Clock"));

  if (clock->analog) {
    clock->face = gtk_drawing_area_new();
    gtk_widget_add_css_class(clock->face, "whelk-widget-clock-face");
    gtk_widget_set_hexpand(clock->face, TRUE);
    gtk_widget_set_vexpand(clock->face, TRUE);
    gtk_widget_set_size_request(clock->face, 120, 120);
    gtk_drawing_area_set_content_width(GTK_DRAWING_AREA(clock->face), 170);
    gtk_drawing_area_set_content_height(GTK_DRAWING_AREA(clock->face), 170);
    gtk_drawing_area_set_draw_func(GTK_DRAWING_AREA(clock->face),
                                   draw_clock_face, clock, NULL);
    gtk_box_append(GTK_BOX(widget->card), clock->face);
  }

  clock->time = widget_label("--:--", "whelk-widget-clock-time", 0.5);

  if (!clock->analog)
    gtk_widget_add_css_class(clock->time, "digital");

  clock->date = widget_label("", "whelk-widget-subtle", 0.5);
  gtk_box_append(GTK_BOX(widget->card), clock->time);
  gtk_box_append(GTK_BOX(widget->card), clock->date);

  widget->data = clock;
  widget->update = clock_update;
  widget->destroy = clock_destroy;
  widget->interval = 1;
  return TRUE;
}

static void clock_destroy(WidgetInstance *widget) {
  g_free(widget->data);
  widget->data = NULL;
}

/* =========================================================================
 * CALENDAR
 * ========================================================================= */

typedef struct {
  GtkWidget *month;
  GtkWidget *grid;
  GDateTime *shown; /* the first day of the month on the card */
} CalendarWidget;

static GDateTime *calendar_first_of_month(GDateTime *date) {
  return g_date_time_new_local(g_date_time_get_year(date),
                               g_date_time_get_month(date), 1, 0, 0, 0.0);
}

static void calendar_update(WidgetInstance *widget) {
  CalendarWidget *calendar = widget->data;
  g_autoptr(GDateTime) now = g_date_time_new_now_local();

  if (!now)
    return;

  if (!calendar->shown)
    calendar->shown = calendar_first_of_month(now);

  if (!calendar->shown)
    return;

  const int year = g_date_time_get_year(calendar->shown);
  const int month = g_date_time_get_month(calendar->shown);
  g_autoptr(GDateTime) first = calendar_first_of_month(calendar->shown);

  if (!first)
    return;

  g_autoptr(GDateTime) next_month = g_date_time_add_months(first, 1);
  g_autoptr(GDateTime) last = g_date_time_add_days(next_month, -1);
  g_autofree char *heading = g_date_time_format(first, "%B %Y");
  const int days = g_date_time_get_day_of_month(last);
  const int today = g_date_time_get_day_of_month(now);
  const int leading = g_date_time_get_day_of_week(first) - 1;
  const gboolean this_month = year == g_date_time_get_year(now) &&
                              month == g_date_time_get_month(now);

  gtk_button_set_label(GTK_BUTTON(calendar->month), heading);

  GtkWidget *child = gtk_widget_get_first_child(calendar->grid);

  while (child) {
    GtkWidget *next = gtk_widget_get_next_sibling(child);

    gtk_widget_unparent(child);
    child = next;
  }

  static const char *const weekdays[] = {"Mo", "Tu", "We", "Th", "Fr",
                                         "Sa", "Su"};

  for (int column = 0; column < 7; column++) {
    GtkWidget *label = widget_label(weekdays[column], "whelk-widget-weekday",
                                    0.5);

    gtk_widget_set_hexpand(label, TRUE);
    gtk_grid_attach(GTK_GRID(calendar->grid), label, column, 0, 1, 1);
  }

  for (int day = 1; day <= days; day++) {
    const int slot = leading + day - 1;
    g_autofree char *text = g_strdup_printf("%d", day);
    GtkWidget *label = widget_label(text, "whelk-widget-day", 0.5);

    gtk_widget_set_hexpand(label, TRUE);
    gtk_widget_set_valign(label, GTK_ALIGN_FILL);

    if (this_month && day == today)
      gtk_widget_add_css_class(label, "today");

    gtk_grid_attach(GTK_GRID(calendar->grid), label, slot % 7,
                    slot / 7 + 1, 1, 1);
  }
}

static void calendar_shift(WidgetInstance *widget, int months) {
  CalendarWidget *calendar = widget->data;
  g_autoptr(GDateTime) now = g_date_time_new_now_local();

  if (!calendar->shown && now)
    calendar->shown = calendar_first_of_month(now);

  if (!calendar->shown)
    return;

  GDateTime *next = g_date_time_add_months(calendar->shown, months);

  if (!next)
    return;

  g_date_time_unref(calendar->shown);
  calendar->shown = next;
  calendar_update(widget);
}

static void calendar_today(WidgetInstance *widget) {
  CalendarWidget *calendar = widget->data;
  g_autoptr(GDateTime) now = g_date_time_new_now_local();

  if (!now)
    return;

  GDateTime *first = calendar_first_of_month(now);

  if (!first)
    return;

  if (calendar->shown)
    g_date_time_unref(calendar->shown);

  calendar->shown = first;
  calendar_update(widget);
}

static void on_calendar_previous(GtkButton *button, gpointer user_data) {
  (void)button;
  calendar_shift(user_data, -1);
}

static void on_calendar_next(GtkButton *button, gpointer user_data) {
  (void)button;
  calendar_shift(user_data, 1);
}

static void on_calendar_today(GtkButton *button, gpointer user_data) {
  (void)button;
  calendar_today(user_data);
}

static GtkWidget *calendar_button(const char *label, const char *tooltip,
                                  GCallback callback, WidgetInstance *widget) {
  GtkWidget *button = gtk_button_new_with_label(label);

  gtk_widget_add_css_class(button, "flat");
  gtk_widget_add_css_class(button, "whelk-widget-calendar-button");
  gtk_widget_set_focusable(button, FALSE);
  gtk_widget_set_tooltip_text(button, tooltip);
  g_signal_connect(button, "clicked", callback, widget);

  return button;
}

static gboolean calendar_build(WidgetInstance *widget) {
  CalendarWidget *calendar = g_new0(CalendarWidget, 1);
  GtkWidget *navigation = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 4);
  g_autoptr(GDateTime) now = g_date_time_new_now_local();

  if (now)
    calendar->shown = calendar_first_of_month(now);

  gtk_box_append(GTK_BOX(widget->card),
                 widget_header(widget, "▦", "Calendar"));

  gtk_widget_add_css_class(navigation, "whelk-widget-calendar-nav");
  gtk_box_append(GTK_BOX(navigation),
                 calendar_button("‹", "The previous month",
                                 G_CALLBACK(on_calendar_previous), widget));
  calendar->month = calendar_button("", "Back to today",
                                    G_CALLBACK(on_calendar_today), widget);
  gtk_widget_add_css_class(calendar->month, "whelk-widget-calendar-month");
  gtk_widget_set_hexpand(calendar->month, TRUE);
  gtk_box_append(GTK_BOX(navigation), calendar->month);
  gtk_box_append(GTK_BOX(navigation),
                 calendar_button("›", "The next month",
                                 G_CALLBACK(on_calendar_next), widget));
  gtk_box_append(GTK_BOX(widget->card), navigation);

  calendar->grid = gtk_grid_new();
  gtk_widget_add_css_class(calendar->grid, "whelk-widget-calendar-grid");
  gtk_grid_set_column_homogeneous(GTK_GRID(calendar->grid), TRUE);
  gtk_grid_set_row_spacing(GTK_GRID(calendar->grid), 2);
  gtk_widget_set_vexpand(calendar->grid, TRUE);
  gtk_box_append(GTK_BOX(widget->card), calendar->grid);

  widget->data = calendar;
  widget->update = calendar_update;
  widget->destroy = calendar_destroy;
  widget->interval = 60;
  return TRUE;
}

static void calendar_destroy(WidgetInstance *widget) {
  CalendarWidget *calendar = widget->data;

  if (calendar)
    g_clear_pointer(&calendar->shown, g_date_time_unref);

  g_free(widget->data);
  widget->data = NULL;
}

/* =========================================================================
 * WEATHER
 * ========================================================================= */

typedef struct {
  GtkWidget *place;
  GtkWidget *icon;
  GtkWidget *temperature;
  GtkWidget *condition;
  GtkWidget *range;
  GtkWidget *details;
  gboolean metric;
} WeatherWidget;

static const char *weather_condition(int code) {
  if (code == 0)
    return "Clear";
  if (code >= 1 && code <= 3)
    return "Partly cloudy";
  if (code == 45 || code == 48)
    return "Fog";
  if (code >= 51 && code <= 57)
    return "Drizzle";
  if (code >= 61 && code <= 67)
    return "Rain";
  if (code >= 71 && code <= 77)
    return "Snow";
  if (code >= 80 && code <= 82)
    return "Showers";
  if (code >= 85 && code <= 86)
    return "Snow showers";
  if (code >= 95)
    return "Thunderstorm";

  return "Weather";
}

static const char *weather_glyph(int code) {
  if (code == 0)
    return "☀";
  if (code >= 1 && code <= 3)
    return "⛅";
  if (code == 45 || code == 48)
    return "🌫";
  if (code >= 51 && code <= 57)
    return "🌦";
  if (code >= 61 && code <= 67)
    return "☔";
  if (code >= 71 && code <= 86)
    return "❄";
  if (code >= 95)
    return "⛈";

  return "☁";
}

static void weather_show_unavailable(WidgetInstance *widget) {
  WeatherWidget *weather = widget->data;

  gtk_label_set_text(GTK_LABEL(weather->icon), "·");
  gtk_label_set_text(GTK_LABEL(weather->temperature), "—");
  gtk_label_set_text(GTK_LABEL(weather->condition), "Weather unavailable");
  gtk_label_set_text(GTK_LABEL(weather->range), "");
  gtk_label_set_text(GTK_LABEL(weather->details), "");
}

static void weather_apply(WidgetInstance *widget, const char *text) {
  WeatherWidget *weather = widget->data;

  if (!text || *text == '\0') {
    weather_show_unavailable(widget);
    return;
  }

  g_autoptr(GError) error = NULL;
  WhelkJson *root = whelk_json_parse(text, &error);

  if (!root) {
    weather_show_unavailable(widget);
    return;
  }

  WhelkJson *current = whelk_json_object_get(root, "current");
  WhelkJson *daily = whelk_json_object_get(root, "daily");
  WhelkJson *highs = whelk_json_object_get(daily, "temperature_2m_max");
  WhelkJson *lows = whelk_json_object_get(daily, "temperature_2m_min");

  if (!current || !highs || !lows || whelk_json_array_length(highs) == 0 ||
      whelk_json_array_length(lows) == 0) {
    weather_show_unavailable(widget);
    whelk_json_free(root);
    return;
  }

  const double temperature =
      whelk_json_object_number(current, "temperature_2m", 0.0);
  const double feels =
      whelk_json_object_number(current, "apparent_temperature", temperature);
  const double humidity =
      whelk_json_object_number(current, "relative_humidity_2m", -1.0);
  const double high = whelk_json_number(whelk_json_array_get(highs, 0));
  const double low = whelk_json_number(whelk_json_array_get(lows, 0));
  const int code = (int)whelk_json_object_number(current, "weather_code", 0);
  const char *unit = weather->metric ? "C" : "F";

  g_autofree char *temperature_text =
      g_strdup_printf("%.0f°%s", temperature, unit);
  g_autofree char *range_text =
      g_strdup_printf("H %.0f°  L %.0f°", high, low);
  g_autofree char *details_text =
      humidity >= 0
          ? g_strdup_printf("Feels like %.0f° · %.0f%% humidity", feels,
                            humidity)
          : g_strdup_printf("Feels like %.0f°", feels);

  gtk_label_set_text(GTK_LABEL(weather->icon), weather_glyph(code));
  gtk_label_set_text(GTK_LABEL(weather->temperature), temperature_text);
  gtk_label_set_text(GTK_LABEL(weather->condition), weather_condition(code));
  gtk_label_set_text(GTK_LABEL(weather->range), range_text);
  gtk_label_set_text(GTK_LABEL(weather->details), details_text);

  whelk_json_free(root);
}

static void weather_update(WidgetInstance *widget) {
  WeatherWidget *weather = widget->data;
  const char *latitude = widget_option(widget, "latitude", "37.7749");
  const char *longitude = widget_option(widget, "longitude", "-122.4194");
  const char *units = widget_option(widget, "units", "metric");
  const gboolean imperial = g_strcmp0(units, "imperial") == 0;
  const char *unit_query = imperial ? "&temperature_unit=fahrenheit" : "";
  g_autofree char *url = g_strdup_printf(
      "https://api.open-meteo.com/v1/forecast?latitude=%s&longitude=%s"
      "&current=temperature_2m,apparent_temperature,relative_humidity_2m,"
      "weather_code&daily=temperature_2m_max,temperature_2m_min"
      "&forecast_days=1&timezone=auto%s",
      latitude, longitude, unit_query);

  weather->metric = !imperial;
  gtk_label_set_text(GTK_LABEL(weather->place),
                     widget_option(widget, "location", "San Francisco"));
  widget_fetch(widget, url, weather_apply);
}

static gboolean weather_build(WidgetInstance *widget) {
  WeatherWidget *weather = g_new0(WeatherWidget, 1);
  GtkWidget *reading = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 12);

  gtk_box_append(GTK_BOX(widget->card),
                 widget_header(widget, "☁", "Weather"));

  weather->place = widget_label("", "whelk-widget-subtle", 0);
  gtk_box_append(GTK_BOX(widget->card), weather->place);

  gtk_widget_set_hexpand(reading, TRUE);
  weather->icon = widget_label("·", "whelk-widget-weather-icon", 0.5);
  weather->temperature =
      widget_label("—", "whelk-widget-temperature", 0);
  gtk_widget_set_valign(weather->icon, GTK_ALIGN_CENTER);
  gtk_box_append(GTK_BOX(reading), weather->icon);
  gtk_box_append(GTK_BOX(reading), weather->temperature);
  gtk_box_append(GTK_BOX(widget->card), reading);

  weather->condition = widget_label("", "whelk-widget-reading", 0);
  weather->range = widget_label("", "whelk-widget-subtle", 0);
  weather->details = widget_label("", "whelk-widget-subtle", 0);
  gtk_box_append(GTK_BOX(widget->card), weather->condition);
  gtk_box_append(GTK_BOX(widget->card), weather->range);
  gtk_box_append(GTK_BOX(widget->card), weather->details);

  widget->data = weather;
  widget->update = weather_update;
  widget->destroy = weather_destroy;
  widget->interval = 1800;
  return TRUE;
}

static void weather_destroy(WidgetInstance *widget) {
  g_free(widget->data);
  widget->data = NULL;
}

/* =========================================================================
 * BATTERY
 * ========================================================================= */

typedef struct {
  GtkWidget *percent;
  GtkWidget *status;
  GtkWidget *level;
  GtkWidget *device;
} BatteryWidget;

static gboolean battery_charging(const char *status) {
  return g_strcmp0(status, "Charging") == 0 ||
         g_strcmp0(status, "Full") == 0 ||
         g_strcmp0(status, "Not charging") == 0;
}

static void battery_update(WidgetInstance *widget) {
  BatteryWidget *battery = widget->data;
  WhelkBatteryInfo info = {0};

  if (!whelk_battery_read(widget_option(widget, "device", NULL), &info) ||
      !info.present) {
    gtk_label_set_text(GTK_LABEL(battery->percent), "—");
    gtk_label_set_text(GTK_LABEL(battery->status), "No battery");
    gtk_label_set_text(GTK_LABEL(battery->device), "");
    gtk_widget_set_visible(battery->level, FALSE);
    whelk_battery_clear(&info);
    return;
  }

  const int warning = widget_option_int(widget, "warning", 30);
  const int critical = widget_option_int(widget, "critical", 15);
  const gboolean charging = battery_charging(info.status);
  g_autofree char *percent = g_strdup_printf("%d%%", info.capacity);
  g_autofree char *status = g_strdup_printf(
      "%s · %s", info.status, charging ? "charging" : "on battery");

  gtk_label_set_text(GTK_LABEL(battery->percent), percent);
  gtk_label_set_text(GTK_LABEL(battery->status), status);
  gtk_label_set_text(GTK_LABEL(battery->device), info.name);
  gtk_widget_set_visible(battery->level, TRUE);
  gtk_level_bar_set_value(GTK_LEVEL_BAR(battery->level), info.capacity);

  gtk_widget_remove_css_class(widget->card, "warning");
  gtk_widget_remove_css_class(widget->card, "critical");
  gtk_widget_remove_css_class(widget->card, "charging");

  if (info.capacity <= critical)
    gtk_widget_add_css_class(widget->card, "critical");
  else if (info.capacity <= warning)
    gtk_widget_add_css_class(widget->card, "warning");

  if (charging)
    gtk_widget_add_css_class(widget->card, "charging");

  whelk_battery_clear(&info);
}

static gboolean battery_build(WidgetInstance *widget) {
  BatteryWidget *battery = g_new0(BatteryWidget, 1);

  gtk_box_append(GTK_BOX(widget->card),
                 widget_header(widget, "▮", "Batteries"));

  battery->percent = widget_label("—", "whelk-widget-battery-percent", 0);
  gtk_box_append(GTK_BOX(widget->card), battery->percent);

  battery->level = gtk_level_bar_new_for_interval(0, 100);
  gtk_widget_add_css_class(battery->level, "whelk-widget-battery-level");
  gtk_level_bar_set_value(GTK_LEVEL_BAR(battery->level), 0);
  gtk_widget_set_hexpand(battery->level, TRUE);
  gtk_box_append(GTK_BOX(widget->card), battery->level);

  battery->status = widget_label("", "whelk-widget-reading", 0);
  battery->device = widget_label("", "whelk-widget-subtle", 0);
  gtk_box_append(GTK_BOX(widget->card), battery->status);
  gtk_box_append(GTK_BOX(widget->card), battery->device);

  widget->data = battery;
  widget->update = battery_update;
  widget->destroy = battery_destroy;
  widget->interval = 30;
  return TRUE;
}

static void battery_destroy(WidgetInstance *widget) {
  g_free(widget->data);
  widget->data = NULL;
}

/* =========================================================================
 * NOTES
 * ========================================================================= */

typedef struct {
  GtkWidget *body;
} NotesWidget;

static void notes_update(WidgetInstance *widget) {
  NotesWidget *notes = widget->data;
  const char *file = widget_option(widget, "file", NULL);
  g_autofree char *contents = NULL;

  if (file && *file && g_file_get_contents(file, &contents, NULL, NULL)) {
    g_strstrip(contents);
  } else {
    contents = g_strdup(widget_option(
        widget, "text",
        "No note yet.\nSet #:text or #:file on this widget."));
  }

  gtk_label_set_text(GTK_LABEL(notes->body), contents);
}

static gboolean notes_build(WidgetInstance *widget) {
  NotesWidget *notes = g_new0(NotesWidget, 1);

  gtk_box_append(GTK_BOX(widget->card),
                 widget_header(widget, "✎", "Notes"));

  notes->body = widget_label("", "whelk-widget-note", 0);
  gtk_label_set_wrap(GTK_LABEL(notes->body), TRUE);
  gtk_label_set_wrap_mode(GTK_LABEL(notes->body), PANGO_WRAP_WORD_CHAR);
  gtk_label_set_ellipsize(GTK_LABEL(notes->body), PANGO_ELLIPSIZE_END);
  gtk_label_set_lines(GTK_LABEL(notes->body), 7);
  gtk_label_set_yalign(GTK_LABEL(notes->body), 0);
  gtk_widget_set_vexpand(notes->body, TRUE);
  gtk_widget_set_hexpand(notes->body, TRUE);
  gtk_box_append(GTK_BOX(widget->card), notes->body);

  widget->data = notes;
  widget->update = notes_update;
  widget->destroy = notes_destroy;
  widget->interval = (widget_option(widget, "file", NULL) != NULL) ? 30 : 0;
  return TRUE;
}

static void notes_destroy(WidgetInstance *widget) {
  g_free(widget->data);
  widget->data = NULL;
}

/* =========================================================================
 * REMINDERS
 * ========================================================================= */

typedef struct {
  GtkWidget *list;
  GPtrArray *items; /* char*, in the order the card draws them */
} RemindersWidget;

static gboolean reminder_done(const char *text) {
  return g_str_has_prefix(text, "[x] ") || g_str_has_prefix(text, "x ") ||
         g_str_has_prefix(text, "[X] ") || g_str_has_prefix(text, "X ");
}

static const char *reminder_text(const char *text) {
  if (g_str_has_prefix(text, "[x] ") || g_str_has_prefix(text, "x "))
    return text + 4;
  if (g_str_has_prefix(text, "[X] ") || g_str_has_prefix(text, "X "))
    return text + 4;

  return text;
}

static void reminders_save(WidgetInstance *widget) {
  RemindersWidget *reminders = widget->data;
  GString *value = g_string_new(NULL);

  if (!widget->state_path || !widget->widget_id) {
    g_string_free(value, TRUE);
    return;
  }

  for (guint i = 0; i < reminders->items->len; i++) {
    if (i > 0)
      g_string_append_c(value, '\n');

    g_string_append(value, g_ptr_array_index(reminders->items, i));
  }

  whelk_widget_state_set(widget->state_path, widget->widget_id, "items",
                         value->str);
  g_string_free(value, TRUE);
}

static void reminders_render(WidgetInstance *widget) {
  RemindersWidget *reminders = widget->data;

  GtkWidget *child = gtk_widget_get_first_child(reminders->list);

  while (child) {
    GtkWidget *next = gtk_widget_get_next_sibling(child);

    gtk_widget_unparent(child);
    child = next;
  }

  for (guint i = 0; i < reminders->items->len && i < 8; i++) {
    const char *text = g_ptr_array_index(reminders->items, i);
    const gboolean done = reminder_done(text);
    const char *plain = reminder_text(text);
    g_autofree char *markup =
        done ? g_markup_printf_escaped("<s>%s</s>", plain)
             : g_markup_escape_text(plain, -1);
    GtkWidget *row = gtk_button_new();
    GtkWidget *content = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
    GtkWidget *mark = widget_label(done ? "✓" : "○",
                                   done ? "whelk-widget-reminder-done"
                                        : "whelk-widget-reminder-mark",
                                   0);
    GtkWidget *label = widget_label("", "whelk-widget-reminder", 0);

    gtk_label_set_markup(GTK_LABEL(label), markup);
    gtk_label_set_ellipsize(GTK_LABEL(label), PANGO_ELLIPSIZE_END);
    gtk_label_set_max_width_chars(GTK_LABEL(label), 28);
    gtk_widget_set_hexpand(label, TRUE);
    gtk_box_append(GTK_BOX(content), mark);
    gtk_box_append(GTK_BOX(content), label);
    gtk_button_set_child(GTK_BUTTON(row), content);
    gtk_widget_add_css_class(row, "flat");
    gtk_widget_add_css_class(row, "whelk-widget-reminder-row");
    gtk_widget_set_tooltip_text(
        row, done ? "Mark this reminder as not done" : "Mark this reminder done");
    g_object_set_data(G_OBJECT(row), "whelk-reminder-index",
                      GUINT_TO_POINTER(i));
    g_signal_connect(row, "clicked", G_CALLBACK(reminders_toggle_row), widget);
    gtk_box_append(GTK_BOX(reminders->list), row);
  }
}

static void reminders_toggle_row(GtkButton *button, gpointer user_data) {
  WidgetInstance *widget = user_data;
  RemindersWidget *reminders = widget->data;
  const guint index =
      GPOINTER_TO_UINT(g_object_get_data(G_OBJECT(button),
                                         "whelk-reminder-index"));

  if (index >= reminders->items->len)
    return;

  const char *old = g_ptr_array_index(reminders->items, index);
  const gboolean done = reminder_done(old);
  char *next = done ? g_strdup(reminder_text(old))
                    : g_strdup_printf("[x] %s", reminder_text(old));

  g_free(g_ptr_array_index(reminders->items, index));
  g_ptr_array_index(reminders->items, index) = next;
  reminders_save(widget);
  reminders_render(widget);
}

static void reminders_add(RemindersWidget *reminders, const char *text) {
  g_autofree char *clean = g_strdup(text);

  g_strstrip(clean);

  if (*clean != '\0')
    g_ptr_array_add(reminders->items, g_steal_pointer(&clean));
}

static gboolean reminders_build(WidgetInstance *widget) {
  RemindersWidget *reminders = g_new0(RemindersWidget, 1);
  const char *defaults =
      "Review the release,Water the plants,Read the manual";
  g_autofree char *saved =
      whelk_widget_state_get(widget->state_path, widget->widget_id, "items");
  const char *items =
      (saved && *saved) ? saved : widget_option(widget, "items", defaults);
  char **parts = g_strsplit_set(items, ",\n", -1);

  reminders->items = g_ptr_array_new_with_free_func(g_free);
  gtk_box_append(GTK_BOX(widget->card),
                 widget_header(widget, "☑", "Reminders"));

  for (guint i = 0; parts[i]; i++)
    reminders_add(reminders, parts[i]);

  g_strfreev(parts);

  reminders->list = gtk_box_new(GTK_ORIENTATION_VERTICAL, 4);
  gtk_widget_add_css_class(reminders->list, "whelk-widget-reminder-list");
  gtk_widget_set_vexpand(reminders->list, TRUE);
  gtk_box_append(GTK_BOX(widget->card), reminders->list);

  widget->data = reminders;
  widget->update = reminders_render;
  widget->destroy = reminders_destroy;
  widget->interval = 0;
  return TRUE;
}

static void reminders_destroy(WidgetInstance *widget) {
  RemindersWidget *reminders = widget->data;

  if (reminders)
    g_clear_pointer(&reminders->items, g_ptr_array_unref);

  g_free(widget->data);
  widget->data = NULL;
}

/* =========================================================================
 * WORLD CLOCK
 * ========================================================================= */

typedef struct {
  char *label;
  GTimeZone *zone;
  GtkWidget *time;
  GtkWidget *day;
} CityRow;

typedef struct {
  GPtrArray *cities;
} WorldClockWidget;

static void city_row_free(CityRow *city) {
  if (!city)
    return;

  g_free(city->label);
  g_clear_pointer(&city->zone, g_time_zone_unref);
  g_free(city);
}

static void world_clock_update(WidgetInstance *widget) {
  WorldClockWidget *world = widget->data;

  for (guint i = 0; i < world->cities->len; i++) {
    CityRow *city = g_ptr_array_index(world->cities, i);
    g_autoptr(GDateTime) now = g_date_time_new_now(city->zone);

    if (!now)
      continue;

    g_autofree char *time = g_date_time_format(now, "%H:%M");
    g_autofree char *day = g_date_time_format(now, "%a");

    gtk_label_set_text(GTK_LABEL(city->time), time);
    gtk_label_set_text(GTK_LABEL(city->day), day);
  }
}

static gboolean world_clock_build(WidgetInstance *widget) {
  WorldClockWidget *world = g_new0(WorldClockWidget, 1);
  const char *cities = widget_option(
      widget, "cities",
      "Cupertino=America/Los_Angeles,London=Europe/London,Tokyo=Asia/Tokyo");
  char **entries = g_strsplit(cities, ",", -1);

  world->cities = g_ptr_array_new_with_free_func((GDestroyNotify)city_row_free);
  gtk_box_append(GTK_BOX(widget->card),
                 widget_header(widget, "⊕", "World Clock"));

  for (guint i = 0; entries[i]; i++) {
    char *entry = g_strstrip(entries[i]);

    if (*entry == '\0')
      continue;

    char *equals = strchr(entry, '=');
    char *label = equals ? g_strndup(entry, (gsize)(equals - entry)) : NULL;
    const char *identifier = equals ? equals + 1 : entry;
    GTimeZone *zone = g_time_zone_new_identifier(identifier);

    if (!zone) {
      LOG_C_WARN("Widget", "unknown time zone '%s' in the world clock",
                 identifier);
      zone = g_time_zone_new_utc();
    }

    if (!label || *label == '\0') {
      g_free(label);
      const char *slash = strrchr(identifier, '/');

      label = g_strdup(slash ? slash + 1 : identifier);
    }

    CityRow *city = g_new0(CityRow, 1);
    GtkWidget *row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 10);
    GtkWidget *name = widget_label(label, "whelk-widget-city", 0);

    city->label = g_strdup(label);
    city->zone = zone;
    city->time = widget_label("--:--", "whelk-widget-world-time", 1.0);
    city->day = widget_label("", "whelk-widget-subtle", 1.0);

    gtk_widget_set_hexpand(name, TRUE);
    gtk_box_append(GTK_BOX(row), name);
    gtk_box_append(GTK_BOX(row), city->time);
    gtk_box_append(GTK_BOX(row), city->day);
    gtk_widget_set_halign(city->day, GTK_ALIGN_END);
    gtk_box_append(GTK_BOX(widget->card), row);
    g_ptr_array_add(world->cities, city);
    g_free(label);
  }

  g_strfreev(entries);

  widget->data = world;
  widget->update = world_clock_update;
  widget->destroy = world_clock_destroy;
  widget->interval = 30;
  return TRUE;
}

static void world_clock_destroy(WidgetInstance *widget) {
  WorldClockWidget *world = widget->data;

  if (world) {
    g_clear_pointer(&world->cities, g_ptr_array_unref);
    g_free(world);
  }

  widget->data = NULL;
}

/* =========================================================================
 * STOCKS
 * ========================================================================= */

typedef struct {
  char *symbol;
  GtkWidget *price;
  GtkWidget *change;
} StockRow;

typedef struct {
  GPtrArray *rows;
  GtkWidget *footnote;
} StocksWidget;

static void stock_row_free(StockRow *row) {
  if (!row)
    return;

  g_free(row->symbol);
  g_free(row);
}

static void stocks_apply(WidgetInstance *widget, const char *text) {
  StocksWidget *stocks = widget->data;

  if (!text || *text == '\0') {
    gtk_label_set_text(GTK_LABEL(stocks->footnote), "Quotes unavailable");
    return;
  }

  g_autoptr(GError) error = NULL;
  WhelkJson *root = whelk_json_parse(text, &error);

  if (!root) {
    gtk_label_set_text(GTK_LABEL(stocks->footnote), "Quotes unavailable");
    return;
  }

  guint quoted = 0;

  for (guint i = 0; i < stocks->rows->len; i++) {
    StockRow *row = g_ptr_array_index(stocks->rows, i);
    WhelkJson *quote = whelk_json_object_get(root, row->symbol);

    if (!quote)
      continue;

    const double price = whelk_json_object_number(quote, "fulldayPrice", 0.0);
    double change =
        whelk_json_object_number(quote, "fulldayChangePercent", NAN);

    if (price <= 0.0)
      continue;

    if (isnan(change)) {
      const double previous =
          whelk_json_object_number(quote, "chartPreviousClose", 0.0);

      change = previous > 0.0 ? (price - previous) / previous * 100.0 : 0.0;
    }

    g_autofree char *price_text = g_strdup_printf("%.2f", price);
    g_autofree char *change_text = g_strdup_printf("%+.2f%%", change);

    gtk_label_set_text(GTK_LABEL(row->price), price_text);
    gtk_label_set_text(GTK_LABEL(row->change), change_text);
    gtk_widget_remove_css_class(row->change, "up");
    gtk_widget_remove_css_class(row->change, "down");
    gtk_widget_add_css_class(row->change, change >= 0.0 ? "up" : "down");
    quoted++;
  }

  whelk_json_free(root);
  gtk_label_set_text(GTK_LABEL(stocks->footnote),
                     quoted > 0 ? "Delayed quotes · Yahoo Finance"
                                : "Quotes unavailable");
}

static void stocks_update(WidgetInstance *widget) {
  StocksWidget *stocks = widget->data;
  GString *query = g_string_new(NULL);

  for (guint i = 0; i < stocks->rows->len; i++) {
    StockRow *row = g_ptr_array_index(stocks->rows, i);

    if (query->len > 0)
      g_string_append_c(query, ',');

    g_string_append(query, row->symbol);
  }

  if (query->len == 0) {
    g_string_free(query, TRUE);
    gtk_label_set_text(GTK_LABEL(stocks->footnote), "Add symbols");
    return;
  }

  g_autofree char *encoded = g_uri_escape_string(query->str, NULL, TRUE);
  g_autofree char *url = g_strdup_printf(
      "https://query1.finance.yahoo.com/v8/finance/spark?symbols=%s"
      "&range=1d&interval=1d",
      encoded);

  g_string_free(query, TRUE);
  widget_fetch(widget, url, stocks_apply);
}

static gboolean stocks_build(WidgetInstance *widget) {
  StocksWidget *stocks = g_new0(StocksWidget, 1);
  const char *symbols = widget_option(widget, "symbols", "AAPL,MSFT,NVDA");
  char **entries = g_strsplit(symbols, ",", -1);

  stocks->rows = g_ptr_array_new_with_free_func((GDestroyNotify)stock_row_free);
  gtk_box_append(GTK_BOX(widget->card),
                 widget_header(widget, "↗", "Stocks"));

  for (guint i = 0; entries[i] && stocks->rows->len < 6; i++) {
    char *symbol = g_strstrip(entries[i]);

    if (*symbol == '\0')
      continue;

    StockRow *row = g_new0(StockRow, 1);
    GtkWidget *line = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
    GtkWidget *name = widget_label(symbol, "whelk-widget-stock-symbol", 0);

    row->symbol = g_ascii_strup(symbol, -1);
    row->price = widget_label("—", "whelk-widget-stock-price", 1.0);
    row->change = widget_label("", "whelk-widget-stock-change", 1.0);

    gtk_widget_set_hexpand(name, TRUE);
    gtk_box_append(GTK_BOX(line), name);
    gtk_box_append(GTK_BOX(line), row->price);
    gtk_box_append(GTK_BOX(line), row->change);
    gtk_widget_set_halign(row->change, GTK_ALIGN_END);
    gtk_box_append(GTK_BOX(widget->card), line);
    g_ptr_array_add(stocks->rows, row);
  }

  g_strfreev(entries);

  stocks->footnote = widget_label("", "whelk-widget-footnote", 0);
  gtk_box_append(GTK_BOX(widget->card), stocks->footnote);

  widget->data = stocks;
  widget->update = stocks_update;
  widget->destroy = stocks_destroy;
  widget->interval = 300;
  return TRUE;
}

static void stocks_destroy(WidgetInstance *widget) {
  StocksWidget *stocks = widget->data;

  if (stocks) {
    g_clear_pointer(&stocks->rows, g_ptr_array_unref);
    g_free(stocks);
  }

  widget->data = NULL;
}

/* =========================================================================
 * CUSTOM
 * ========================================================================= */

typedef struct {
  GtkWidget *body;
} CustomWidget;

static void custom_apply(WidgetInstance *widget, const char *text) {
  CustomWidget *custom = widget->data;
  const char *value = (text && *text) ? text : widget_option(
      widget, "empty", "No output");

  gtk_label_set_text(GTK_LABEL(custom->body), value);
}

static void custom_update(WidgetInstance *widget) {
  const char *command = widget_option(widget, "exec", NULL);

  if (!command || *command == '\0') {
    custom_apply(widget, "Set #:exec on this widget.");
    return;
  }

  widget_command(widget, command, custom_apply);
}

static gboolean custom_build(WidgetInstance *widget) {
  CustomWidget *custom = g_new0(CustomWidget, 1);

  gtk_box_append(GTK_BOX(widget->card),
                 widget_header(widget, "◌", "Widget"));
  custom->body = widget_label("", "whelk-widget-note", 0);
  gtk_label_set_wrap(GTK_LABEL(custom->body), TRUE);
  gtk_label_set_wrap_mode(GTK_LABEL(custom->body), PANGO_WRAP_WORD_CHAR);
  gtk_label_set_ellipsize(GTK_LABEL(custom->body), PANGO_ELLIPSIZE_END);
  gtk_label_set_lines(GTK_LABEL(custom->body), 8);
  gtk_label_set_yalign(GTK_LABEL(custom->body), 0);
  gtk_widget_set_vexpand(custom->body, TRUE);
  gtk_box_append(GTK_BOX(widget->card), custom->body);

  widget->data = custom;
  widget->update = custom_update;
  widget->destroy = custom_destroy;
  widget->interval = (guint)MAX(5, widget_option_int(widget, "interval", 300));
  return TRUE;
}

static void custom_destroy(WidgetInstance *widget) {
  g_free(widget->data);
  widget->data = NULL;
}

/* =========================================================================
 * FACTORIES AND THE MANAGER
 * ========================================================================= */

static const struct {
  const char *kind;
  WidgetBuildFunc build;
} widget_factories[] = {
    {"clock", clock_build},       {"calendar", calendar_build},
    {"weather", weather_build},   {"battery", battery_build},
    {"notes", notes_build},       {"reminders", reminders_build},
    {"world-clock", world_clock_build},
    {"stocks", stocks_build},     {"custom", custom_build},
};

static WidgetBuildFunc widget_factory_for(const char *kind) {
  for (guint i = 0; i < G_N_ELEMENTS(widget_factories); i++) {
    if (g_strcmp0(widget_factories[i].kind, kind) == 0)
      return widget_factories[i].build;
  }

  return NULL;
}

static GdkMonitor *widget_monitor(const char *connector) {
  if (!connector || *connector == '\0' || g_strcmp0(connector, "default") == 0)
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
      return monitor;

    g_clear_object(&monitor);
  }

  return NULL;
}

static WidgetInstance *widget_instance_new(GtkApplication *app,
                                           const WhelkWidgetConfig *config,
                                           const WhelkBarConfig *style,
                                           const char *state_path,
                                           guint index) {
  WidgetInstance *widget = g_new0(WidgetInstance, 1);

  widget->app = app;
  widget->style = style;
  widget->config = whelk_widget_config_copy(config);
  widget->state_path = g_strdup(state_path);
  widget->widget_id =
      g_strdup_printf("widget-%s-%u", config->spec->kind, index);
  widget->guard = widget_guard_new();

  widget->card = gtk_box_new(GTK_ORIENTATION_VERTICAL, 9);
  gtk_widget_add_css_class(widget->card, "whelk-widget-card");
  gtk_widget_set_size_request(widget->card, config->width, config->height);
  gtk_widget_set_halign(widget->card, GTK_ALIGN_FILL);
  gtk_widget_set_valign(widget->card, GTK_ALIGN_FILL);

  /* The whole mouse vocabulary a bar module has: primary, middle and right
   * clicks, and the two scroll directions, all read from the same option
   * names.  A widget with none configured is still draggable by its grip. */
  whelk_click_attach(widget->card, config->spec);

  g_autofree char *kind_class =
      g_strdup_printf("whelk-widget-%s", config->spec->kind);

  gtk_widget_add_css_class(widget->card, kind_class);

  const char *tooltip = whelk_spec_str(config->spec, "tooltip", NULL);

  if (tooltip && *tooltip)
    gtk_widget_set_tooltip_text(widget->card, tooltip);

  widget->display = gdk_display_get_default();

  if (widget->display)
    g_object_ref(widget->display);

  widget->surface = whelk_surface_new(app, widget->widget_id);

  whelk_surface_set_layer(widget->surface, GTK_LAYER_SHELL_LAYER_BOTTOM);
  whelk_surface_set_keyboard_mode(widget->surface,
                                  GTK_LAYER_SHELL_KEYBOARD_MODE_NONE);
  whelk_surface_set_exclusive(widget->surface, FALSE);
  whelk_surface_set_anchors(widget->surface, TRUE, FALSE, FALSE, TRUE);
  whelk_surface_set_margins(widget->surface, config->y, 0, 0, config->x);
  whelk_surface_set_namespace(widget->surface, "whelk-widget");

  g_autofree char *css_id = whelk_style_widget_id(widget->widget_id);

  whelk_surface_set_widget_id(widget->surface, css_id);
  gtk_widget_set_name(widget->card, css_id);
  widget->skin = whelk_style_apply(
      css_id, style, "whelk-widget-card",
      whelk_spec_str(config->spec, "css", NULL));

  whelk_surface_set_child(widget->surface, widget->card);

  GdkMonitor *monitor = widget_monitor(config->monitor);

  if (monitor) {
    whelk_surface_set_monitor(widget->surface, monitor);
    g_object_unref(monitor);
  }

  /* A dragged position wins over the settings file.  It is clamped again here
   * because the monitor may have been resized or replaced since it was saved,
   * and a card that is off-screen after a hotplug cannot be dragged back. */
  int x = widget->config->x;
  int y = widget->config->y;

  if (whelk_widget_state_load_position(widget->state_path, widget->widget_id,
                                       &x, &y)) {
    int area_width = 0;
    int area_height = 0;

    widget_monitor_area(widget, &area_width, &area_height);
    whelk_widget_clamp(&x, &y, widget->config->width, widget->config->height,
                       area_width, area_height);
    widget->config->x = x;
    widget->config->y = y;
  }

  whelk_surface_set_position(widget->surface, widget->config->x,
                             widget->config->y);

  return widget;
}

static void widget_instance_free(WidgetInstance *widget) {
  if (!widget)
    return;

  widget_guard_kill(widget->guard);

  if (widget->timer_id != 0)
    g_source_remove(widget->timer_id);

  if (widget->cancel)
    g_cancellable_cancel(widget->cancel);

  if (widget->destroy)
    widget->destroy(widget);

  /* whelk_style_detach() removes the provider from the display and frees the
   * caller's reference; clearing the field after it would be a second unref
   * of the same object, which is what made a reload crash once a widget had
   * been built. */
  if (widget->skin && widget->display) {
    whelk_style_detach(widget->skin, widget->display);
    widget->skin = NULL;
  }

  g_clear_object(&widget->surface);
  g_clear_object(&widget->cancel);
  g_clear_object(&widget->display);
  g_clear_pointer(&widget->config, whelk_widget_config_free);
  g_clear_pointer(&widget->guard, widget_guard_unref);
  g_free(widget->widget_id);
  g_free(widget->state_path);
  g_free(widget);
}

static gboolean widget_instance_present(WidgetInstance *widget) {
  WidgetBuildFunc build = widget_factory_for(widget->config->spec->kind);

  if (!build) {
    LOG_C_WARN("Widget", "unknown desktop widget kind '%s'",
               widget->config->spec->kind);
    return FALSE;
  }

  if (!build(widget))
    return FALSE;

  if (widget->update)
    widget->update(widget);

  whelk_surface_present(widget->surface);

  if (widget->interval > 0)
    widget->timer_id = g_timeout_add_seconds(widget->interval, widget_tick,
                                             widget);

  return TRUE;
}

WhelkWidgets *whelk_widgets_new(GtkApplication *app, const char *config_path) {
  WhelkWidgets *widgets = g_new0(WhelkWidgets, 1);

  widgets->app = app;
  widgets->state_path = whelk_widget_state_path(config_path);
  widgets->instances = g_ptr_array_new_with_free_func(
      (GDestroyNotify)widget_instance_free);

  return widgets;
}

void whelk_widgets_build(WhelkWidgets *widgets, const GPtrArray *configs,
                         const WhelkBarConfig *style) {
  if (!widgets)
    return;

  g_ptr_array_set_size(widgets->instances, 0);

  if (!configs || !style)
    return;

  for (guint i = 0; i < configs->len; i++) {
    const WhelkWidgetConfig *config = g_ptr_array_index(configs, i);
    WidgetInstance *widget = widget_instance_new(
        widgets->app, config, style, widgets->state_path, i);

    if (!widget_instance_present(widget)) {
      widget_instance_free(widget);
      continue;
    }

    g_ptr_array_add(widgets->instances, widget);
  }

  LOG_C_INFO("Widget", "%u desktop widget(s) up", widgets->instances->len);
}

guint whelk_widgets_count(WhelkWidgets *widgets) {
  return widgets ? widgets->instances->len : 0;
}

void whelk_widgets_free(WhelkWidgets *widgets) {
  if (!widgets)
    return;

  g_clear_pointer(&widgets->instances, g_ptr_array_unref);
  g_free(widgets->state_path);
  g_free(widgets);
}
