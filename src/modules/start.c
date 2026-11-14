// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "about.h"
#include "guile_bridge.h"
#include "logger.h"
#include "shell.h"
#include "style.h"
#include "util/command.h"
#include "util/format.h"
#include "util/menutree.h"
#include "util/session.h"

#include <gdk/gdkkeysyms.h>

/* =========================================================================
 * THE SHELL'S OWN MENU
 * =========================================================================
 * The first thing on the bar, and the one item that belongs to the machine
 * rather than to the application in front: a menu bar's leftmost menu, where
 * macOS keeps the apple.  Here it wears the whelk the shell is named after.
 *
 * What is in it is configuration.  `(whelk menu)' units -- items, rules and
 * submenus -- are read from the settings file (or from `default-shell-menu'
 * when it wrote none), so the menu is a value a person can rewrite and a
 * reload away from being on screen, not a C table somebody has to recompile.
 *
 * An item whose action begins with `shell:' is something this shell does
 * itself; anything else is a command line, run the way every other click on
 * the bar runs one.  A submenu marked `shell:recent-items' is filled when it
 * opens, because what was opened recently is not known until then.
 */

typedef struct {
  GPtrArray *menus; /* of WhelkMenu: the popover's contents, from Scheme */
} StartData;

/* --- what the shell itself can do ----------------------------------------- */

static void show_about(const WhelkBarConfig *bar) { whelk_about_show(bar); }

/* `shell:NAME', without the prefix. */
static void run_shell_action(const char *name, const WhelkBarConfig *bar) {
  WhelkShell *shell = whelk_shell_get_default();

  if (!name || *name == '\0')
    return;

  if (g_strcmp0(name, "launcher") == 0 || g_strcmp0(name, "search") == 0) {
    if (shell)
      whelk_shell_show_launcher(shell, NULL);
    return;
  }

  if (g_strcmp0(name, "force-quit") == 0) {
    if (shell)
      whelk_shell_show_launcher(shell, "quit");
    return;
  }

  if (g_strcmp0(name, "settings") == 0) {
    if (shell)
      whelk_shell_show_settings(shell);
    return;
  }

  if (g_strcmp0(name, "network") == 0) {
    if (shell)
      whelk_shell_show_network_panel(shell, "wifi");
    return;
  }

  if (g_strcmp0(name, "bluetooth") == 0) {
    if (shell)
      whelk_shell_show_network_panel(shell, "bluetooth");
    return;
  }

  if (g_strcmp0(name, "notifications") == 0) {
    if (shell)
      whelk_shell_toggle_notifications(shell);
    return;
  }

  if (g_strcmp0(name, "dnd") == 0) {
    if (shell)
      whelk_shell_toggle_dnd(shell);
    return;
  }

  if (g_strcmp0(name, "lock") == 0) {
    if (shell)
      whelk_shell_lock(shell);
    return;
  }

  if (g_strcmp0(name, "reload") == 0) {
    if (shell)
      whelk_shell_reload(shell);
    return;
  }

  if (g_strcmp0(name, "about") == 0) {
    show_about(bar);
    return;
  }

  LOG_C_DEBUG("Start", "no shell action named '%s'", name);
}

/* --- a menu, as a popover -------------------------------------------------- */

static void close_ancestor_popover(GtkWidget *widget) {
  GtkWidget *popover = gtk_widget_get_ancestor(widget, GTK_TYPE_POPOVER);

  if (popover)
    gtk_popover_popdown(GTK_POPOVER(popover));
}

static void on_item_clicked(GtkButton *button, gpointer user_data) {
  (void)user_data;

  WhelkMenuEntry *entry = g_object_get_data(G_OBJECT(button), "whelk-entry");
  const char *command = g_object_get_data(G_OBJECT(button), "whelk-command");
  const WhelkBarConfig *bar =
      g_object_get_data(G_OBJECT(button), "whelk-bar");

  close_ancestor_popover(GTK_WIDGET(button));

  /* A row built here (Recent Items) carries its command directly; a row of the
   * configured menu carries the entry it came from. */
  if (!entry) {
    if (command)
      whelk_command_spawn(command);

    return;
  }

  if (!entry->enabled)
    return;

  if (entry->action && g_str_has_prefix(entry->action, "shell:")) {
    run_shell_action(entry->action + 6, bar);
    return;
  }

  if (entry->command)
    whelk_command_spawn(entry->command);
  else if (entry->action)
    whelk_command_spawn(entry->action);
}

static GtkWidget *row_with_accel(const char *label, const char *accel) {
  GtkWidget *row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 24);
  GtkWidget *text = gtk_label_new(label);
  GtkWidget *chord = gtk_label_new(accel);

  gtk_widget_set_hexpand(text, TRUE);
  gtk_widget_set_halign(text, GTK_ALIGN_START);
  gtk_label_set_xalign(GTK_LABEL(text), 0.0f);
  gtk_widget_add_css_class(chord, "whelk-menu-accel");
  gtk_box_append(GTK_BOX(row), text);
  gtk_box_append(GTK_BOX(row), chord);

  return row;
}

static gint compare_recent(gconstpointer a, gconstpointer b) {
  GtkRecentInfo *left = *(GtkRecentInfo *const *)a;
  GtkRecentInfo *right = *(GtkRecentInfo *const *)b;

  /* The GDateTime is the info's own, not a copy: it is read, never freed. */
  return g_date_time_compare(gtk_recent_info_get_modified(right),
                             gtk_recent_info_get_modified(left));
}

/* What was opened recently, from the desktop's own list rather than a file the
 * shell keeps: GTK's recent manager is the same list every other application
 * adds to, so the menu agrees with them without being told. */
static void fill_recent_items(GtkPopover *popover) {
  GtkRecentManager *manager = gtk_recent_manager_get_default();
  GList *items = gtk_recent_manager_get_items(manager);
  GPtrArray *list = g_ptr_array_new();
  GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);
  guint shown = 0;

  for (GList *cursor = items; cursor; cursor = cursor->next)
    g_ptr_array_add(list, cursor->data);

  g_ptr_array_sort(list, compare_recent);

  for (guint i = 0; i < list->len && shown < 12; i++) {
    GtkRecentInfo *info = g_ptr_array_index(list, i);
    const char *uri = gtk_recent_info_get_uri(info);
    const char *name = gtk_recent_info_get_display_name(info);
    g_autofree char *quoted = uri ? g_shell_quote(uri) : NULL;
    g_autofree char *command =
        quoted ? g_strdup_printf("xdg-open %s", quoted) : NULL;
    GtkWidget *button = NULL;

    if (!command)
      continue;

    button = gtk_button_new_with_label(name ? name : uri);
    gtk_widget_add_css_class(button, "flat");
    gtk_widget_add_css_class(button, "whelk-menu-item");
    gtk_widget_set_halign(button, GTK_ALIGN_FILL);
    g_object_set_data_full(G_OBJECT(button), "whelk-command",
                           g_strdup(command), g_free);
    g_signal_connect(button, "clicked", G_CALLBACK(on_item_clicked), NULL);
    gtk_box_append(GTK_BOX(box), button);
    shown++;
  }

  if (shown == 0) {
    GtkWidget *empty = gtk_label_new("No Recent Items");

    gtk_widget_add_css_class(empty, "dim-label");
    gtk_box_append(GTK_BOX(box), empty);
  }

  gtk_popover_set_child(popover, box);
  g_ptr_array_unref(list);
  g_list_free_full(items, (GDestroyNotify)gtk_recent_info_unref);
}

static void on_recent_shown(GtkWidget *popover, gpointer user_data) {
  (void)user_data;
  fill_recent_items(GTK_POPOVER(popover));
}

static void fill_menu(GtkPopover *popover, WhelkMenu *menu,
                      const WhelkBarConfig *bar);

static gboolean on_menu_key(GtkEventControllerKey *controller, guint keyval,
                            guint keycode, GdkModifierType state,
                            gpointer user_data);

/* Every popover of the shell's menu walks with the keyboard, submenus included:
 * a chevron that cannot be opened without a mouse is a menu that has stopped
 * being one. */
static void attach_keys(GtkPopover *popover) {
  GtkEventController *keys = gtk_event_controller_key_new();

  gtk_event_controller_set_propagation_phase(keys, GTK_PHASE_CAPTURE);
  g_signal_connect(keys, "key-pressed", G_CALLBACK(on_menu_key), NULL);
  gtk_widget_add_controller(GTK_WIDGET(popover), keys);
}

static void add_entry_row(GtkWidget *box, WhelkMenuEntry *entry,
                          const WhelkBarConfig *bar) {
  if (entry->kind == WHELK_MENU_SEPARATOR) {
    gtk_box_append(GTK_BOX(box), gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));
    return;
  }

  if (entry->submenu) {
    GtkWidget *button = gtk_menu_button_new();
    GtkWidget *submenu_popover = gtk_popover_new();

    gtk_menu_button_set_has_frame(GTK_MENU_BUTTON(button), FALSE);
    gtk_menu_button_set_label(GTK_MENU_BUTTON(button), entry->label);
    gtk_menu_button_set_direction(GTK_MENU_BUTTON(button), GTK_ARROW_RIGHT);
    gtk_widget_add_css_class(button, "flat");
    gtk_widget_add_css_class(button, "whelk-menu-item");
    gtk_widget_set_halign(button, GTK_ALIGN_FILL);

    whelk_style_popover(GTK_POPOVER(submenu_popover), bar);
    fill_menu(GTK_POPOVER(submenu_popover), entry->submenu, bar);
    attach_keys(GTK_POPOVER(submenu_popover));
    gtk_menu_button_set_popover(GTK_MENU_BUTTON(button), submenu_popover);

    /* A slot the shell fills when it opens: what was opened recently is only
     * known at the moment somebody looks. */
    if (entry->action && g_strcmp0(entry->action, "shell:recent-items") == 0)
      g_signal_connect(submenu_popover, "show", G_CALLBACK(on_recent_shown),
                       NULL);

    g_object_set_data(G_OBJECT(button), "whelk-bar", (gpointer)bar);
    gtk_box_append(GTK_BOX(box), button);
    return;
  }

  GtkWidget *button = gtk_button_new();

  if (entry->accel)
    gtk_button_set_child(GTK_BUTTON(button),
                         row_with_accel(entry->label, entry->accel));
  else
    gtk_button_set_child(GTK_BUTTON(button), gtk_label_new(entry->label));

  gtk_widget_add_css_class(button, "flat");
  gtk_widget_add_css_class(button, "whelk-menu-item");
  gtk_widget_set_halign(button, GTK_ALIGN_FILL);
  gtk_button_set_has_frame(GTK_BUTTON(button), FALSE);

  if (!entry->enabled)
    gtk_widget_set_sensitive(button, FALSE);

  g_object_set_data(G_OBJECT(button), "whelk-entry", entry);
  g_object_set_data(G_OBJECT(button), "whelk-bar", (gpointer)bar);
  g_signal_connect(button, "clicked", G_CALLBACK(on_item_clicked), NULL);
  gtk_box_append(GTK_BOX(box), button);
}

static void fill_menu(GtkPopover *popover, WhelkMenu *menu,
                      const WhelkBarConfig *bar) {
  GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);

  for (guint i = 0; i < menu->entries->len; i++)
    add_entry_row(box, g_ptr_array_index(menu->entries, i), bar);

  gtk_popover_set_child(popover, box);
}

/* --- keyboard -------------------------------------------------------------- */

static gboolean focusable(GtkWidget *child) {
  return child && GTK_IS_BUTTON(child) && gtk_widget_get_sensitive(child);
}

/* A menu is a loop, not a page: Down past the last row is the first, and Up
 * from the first is the last. */
static GtkWidget *neighbour_item(GtkWidget *box, GtkWidget *from,
                                 gboolean down) {
  GPtrArray *items = g_ptr_array_new();
  GtkWidget *target = NULL;
  guint index = 0;
  gboolean found = (from == NULL);

  for (GtkWidget *child = gtk_widget_get_first_child(box); child;
       child = gtk_widget_get_next_sibling(child)) {
    if (!focusable(child))
      continue;

    if (!found && child == from) {
      index = items->len;
      found = TRUE;
    }

    g_ptr_array_add(items, child);
  }

  if (items->len > 0) {
    const guint count = items->len;

    if (from == NULL)
      index = down ? 0 : count - 1;
    else
      index = down ? (index + 1) % count : (index + count - 1) % count;

    target = g_ptr_array_index(items, index);
  }

  g_ptr_array_unref(items);

  return target;
}

static gboolean on_menu_key(GtkEventControllerKey *controller, guint keyval,
                            guint keycode, GdkModifierType state,
                            gpointer user_data) {
  (void)keycode;
  (void)state;
  (void)user_data;

  GtkWidget *popover =
      gtk_event_controller_get_widget(GTK_EVENT_CONTROLLER(controller));
  GtkWidget *box = gtk_popover_get_child(GTK_POPOVER(popover));
  GtkRoot *root = gtk_widget_get_root(popover);
  GtkWidget *focus = root ? gtk_root_get_focus(root) : NULL;

  if (focus && !gtk_widget_is_ancestor(focus, popover))
    focus = NULL;

  if (keyval == GDK_KEY_Escape) {
    gtk_popover_popdown(GTK_POPOVER(popover));
    return TRUE;
  }

  /* Right opens the row's submenu, the way a menubar's Right does; the submenu
   * carries the same controller, so it walks on its own. */
  if (keyval == GDK_KEY_Right && focus && GTK_IS_MENU_BUTTON(focus)) {
    GtkPopover *submenu = gtk_menu_button_get_popover(GTK_MENU_BUTTON(focus));

    if (submenu) {
      gtk_popover_popup(submenu);
      return TRUE;
    }
  }

  if (!box || (keyval != GDK_KEY_Down && keyval != GDK_KEY_Up))
    return FALSE;

  gtk_widget_grab_focus(
      neighbour_item(box, focus, keyval == GDK_KEY_Down));

  return TRUE;
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

/* --- lifecycle ------------------------------------------------------------- */

static void start_destroy(WhelkModule *module) {
  StartData *data = module->data;

  if (!data)
    return;

  g_clear_pointer(&data->menus, g_ptr_array_unref);
  g_free(data);
}

static WhelkMenu *first_menu(StartData *data) {
  return (data->menus && data->menus->len > 0)
             ? g_ptr_array_index(data->menus, 0)
             : NULL;
}

/* The menu a shell with no Scheme still has: the few ways in that are true
 * whatever the configuration says.  A mark that opens nothing is worse than a
 * short menu. */
static WhelkMenu *fallback_menu(void) {
  WhelkMenu *menu = whelk_menu_new("shell");
  WhelkMenuEntry *entry = NULL;

  entry = whelk_menu_add_entry(menu, "Search…");
  entry->action = g_strdup("shell:launcher");
  entry = whelk_menu_add_entry(menu, "Notifications");
  entry->action = g_strdup("shell:notifications");
  whelk_menu_add_separator(menu);
  entry = whelk_menu_add_entry(menu, "Reload settings");
  entry->command = g_strdup("whelk --reload");
  entry = whelk_menu_add_entry(menu, "Quit whelk");
  entry->command = g_strdup("whelk --quit");

  return menu;
}

WhelkModule *whelk_start_module_new(const WhelkModuleSpec *spec,
                                    const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("start", spec, bar);
  StartData *data = g_new0(StartData, 1);

  gtk_widget_add_css_class(module->root, "whelk-start");

  data->menus = whelk_bridge_shell_menu(NULL);
  module->data = data;
  module->destroy = start_destroy;

  GtkWidget *button = gtk_menu_button_new();

  gtk_widget_add_css_class(button, "flat");
  gtk_widget_add_css_class(button, "whelk-start-button");
  gtk_menu_button_set_child(GTK_MENU_BUTTON(button), button_child(spec, bar));
  gtk_widget_set_tooltip_text(button,
                              whelk_spec_str(spec, "tooltip", "whelk"));

  GtkWidget *popover = gtk_popover_new();
  WhelkMenu *menu = first_menu(data);

  whelk_style_popover(GTK_POPOVER(popover), bar);

  if (menu) {
    fill_menu(GTK_POPOVER(popover), menu, bar);
  } else {
    /* No Scheme came up: the shell still has to offer its own ways in. */
    WhelkMenu *fallback = fallback_menu();

    fill_menu(GTK_POPOVER(popover), fallback, bar);
    whelk_menu_free(fallback);
  }

  gtk_menu_button_set_popover(GTK_MENU_BUTTON(button), popover);
  attach_keys(GTK_POPOVER(popover));

  gtk_box_append(GTK_BOX(module->root), button);

  return module;
}
