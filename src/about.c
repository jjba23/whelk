// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "about.h"

#include <stdio.h>
#include <sys/statvfs.h>
#include <sys/utsname.h>

#include "logger.h"
#include "style.h"
#include "util/session.h"

/* =========================================================================
 * WHAT THE MACHINE SAYS ABOUT ITSELF
 * =========================================================================
 * The same sources `about` of every other desktop reads: /etc/os-release,
 * /proc/cpuinfo, /proc/meminfo, /proc/uptime, uname and statvfs.  Read once,
 * when the window opens, because a window that says "memory: 31.2 GiB" should
 * say what memory is there now, not what it was at login.
 */

static char *os_pretty_name(void) {
  g_autofree char *name = g_get_os_info(G_OS_INFO_KEY_PRETTY_NAME);

  if (name && *name)
    return g_steal_pointer(&name);

  return g_strdup("GNU/Linux");
}

static char *cpu_model(void) {
  static const char *const keys[] = {"model name", "Model", "Hardware",
                                     "Processor", "cpu model", NULL};
  g_autofree char *contents = NULL;

  if (!g_file_get_contents("/proc/cpuinfo", &contents, NULL, NULL))
    return NULL;

  g_auto(GStrv) lines = g_strsplit(contents, "\n", -1);

  for (guint i = 0; lines[i]; i++) {
    g_auto(GStrv) pair = g_strsplit(lines[i], ":", 2);

    if (!pair[0] || !pair[1])
      continue;

    g_strstrip(pair[0]);

    for (guint k = 0; keys[k]; k++) {
      if (g_ascii_strcasecmp(pair[0], keys[k]) == 0) {
        g_strstrip(pair[1]);

        if (*pair[1])
          return g_strdup(pair[1]);
      }
    }
  }

  return NULL;
}

static char *memory_total(void) {
  g_autofree char *contents = NULL;

  if (!g_file_get_contents("/proc/meminfo", &contents, NULL, NULL))
    return NULL;

  g_auto(GStrv) lines = g_strsplit(contents, "\n", -1);

  for (guint i = 0; lines[i]; i++) {
    guint64 kibibytes = 0;

    if (sscanf(lines[i], "MemTotal: %" G_GUINT64_FORMAT " kB", &kibibytes) ==
            1 &&
        kibibytes > 0)
      return g_strdup_printf("%.1f GiB",
                             (double)kibibytes / (1024.0 * 1024.0));
  }

  return NULL;
}

static char *disk_summary(void) {
  struct statvfs stats;

  if (statvfs("/", &stats) != 0)
    return NULL;

  const guint64 total = (guint64)stats.f_blocks * stats.f_frsize;
  const guint64 free = (guint64)stats.f_bavail * stats.f_frsize;
  const guint64 used = total > free ? total - free : 0;

  return g_strdup_printf("%.0f GiB free of %.0f GiB", (double)free / 1073741824.0,
                         (double)(used + free) / 1073741824.0);
}

static char *kernel_release(void) {
  struct utsname name;

  if (uname(&name) != 0)
    return NULL;

  return g_strdup_printf("%s (%s)", name.release, name.machine);
}

static char *uptime_summary(void) {
  g_autofree char *contents = NULL;

  if (!g_file_get_contents("/proc/uptime", &contents, NULL, NULL))
    return NULL;

  double seconds = 0.0;

  if (sscanf(contents, "%lf", &seconds) != 1)
    return NULL;

  const guint64 minutes = (guint64)(seconds / 60.0);
  const guint64 days = minutes / (60 * 24);
  const guint64 hours = (minutes / 60) % 24;

  if (days > 0)
    return g_strdup_printf("%" G_GUINT64_FORMAT " day%s, %" G_GUINT64_FORMAT
                           " hour%s",
                           days, days == 1 ? "" : "s", hours,
                           hours == 1 ? "" : "s");

  if (minutes >= 60)
    return g_strdup_printf("%" G_GUINT64_FORMAT " hour%s, %" G_GUINT64_FORMAT
                           " minute%s",
                           hours, hours == 1 ? "" : "s", minutes % 60,
                           (minutes % 60) == 1 ? "" : "s");

  return g_strdup_printf("%" G_GUINT64_FORMAT " minute%s", minutes,
                         minutes == 1 ? "" : "s");
}

static char *session_summary(void) {
  const char *desktop = g_getenv("XDG_CURRENT_DESKTOP");

  if (desktop && *desktop)
    return g_strdup_printf("%s, %s", whelk_session_name(whelk_session_detect()),
                           desktop);

  return g_strdup(whelk_session_name(whelk_session_detect()));
}

/* =========================================================================
 * THE WINDOW
 * ========================================================================= */

typedef struct {
  GString *rows; /* "Key: value" lines, for the copy button */
} About;

static void about_free(About *about) {
  if (!about)
    return;

  g_string_free(about->rows, TRUE);
  g_free(about);
}

static void add_row(GtkGrid *grid, int *row, GString *lines, const char *key,
                    const char *value) {
  const char *text = (value && *value) ? value : "—";
  GtkWidget *name = gtk_label_new(key);
  GtkWidget *reading = gtk_label_new(text);

  gtk_widget_add_css_class(name, "whelk-about-key");
  gtk_label_set_xalign(GTK_LABEL(name), 0.0);
  gtk_widget_set_halign(name, GTK_ALIGN_START);

  gtk_widget_add_css_class(reading, "whelk-about-value");
  gtk_label_set_xalign(GTK_LABEL(reading), 1.0);
  gtk_widget_set_halign(reading, GTK_ALIGN_END);
  gtk_label_set_selectable(GTK_LABEL(reading), TRUE);
  gtk_label_set_ellipsize(GTK_LABEL(reading), PANGO_ELLIPSIZE_END);
  gtk_widget_set_hexpand(reading, TRUE);

  gtk_grid_attach(grid, name, 0, *row, 1, 1);
  gtk_grid_attach(grid, reading, 1, *row, 1, 1);
  (*row)++;

  g_string_append_printf(lines, "%s: %s\n", key, text);
}

static void on_copy_clicked(GtkButton *button, gpointer user_data) {
  (void)button;

  About *about = user_data;
  GdkClipboard *clipboard = gdk_display_get_clipboard(gdk_display_get_default());

  gdk_clipboard_set_text(clipboard, about->rows->str);
}

static gboolean on_about_key(GtkEventControllerKey *controller, guint keyval,
                             guint keycode, GdkModifierType state,
                             gpointer user_data) {
  (void)controller;
  (void)keycode;
  (void)state;

  if (keyval == GDK_KEY_Escape) {
    gtk_window_close(GTK_WINDOW(user_data));
    return TRUE;
  }

  return FALSE;
}

static void on_about_destroy(GtkWidget *window, GtkCssProvider *skin) {
  (void)window;

  whelk_style_detach(skin, gdk_display_get_default());
}

void whelk_about_show(const WhelkBarConfig *style) {
  g_autofree char *host = g_strdup(g_get_host_name());
  g_autofree char *os = os_pretty_name();
  g_autofree char *cpu = cpu_model();
  g_autofree char *memory = memory_total();
  g_autofree char *disk = disk_summary();
  g_autofree char *kernel = kernel_release();
  g_autofree char *session = session_summary();
  g_autofree char *uptime = uptime_summary();

  GtkWidget *window = gtk_window_new();
  GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 18);
  GtkWidget *header = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 14);
  GtkWidget *titles = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);
  GtkWidget *badge = gtk_image_new_from_icon_name("computer-symbolic");
  GtkWidget *name = gtk_label_new(host && *host ? host : "This System");
  GtkWidget *system = gtk_label_new(os);
  GtkWidget *grid = gtk_grid_new();
  GtkWidget *footer = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
  GtkWidget *version = gtk_label_new(NULL);
  GtkWidget *copy = gtk_button_new_with_label("Copy");
  GtkWidget *close = gtk_button_new_with_label("Close");
  About *about = g_new0(About, 1);
  int row = 0;

  about->rows = g_string_new(NULL);

  gtk_window_set_title(GTK_WINDOW(window), "About This System");
  gtk_widget_add_css_class(window, "whelk-about-window");
  gtk_window_set_default_size(GTK_WINDOW(window), 460, -1);
  gtk_window_set_resizable(GTK_WINDOW(window), FALSE);

  gtk_widget_set_margin_start(box, 24);
  gtk_widget_set_margin_end(box, 24);
  gtk_widget_set_margin_top(box, 24);
  gtk_widget_set_margin_bottom(box, 20);
  gtk_widget_add_css_class(box, "whelk-about");

  gtk_image_set_pixel_size(GTK_IMAGE(badge), 56);
  gtk_widget_add_css_class(badge, "whelk-about-badge");
  gtk_widget_set_valign(badge, GTK_ALIGN_CENTER);

  gtk_widget_add_css_class(name, "whelk-about-host");
  gtk_label_set_xalign(GTK_LABEL(name), 0.0);
  gtk_widget_add_css_class(system, "whelk-about-os");
  gtk_label_set_xalign(GTK_LABEL(system), 0.0);
  gtk_label_set_ellipsize(GTK_LABEL(system), PANGO_ELLIPSIZE_END);

  gtk_box_append(GTK_BOX(titles), name);
  gtk_box_append(GTK_BOX(titles), system);
  gtk_widget_set_valign(titles, GTK_ALIGN_CENTER);

  gtk_box_append(GTK_BOX(header), badge);
  gtk_box_append(GTK_BOX(header), titles);

  gtk_grid_set_column_spacing(GTK_GRID(grid), 24);
  gtk_grid_set_row_spacing(GTK_GRID(grid), 9);
  gtk_widget_add_css_class(grid, "whelk-about-rows");

  add_row(GTK_GRID(grid), &row, about->rows, "Processor", cpu);
  add_row(GTK_GRID(grid), &row, about->rows, "Memory", memory);
  add_row(GTK_GRID(grid), &row, about->rows, "Disk", disk);
  add_row(GTK_GRID(grid), &row, about->rows, "Kernel", kernel);
  add_row(GTK_GRID(grid), &row, about->rows, "Session", session);
  add_row(GTK_GRID(grid), &row, about->rows, "Uptime", uptime);

  g_autofree char *version_text = g_strdup_printf(
      "whelk %s · GTK %u.%u.%u", PACKAGE_VERSION, gtk_get_major_version(),
      gtk_get_minor_version(), gtk_get_micro_version());

  gtk_label_set_text(GTK_LABEL(version), version_text);
  gtk_widget_add_css_class(version, "whelk-about-footer");
  gtk_label_set_xalign(GTK_LABEL(version), 0.0);
  gtk_widget_set_hexpand(version, TRUE);

  gtk_widget_add_css_class(copy, "whelk-about-action");
  gtk_widget_set_tooltip_text(copy, "Copy this summary to the clipboard");
  gtk_widget_add_css_class(close, "whelk-about-action");
  gtk_widget_add_css_class(close, "suggested-action");

  g_signal_connect(copy, "clicked", G_CALLBACK(on_copy_clicked), about);
  g_signal_connect_swapped(close, "clicked", G_CALLBACK(gtk_window_close),
                           window);

  gtk_box_append(GTK_BOX(footer), version);
  gtk_box_append(GTK_BOX(footer), copy);
  gtk_box_append(GTK_BOX(footer), close);

  gtk_box_append(GTK_BOX(box), header);
  gtk_box_append(GTK_BOX(box), gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));
  gtk_box_append(GTK_BOX(box), grid);
  gtk_box_append(GTK_BOX(box), gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));
  gtk_box_append(GTK_BOX(box), footer);

  gtk_window_set_child(GTK_WINDOW(window), box);

  g_autofree char *widget_id = whelk_style_widget_id("about");

  gtk_widget_set_name(window, widget_id);

  GtkCssProvider *skin = whelk_style_apply(widget_id, style, "whelk-about",
                                           NULL);

  g_signal_connect(window, "destroy", G_CALLBACK(on_about_destroy), skin);

  GtkEventController *keys = gtk_event_controller_key_new();

  g_signal_connect(keys, "key-pressed", G_CALLBACK(on_about_key), window);
  gtk_widget_add_controller(window, keys);

  gtk_window_present(GTK_WINDOW(window));

  /* Both the labels and the copy button borrow @about for the window's life;
   * the window owns it, so it dies with the window. */
  g_object_set_data_full(G_OBJECT(window), "whelk-about", about,
                         (GDestroyNotify)about_free);
}
