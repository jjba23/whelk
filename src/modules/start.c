// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "shell.h"
#include "style.h"
#include "util/command.h"
#include "util/format.h"

/* =========================================================================
 * THE SHELL'S OWN MENU
 * =========================================================================
 * The first thing on the bar, and the only one that is about the shell rather
 * than about the machine or the application in front: a shell's worth of
 * entries -- the launcher, the panels, the settings and the way out -- behind
 * one mark: the whelk the shell is named after, or the glyph the configuration
 * names instead.
 */

#define WHELK_START_ACTION "whelk-start-action"

static void on_menu_action(GtkButton *button, gpointer user_data) {
  (void)user_data;

  const char *action =
      g_object_get_data(G_OBJECT(button), WHELK_START_ACTION);

  GtkWidget *popover =
      gtk_widget_get_ancestor(GTK_WIDGET(button), GTK_TYPE_POPOVER);

  if (popover)
    gtk_popover_popdown(GTK_POPOVER(popover));

  if (!action)
    return;

  WhelkShell *shell = whelk_shell_get_default();

  if (g_strcmp0(action, "launcher") == 0) {
    if (shell)
      whelk_shell_show_launcher(shell, NULL);
    return;
  }

  if (g_strcmp0(action, "network") == 0) {
    if (shell)
      whelk_shell_show_network_panel(shell, "wifi");
    return;
  }

  if (g_strcmp0(action, "notifications") == 0) {
    if (shell)
      whelk_shell_toggle_notifications(shell);
    return;
  }

  /* Everything else is a command line, so "the settings file" or "quit" is a
   * configuration's choice and not a new button in C. */
  whelk_command_spawn(action);
}

static void add_item(GtkWidget *box, const char *label, const char *action) {
  GtkWidget *button = gtk_button_new_with_label(label);

  gtk_widget_add_css_class(button, "flat");
  gtk_widget_add_css_class(button, "whelk-menu-item");
  gtk_widget_set_halign(button, GTK_ALIGN_FILL);
  g_object_set_data_full(G_OBJECT(button), WHELK_START_ACTION,
                         g_strdup(action), g_free);
  g_signal_connect(button, "clicked", G_CALLBACK(on_menu_action), NULL);
  gtk_box_append(GTK_BOX(box), button);
}

/* A menu button's hover text belongs to the button, not to the menu: while the
 * menu is open the tooltip either covers the first entry or follows the
 * pointer down the list.  It is switched off for as long as the popover is up. */
static void on_popover_show(GtkWidget *popover, gpointer user_data) {
  (void)popover;
  gtk_widget_set_has_tooltip(GTK_WIDGET(user_data), FALSE);
}

static void on_popover_hide(GtkWidget *popover, gpointer user_data) {
  (void)popover;
  gtk_widget_set_has_tooltip(GTK_WIDGET(user_data), TRUE);
}

/* =========================================================================
 * THE WHELK MARK
 * =========================================================================
 * The shell wears the shell it is named after.  It is drawn rather than
 * loaded: a mark made from a formula takes the bar's own ink wherever it is
 * drawn, needs no icon theme and no file next to the binary, and stays sharp
 * at whatever size the bar asks for.
 *
 * It is one ribbon spiralling out from the centre.  The radius doubles every
 * turn, the way a whelk's whorls grow, and the ribbon widens with it but never
 * fills the gap it leaves, so the whorl stays showing: the thin end is the
 * spire, the wide end the lip, and what runs between two turns is the whorl.
 */

#define WHELK_MARK_STEPS 240

static void mark_path(cairo_t *cr, double box) {
  const double turns = 3.0, ratio = 2.0, spin = -0.45;
  const double margin = 0.06; /* the mark never touches its edge */
  const double grow = log(ratio) / (2.0 * G_PI);
  const double c = cos(spin), s = sin(spin);
  double px[WHELK_MARK_STEPS + 1], py[WHELK_MARK_STEPS + 1];
  double half[WHELK_MARK_STEPS + 1];
  double ox[WHELK_MARK_STEPS + 1], oy[WHELK_MARK_STEPS + 1];
  double ix[WHELK_MARK_STEPS + 1], iy[WHELK_MARK_STEPS + 1];
  double minx = G_MAXDOUBLE, maxx = -G_MAXDOUBLE;
  double miny = G_MAXDOUBLE, maxy = -G_MAXDOUBLE;
  double span, scale, mid = box / 2.0;

  for (int i = 0; i <= WHELK_MARK_STEPS; i++) {
    double t = (double)i / WHELK_MARK_STEPS;
    double th = t * turns * 2.0 * G_PI;
    double r = exp(grow * th);
    double x = r * cos(th), y = r * sin(th);

    px[i] = x * c - y * s;
    py[i] = x * s + y * c;
    /* Narrow enough to leave the whorl between two turns showing. */
    half[i] = MAX(0.19 * r, 0.26);
  }

  /* Both edges run parallel to the centre line, so the ribbon is a closed
   * polygon: out along one side, back along the other. */
  for (int i = 0; i <= WHELK_MARK_STEPS; i++) {
    int before = MAX(i - 1, 0), after = MIN(i + 1, WHELK_MARK_STEPS);
    double dx = px[after] - px[before], dy = py[after] - py[before];
    double length = hypot(dx, dy);
    double nx = -dy / length, ny = dx / length;

    ox[i] = px[i] + nx * half[i];
    oy[i] = py[i] + ny * half[i];
    ix[i] = px[i] - nx * half[i];
    iy[i] = py[i] - ny * half[i];

    minx = MIN(minx, MIN(ox[i], ix[i]));
    maxx = MAX(maxx, MAX(ox[i], ix[i]));
    miny = MIN(miny, MIN(oy[i], iy[i]));
    maxy = MAX(maxy, MAX(oy[i], iy[i]));
  }

  span = MAX(maxx - minx, maxy - miny);
  scale = box * (1.0 - 2.0 * margin) / span;

  cairo_move_to(cr, mid + (ox[0] - (minx + maxx) / 2.0) * scale,
                mid + (oy[0] - (miny + maxy) / 2.0) * scale);

  for (int i = 1; i <= WHELK_MARK_STEPS; i++)
    cairo_line_to(cr, mid + (ox[i] - (minx + maxx) / 2.0) * scale,
                  mid + (oy[i] - (miny + maxy) / 2.0) * scale);

  for (int i = WHELK_MARK_STEPS; i >= 0; i--)
    cairo_line_to(cr, mid + (ix[i] - (minx + maxx) / 2.0) * scale,
                  mid + (iy[i] - (miny + maxy) / 2.0) * scale);

  cairo_close_path(cr);
}

static void on_draw_mark(GtkDrawingArea *area, cairo_t *cr, int width,
                         int height, gpointer user_data) {
  const WhelkBarConfig *bar = user_data;
  GdkRGBA ink = {0.95, 0.95, 0.96, 1.0};

  (void)height;

  /* The bar's own ink -- the colour its labels and glyphs are set in, from the
   * same configuration field the generated stylesheet uses.  A widget's colour
   * is the theme's, which on a bar painted by its configuration is not the
   * colour anything else on it is drawn in. */
  if (!bar || !bar->foreground || !gdk_rgba_parse(&ink, bar->foreground))
    gtk_widget_get_color(GTK_WIDGET(area), &ink);

  gdk_cairo_set_source_rgba(cr, &ink);
  mark_path(cr, MIN(width, height));
  cairo_fill(cr);
}

/* What the menu button wears: the whelk mark, unless the configuration named a
 * glyph of its own with #:label, which is drawn in the bar's icon font.  The
 * mark is 18 pixels unless #:icon-size says otherwise. */
static GtkWidget *button_child(const WhelkModuleSpec *spec,
                               const WhelkBarConfig *bar) {
  const char *glyph = whelk_spec_str(spec, "label", NULL);

  if (glyph && *glyph != '\0') {
    g_autofree char *markup =
        whelk_icon_markup(glyph, bar ? bar->icon_font : NULL);
    GtkWidget *label = gtk_label_new(NULL);

    gtk_label_set_markup(GTK_LABEL(label), markup);

    return label;
  }

  int pixels = CLAMP(whelk_spec_int(spec, "icon-size", 18), 8, 96);
  GtkWidget *mark = gtk_drawing_area_new();

  gtk_widget_set_size_request(mark, pixels, pixels);
  gtk_widget_set_valign(mark, GTK_ALIGN_CENTER);
  gtk_drawing_area_set_draw_func(GTK_DRAWING_AREA(mark), on_draw_mark,
                                 (gpointer)bar, NULL);

  return mark;
}

WhelkModule *whelk_start_module_new(const WhelkModuleSpec *spec,
                                    const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("start", spec, bar);

  gtk_widget_add_css_class(module->root, "whelk-start");

  GtkWidget *button = gtk_menu_button_new();

  gtk_widget_add_css_class(button, "flat");
  gtk_widget_add_css_class(button, "whelk-start-button");
  gtk_menu_button_set_child(GTK_MENU_BUTTON(button), button_child(spec, bar));
  gtk_widget_set_tooltip_text(button,
                              whelk_spec_str(spec, "tooltip", "whelk"));

  GtkWidget *popover = gtk_popover_new();
  GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);

  add_item(box, "Launcher", "launcher");
  gtk_box_append(GTK_BOX(box),
                 gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));
  add_item(box, "Network", "network");
  add_item(box, "Notifications", "notifications");
  gtk_box_append(GTK_BOX(box),
                 gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));
  add_item(box, "Reload settings", "whelk --reload");
  add_item(box, "Quit whelk", "whelk --quit");

  whelk_style_popover(GTK_POPOVER(popover),
                      module->bar);

  gtk_popover_set_child(GTK_POPOVER(popover), box);
  g_signal_connect(popover, "show", G_CALLBACK(on_popover_show), button);
  g_signal_connect(popover, "hide", G_CALLBACK(on_popover_hide), button);
  gtk_menu_button_set_popover(GTK_MENU_BUTTON(button), popover);

  gtk_box_append(GTK_BOX(module->root), button);

  return module;
}
