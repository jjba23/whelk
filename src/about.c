// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "about.h"

#include "logger.h"
#include "shell.h"
#include "style.h"
#include "util/hostinfo.h"

/* =========================================================================
 * ABOUT THIS SYSTEM
 * =========================================================================
 * The window behind the shell menu's first item: what machine this is, in the
 * terms a person asks about their own computer -- the name, the system, the
 * processor that is in it, the memory and disk that are left, the display in
 * front of them.  A GtkAboutDialog would only say the program's name; this
 * says the machine's, which is what the menu item promises.
 *
 * The window is deliberately opaque.  The bar and its popovers are glass
 * because the desktop behind them is the point; a system report is text, and
 * text over a wallpaper is the one thing an About window must not be.  The
 * palette still comes from the bar, so it belongs to the same desktop.
 */

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

static void copy_summary(About *about) {
  GdkClipboard *clipboard;

  if (!about)
    return;

  clipboard = gdk_display_get_clipboard(gdk_display_get_default());
  gdk_clipboard_set_text(clipboard, about->rows->str);
}

static void on_copy_clicked(GtkButton *button, gpointer user_data) {
  (void)button;

  copy_summary(user_data);
}

static void on_settings_clicked(GtkButton *button, gpointer user_data) {
  WhelkShell *shell = whelk_shell_get_default();

  (void)button;

  if (!shell)
    return;

  gtk_window_close(GTK_WINDOW(user_data));
  whelk_shell_show_settings(shell);
}

static gboolean on_about_key(GtkEventControllerKey *controller, guint keyval,
                             guint keycode, GdkModifierType state,
                             gpointer user_data) {
  GtkWindow *window = user_data;

  (void)controller;
  (void)keycode;

  if (keyval == GDK_KEY_Escape) {
    gtk_window_close(window);
    return TRUE;
  }

  /* Ctrl+C copies the report, the way it copies any other text on screen. */
  if ((state & GDK_CONTROL_MASK) &&
      (keyval == GDK_KEY_c || keyval == GDK_KEY_C)) {
    copy_summary(g_object_get_data(G_OBJECT(window), "whelk-about"));
    return TRUE;
  }

  return FALSE;
}

static void on_about_destroy(GtkWidget *window, GtkCssProvider *skin) {
  (void)window;

  whelk_style_detach(skin, gdk_display_get_default());
}

/* The window paints itself, so its content box must not paint a second frame
 * inside the decoration's.  A translucent panel inside an opaque window is what
 * read as a glitch, and a border inside a border never lines up. */
static char *window_css(const char *widget_id, const WhelkBarConfig *style) {
  GString *css = g_string_new(NULL);
  g_autofree char *solid =
      whelk_style_solid(style ? style->background : NULL);

  g_string_append_printf(css,
                         "window#%s headerbar {\n"
                         "  background: transparent;\n"
                         "  border: none;\n"
                         "  box-shadow: none;\n"
                         "}\n"
                         "window#%s .whelk-about {\n"
                         "  background: none;\n"
                         "  border: none;\n"
                         "  box-shadow: none;\n"
                         "  border-radius: 0;\n"
                         "  padding: 0;\n"
                         "}\n",
                         widget_id, widget_id);

  if (solid && *solid)
    g_string_append_printf(css,
                           "window#%s {\n  background-color: %s;\n}\n",
                           widget_id, solid);

  if (style && style->foreground && *style->foreground)
    g_string_append_printf(css, "window#%s {\n  color: %s;\n}\n", widget_id,
                           style->foreground);

  return g_string_free(css, FALSE);
}

void whelk_about_show(const WhelkBarConfig *style) {
  g_autofree char *host = g_strdup(g_get_host_name());
  g_autofree char *os = whelk_host_os_name();
  g_autofree char *cpu = whelk_host_cpu_summary();
  g_autofree char *memory = whelk_host_memory_summary();
  g_autofree char *display = whelk_host_display_summary();
  g_autofree char *disk = whelk_host_disk_summary();
  g_autofree char *kernel = whelk_host_kernel_release();
  g_autofree char *session = whelk_host_session_summary();
  g_autofree char *uptime = whelk_host_uptime_summary();

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
  GtkWidget *settings = gtk_button_new_with_label("Settings…");
  GtkWidget *copy = gtk_button_new_with_label("Copy");
  GtkWidget *close = gtk_button_new_with_label("Close");
  About *about = g_new0(About, 1);
  int row = 0;

  about->rows = g_string_new(NULL);

  gtk_window_set_title(GTK_WINDOW(window), "About This System");
  gtk_window_set_icon_name(GTK_WINDOW(window), "org.jointhefreeworld.whelk");
  gtk_widget_add_css_class(window, "whelk-about-window");
  gtk_window_set_default_size(GTK_WINDOW(window), 560, -1);
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

  /* The copy is a report, so it opens with what the window is about. */
  g_string_append_printf(about->rows, "%s\n%s\n\n",
                         host && *host ? host : "This System",
                         os && *os ? os : "GNU/Linux");

  add_row(GTK_GRID(grid), &row, about->rows, "Processor", cpu);
  add_row(GTK_GRID(grid), &row, about->rows, "Memory", memory);
  add_row(GTK_GRID(grid), &row, about->rows, "Display", display);
  add_row(GTK_GRID(grid), &row, about->rows, "Disk", disk);
  add_row(GTK_GRID(grid), &row, about->rows, "Kernel", kernel);
  add_row(GTK_GRID(grid), &row, about->rows, "Session", session);
  add_row(GTK_GRID(grid), &row, about->rows, "Uptime", uptime);

  g_autofree char *version_text = g_strdup_printf(
      "whelk %s · GTK %u.%u.%u · GLib %u.%u.%u", PACKAGE_VERSION,
      gtk_get_major_version(), gtk_get_minor_version(),
      gtk_get_micro_version(), glib_major_version, glib_minor_version,
      glib_micro_version);

  gtk_label_set_text(GTK_LABEL(version), version_text);
  gtk_widget_add_css_class(version, "whelk-about-footer");
  gtk_label_set_xalign(GTK_LABEL(version), 0.0);
  gtk_widget_set_hexpand(version, TRUE);

  gtk_widget_add_css_class(settings, "whelk-about-action");
  gtk_widget_set_tooltip_text(settings, "Open whelk's settings");
  gtk_widget_add_css_class(copy, "whelk-about-action");
  gtk_widget_set_tooltip_text(copy,
                              "Copy this report to the clipboard (Ctrl+C)");
  gtk_widget_add_css_class(close, "whelk-about-action");
  gtk_widget_add_css_class(close, "suggested-action");

  g_signal_connect(copy, "clicked", G_CALLBACK(on_copy_clicked), about);
  g_signal_connect(settings, "clicked", G_CALLBACK(on_settings_clicked), window);
  g_signal_connect_swapped(close, "clicked", G_CALLBACK(gtk_window_close),
                           window);

  gtk_box_append(GTK_BOX(footer), version);
  gtk_box_append(GTK_BOX(footer), settings);
  gtk_box_append(GTK_BOX(footer), copy);
  gtk_box_append(GTK_BOX(footer), close);

  gtk_box_append(GTK_BOX(box), header);
  gtk_box_append(GTK_BOX(box), gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));
  gtk_box_append(GTK_BOX(box), grid);
  gtk_box_append(GTK_BOX(box), gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));
  gtk_box_append(GTK_BOX(box), footer);

  gtk_window_set_child(GTK_WINDOW(window), box);

  g_autofree char *widget_id = whelk_style_widget_id("about");
  g_autofree char *css = window_css(widget_id, style);

  gtk_widget_set_name(window, widget_id);

  GtkCssProvider *skin =
      whelk_style_apply(widget_id, style, "whelk-about", css);

  g_signal_connect(window, "destroy", G_CALLBACK(on_about_destroy), skin);

  /* The key handler and the copy button borrow @about for the window's life;
   * the window owns it, so it dies with the window. */
  g_object_set_data_full(G_OBJECT(window), "whelk-about", about,
                         (GDestroyNotify)about_free);

  GtkEventController *keys = gtk_event_controller_key_new();

  g_signal_connect(keys, "key-pressed", G_CALLBACK(on_about_key), window);
  gtk_widget_add_controller(window, keys);

  gtk_window_present(GTK_WINDOW(window));
}
