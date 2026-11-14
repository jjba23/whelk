// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "bar.h"

#include "logger.h"
#include "module.h"
#include "shell.h"
#include "style.h"

struct _WhelkBar {
  GObject parent_instance;

  WhelkBarConfig *config; /* owned copy */
  WhelkSurface *surface;
  GtkWidget *root;      /* .whelk-bar, the material */
  GtkWidget *start_box; /* module rows */
  GtkWidget *center_box;
  GtkWidget *end_box;
  GtkWidget *end_more;        /* the +N count for a row that does not fit */
  GtkWidget *end_more_count;  /* its label, updated by collapse_end() */
  GtkWidget *end_overflow_box;
  GtkWidget *overflow_box;    /* the configured overflow area, inline */
  GtkWidget *overflow_toggle; /* the chevron that opens it */
  GtkWidget *overflow_icon;
  GtkWidget *context_menu;    /* the bar's own right-click menu */
  GPtrArray *modules;   /* WhelkModule* */
  GtkCssProvider *skin; /* this bar's generated CSS */
};

G_DEFINE_TYPE(WhelkBar, whelk_bar, G_TYPE_OBJECT)

/* The bar's material is generated from its configuration and scoped to its
 * window id, so two bars can wear two skins. */
static void apply_skin(WhelkBar *bar, const char *widget_id) {
  bar->skin = whelk_style_apply(widget_id, bar->config, "whelk-bar",
                                bar->config->css);
}

/* =========================================================================
 * BUILDING
 * ========================================================================= */

/* Marks what this bar reads natively, before any module is built: the tray is
 * usually built before or after the Wi-Fi module depending on the row it is in,
 * and it must not have to care. */
static void note_native_modules(WhelkBarConfig *config, GPtrArray *specs) {
  for (guint i = 0; specs && i < specs->len; i++) {
    const WhelkModuleSpec *spec = g_ptr_array_index(specs, i);

    if (g_strcmp0(spec->kind, "wifi") == 0)
      config->native_network = TRUE;
    else if (g_strcmp0(spec->kind, "bluetooth") == 0)
      config->native_bluetooth = TRUE;
  }
}

static void add_modules(WhelkBar *bar, GPtrArray *specs, GtkWidget *box) {
  for (guint i = 0; i < specs->len; i++) {
    const WhelkModuleSpec *spec = g_ptr_array_index(specs, i);
    WhelkModule *module = whelk_module_new(spec, bar->config);

    if (!module)
      continue;

    gtk_box_append(GTK_BOX(box), module->root);
    g_ptr_array_add(bar->modules, module);
  }
}

/* =========================================================================
 * THE RIGHT ROW'S OVERFLOW
 * =========================================================================
 * A bar has a fixed width and a growing set of readings.  The left and centre
 * keep their room; the right row folds its trailing modules into a popover
 * when they no longer fit, leaving a "+3" where the rest went.  The modules
 * stay alive behind that count, so their timers and clicks still work.
 *
 * The count is not a chevron on purpose: the shelf at the end of the row has
 * the row's one chevron, and a number also says how many readings are missing.
 *
 * This runs once, after the first allocation.  A bar is pinned to a monitor
 * and its modules are stable, so there is nothing to reflow every frame.
 */

/* The fold's label can reach two digits ("+14"), so the room kept for it is a
 * little wider than the chevron it replaced was. */
#define WHELK_OVERFLOW_BUTTON_WIDTH 34

static int widget_width(GtkWidget *widget, int fallback) {
  if (!widget || !gtk_widget_get_visible(widget))
    return 0;

  int width = gtk_widget_get_width(widget);

  if (width <= 0)
    gtk_widget_measure(widget, GTK_ORIENTATION_HORIZONTAL, -1, NULL, &width,
                       NULL, NULL);

  return width > 0 ? width : fallback;
}

static void collapse_end(WhelkBar *bar) {
  if (!bar || !bar->end_box || !bar->end_more || !bar->end_more_count ||
      !bar->end_overflow_box)
    return;

  const int root_width = gtk_widget_get_width(bar->root);

  /* No allocation yet (a headless test, or a surface the compositor has not
   * mapped): leave the row alone rather than hiding everything. */
  if (root_width <= 1)
    return;

  const int spacing = bar->config->spacing;
  const int used_by_others =
      widget_width(bar->start_box, 0) + widget_width(bar->center_box, 0) +
      widget_width(bar->overflow_box, 0) +
      widget_width(bar->overflow_toggle, 0) + 2 * bar->config->padding;
  const int available = MAX(0, root_width - used_by_others - 16);

  GPtrArray *modules = g_ptr_array_new();

  for (GtkWidget *child = gtk_widget_get_first_child(bar->end_box); child;
       child = gtk_widget_get_next_sibling(child)) {
    /* The count and the shelf are the row's structure, not readings: never
     * fold them. */
    if (child != bar->end_more && child != bar->overflow_box &&
        child != bar->overflow_toggle)
      g_ptr_array_add(modules, child);
  }

  if (modules->len == 0) {
    g_ptr_array_unref(modules);
    return;
  }

  int used = 0;
  guint visible = 0;

  for (guint i = 0; i < modules->len; i++) {
    GtkWidget *widget = g_ptr_array_index(modules, i);
    const int width = widget_width(widget, 24);
    const int gap = visible > 0 ? spacing : 0;
    const gboolean more_after = i + 1 < modules->len;
    const int reserve = more_after ? WHELK_OVERFLOW_BUTTON_WIDTH + spacing : 0;

    if (used + gap + width + reserve > available)
      break;

    used += gap + width;
    visible++;
  }

  /* Keep the first reading even when it alone is wider than the room: an
   * empty right row is worse than a clipped clock. */
  if (visible == 0)
    visible = 1;

  for (guint i = visible; i < modules->len; i++) {
    GtkWidget *widget = g_ptr_array_index(modules, i);

    if (gtk_widget_get_parent(widget) == bar->end_box)
      gtk_box_remove(GTK_BOX(bar->end_box), widget);

    gtk_box_append(GTK_BOX(bar->end_overflow_box), widget);
  }

  const gboolean overflowed = visible < modules->len;

  if (overflowed) {
    const guint folded = modules->len - visible;
    g_autofree char *count = g_strdup_printf("+%u", folded);
    g_autofree char *tip =
        g_strdup_printf("%u reading%s did not fit · click to see %s", folded,
                        folded == 1 ? "" : "s", folded == 1 ? "it" : "them");

    gtk_label_set_text(GTK_LABEL(bar->end_more_count), count);
    gtk_widget_set_tooltip_text(bar->end_more, tip);
  }

  gtk_widget_set_visible(bar->end_more, overflowed);
  g_ptr_array_unref(modules);
}

static gboolean collapse_end_idle(gpointer user_data) {
  collapse_end(user_data);
  return G_SOURCE_REMOVE;
}

/* =========================================================================
 * THE CONFIGURED OVERFLOW
 * =========================================================================
 * Some readings are reference rather than status: the machine's load, the ways
 * into its own tools, the brightness that is set once in a while.  They are
 * named by `#:overflow` and live behind the row's one chevron, laid out only
 * while it is open.  Their modules are built with the bar, so
 * opening the area costs a layout pass and nothing restarts.
 */

static void update_overflow(WhelkBar *bar) {
  if (!bar || !bar->overflow_box || !bar->overflow_toggle)
    return;

  const gboolean expanded =
      gtk_toggle_button_get_active(GTK_TOGGLE_BUTTON(bar->overflow_toggle));

  gtk_widget_set_visible(bar->overflow_box, expanded);

  if (bar->overflow_icon)
    gtk_image_set_from_icon_name(GTK_IMAGE(bar->overflow_icon),
                                 expanded ? "pan-end-symbolic"
                                          : "pan-start-symbolic");

  gtk_widget_set_tooltip_text(bar->overflow_toggle,
                              expanded ? "Extra items · click to hide"
                                       : "Extra items · click to show");
}

static void on_overflow_toggled(GtkToggleButton *toggle, gpointer user_data) {
  (void)toggle;
  update_overflow(user_data);
}

static void build_overflow(WhelkBar *bar) {
  if (!bar->config || bar->config->overflow->len == 0)
    return;

  const GtkOrientation orientation = whelk_surface_is_horizontal(bar->surface)
                                         ? GTK_ORIENTATION_HORIZONTAL
                                         : GTK_ORIENTATION_VERTICAL;

  bar->overflow_box = gtk_box_new(orientation, bar->config->spacing);
  gtk_widget_add_css_class(bar->overflow_box, "whelk-overflow");
  gtk_widget_set_valign(bar->overflow_box, GTK_ALIGN_CENTER);
  gtk_box_append(GTK_BOX(bar->end_box), bar->overflow_box);
  add_modules(bar, bar->config->overflow, bar->overflow_box);

  bar->overflow_toggle = gtk_toggle_button_new();
  bar->overflow_icon = gtk_image_new_from_icon_name("pan-start-symbolic");
  gtk_image_set_pixel_size(GTK_IMAGE(bar->overflow_icon), 16);
  gtk_button_set_child(GTK_BUTTON(bar->overflow_toggle), bar->overflow_icon);
  gtk_widget_add_css_class(bar->overflow_toggle, "flat");
  gtk_widget_add_css_class(bar->overflow_toggle, "whelk-overflow-toggle");
  gtk_widget_set_valign(bar->overflow_toggle, GTK_ALIGN_CENTER);
  g_signal_connect(bar->overflow_toggle, "toggled",
                   G_CALLBACK(on_overflow_toggled), bar);
  gtk_box_append(GTK_BOX(bar->end_box), bar->overflow_toggle);

  gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(bar->overflow_toggle),
                               bar->config->overflow_expanded);
  update_overflow(bar);
}

/* =========================================================================
 * THE BAR'S RIGHT-CLICK MENU
 * =========================================================================
 * Right-clicking the bar is the shell's own context menu: a way into the
 * settings window and a reload that does not require remembering a socket
 * command.  The menu is parented to the bar itself, so it follows the surface
 * to whatever edge the configuration put it on.
 */

static void on_edit_settings_clicked(GtkButton *button, gpointer user_data) {
  WhelkShell *shell = whelk_shell_get_default();

  (void)button;
  (void)user_data;

  if (shell)
    whelk_shell_show_settings(shell);
}

static void on_reload_settings_clicked(GtkButton *button, gpointer user_data) {
  WhelkShell *shell = whelk_shell_get_default();

  (void)button;
  (void)user_data;

  if (shell)
    whelk_shell_reload(shell);
}

static void build_context_menu(WhelkBar *bar) {
  GtkWidget *popover = gtk_popover_new();
  GtkWidget *menu = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);
  GtkWidget *heading = gtk_label_new("whelk");
  GtkWidget *edit = gtk_button_new_with_label("Edit settings…");
  GtkWidget *reload = gtk_button_new_with_label("Reload settings");

  gtk_widget_add_css_class(menu, "whelk-bar-menu");
  gtk_widget_add_css_class(heading, "whelk-bar-menu-heading");
  gtk_label_set_xalign(GTK_LABEL(heading), 0.0f);
  gtk_widget_set_margin_start(heading, 8);
  gtk_widget_set_margin_end(heading, 8);
  gtk_widget_set_margin_top(heading, 4);
  gtk_widget_set_margin_bottom(heading, 4);
  gtk_widget_add_css_class(edit, "flat");
  gtk_widget_add_css_class(reload, "flat");
  gtk_widget_set_halign(edit, GTK_ALIGN_START);
  gtk_widget_set_halign(reload, GTK_ALIGN_START);
  g_signal_connect(edit, "clicked", G_CALLBACK(on_edit_settings_clicked),
                   bar);
  g_signal_connect(reload, "clicked",
                   G_CALLBACK(on_reload_settings_clicked), bar);
  gtk_box_append(GTK_BOX(menu), heading);
  gtk_box_append(GTK_BOX(menu), edit);
  gtk_box_append(GTK_BOX(menu), reload);

  gtk_popover_set_autohide(GTK_POPOVER(popover), TRUE);
  gtk_popover_set_has_arrow(GTK_POPOVER(popover), FALSE);
  gtk_popover_set_child(GTK_POPOVER(popover), menu);
  whelk_style_popover(GTK_POPOVER(popover), bar->config);
  gtk_widget_set_parent(popover, bar->root);
  bar->context_menu = popover;
}

static void on_bar_right_click(GtkGestureClick *gesture, int n_press,
                               double x, double y, gpointer user_data) {
  WhelkBar *bar = user_data;
  GdkRectangle point = {(int)x, (int)y, 1, 1};

  (void)gesture;
  (void)n_press;

  if (!bar->context_menu)
    return;

  /* A right-click on a module belongs to that module: the clock copies the
   * time, the bell toggles do-not-disturb, and so on.  Only the bar's own
   * gaps and padding open the shell menu. */
  GtkWidget *target = gtk_widget_pick(bar->root, x, y, GTK_PICK_DEFAULT);

  if (target && target != bar->root && target != bar->start_box &&
      target != bar->center_box && target != bar->end_box)
    return;

  gtk_popover_set_pointing_to(GTK_POPOVER(bar->context_menu), &point);
  gtk_popover_popup(GTK_POPOVER(bar->context_menu));
}

/* A horizontal bar is a centre box, so its centre is the bar's centre rather
 * than the middle of whatever the two sides leave over.  A vertical bar is a
 * column of the same three groups. */
static void build_layout(WhelkBar *bar) {
  const gboolean horizontal = whelk_surface_is_horizontal(bar->surface);
  const int spacing = bar->config->spacing;

  bar->start_box = gtk_box_new(horizontal ? GTK_ORIENTATION_HORIZONTAL
                                          : GTK_ORIENTATION_VERTICAL,
                               spacing);
  bar->center_box = gtk_box_new(horizontal ? GTK_ORIENTATION_HORIZONTAL
                                           : GTK_ORIENTATION_VERTICAL,
                                spacing);
  bar->end_box = gtk_box_new(horizontal ? GTK_ORIENTATION_HORIZONTAL
                                        : GTK_ORIENTATION_VERTICAL,
                             spacing);

  gtk_widget_set_valign(bar->start_box, GTK_ALIGN_CENTER);
  gtk_widget_set_valign(bar->center_box, GTK_ALIGN_CENTER);
  gtk_widget_set_valign(bar->end_box, GTK_ALIGN_CENTER);
  gtk_widget_set_halign(bar->start_box, GTK_ALIGN_START);
  gtk_widget_set_halign(bar->end_box, GTK_ALIGN_END);

  bar->end_overflow_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 4);
  gtk_widget_add_css_class(bar->end_overflow_box, "whelk-overflow-box");

  bar->end_more = gtk_menu_button_new();
  bar->end_more_count = gtk_label_new("+1");
  gtk_widget_add_css_class(bar->end_more_count, "whelk-folded-count");
  gtk_menu_button_set_child(GTK_MENU_BUTTON(bar->end_more),
                            bar->end_more_count);
  gtk_widget_add_css_class(bar->end_more, "flat");
  gtk_widget_add_css_class(bar->end_more, "whelk-overflow-more");
  gtk_widget_set_tooltip_text(bar->end_more, "Readings that did not fit");

  GtkWidget *popover = gtk_popover_new();

  whelk_style_popover(GTK_POPOVER(popover), bar->config);
  gtk_popover_set_child(GTK_POPOVER(popover), bar->end_overflow_box);
  gtk_menu_button_set_popover(GTK_MENU_BUTTON(bar->end_more), popover);
  gtk_widget_set_visible(bar->end_more, FALSE);

  if (horizontal) {
    bar->root = gtk_center_box_new();
    gtk_center_box_set_start_widget(GTK_CENTER_BOX(bar->root), bar->start_box);
    gtk_center_box_set_center_widget(GTK_CENTER_BOX(bar->root),
                                     bar->center_box);
    gtk_center_box_set_end_widget(GTK_CENTER_BOX(bar->root), bar->end_box);
  } else {
    bar->root = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_box_append(GTK_BOX(bar->root), bar->start_box);
    gtk_widget_set_vexpand(bar->center_box, TRUE);
    gtk_box_append(GTK_BOX(bar->root), bar->center_box);
    gtk_box_append(GTK_BOX(bar->root), bar->end_box);
  }

  gtk_widget_add_css_class(bar->root, "whelk-bar");
  gtk_widget_set_size_request(bar->root,
                              horizontal ? -1 : bar->config->height,
                              horizontal ? bar->config->height : -1);

  build_context_menu(bar);

  GtkGesture *right_click = gtk_gesture_click_new();
  gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(right_click),
                                GDK_BUTTON_SECONDARY);
  g_signal_connect(right_click, "pressed", G_CALLBACK(on_bar_right_click),
                   bar);
  gtk_widget_add_controller(bar->root, GTK_EVENT_CONTROLLER(right_click));
}

/* The thickness a bar really needs.
 *
 * A module is laid out flat whatever the bar's orientation, so a vertical bar
 * with a clock in it is wider than the configured height: the height is the
 * floor, the widest module is the truth.  The exclusive zone has to be that
 * same number, or the compositor reserves less than the bar covers and tiled
 * windows slide under it. */
static int bar_thickness(WhelkBar *bar) {
  const GtkOrientation orientation = whelk_surface_is_horizontal(bar->surface)
                                         ? GTK_ORIENTATION_VERTICAL
                                         : GTK_ORIENTATION_HORIZONTAL;
  int minimum = 0;

  gtk_widget_measure(bar->root, orientation, -1, &minimum, NULL, NULL, NULL);

  return MAX(bar->config->height, minimum);
}

static void whelk_bar_dispose(GObject *object) {
  WhelkBar *self = WHELK_BAR(object);

  if (self->modules) {
    g_ptr_array_set_size(self->modules, 0);
    g_clear_pointer(&self->modules, g_ptr_array_unref);
  }

  if (self->skin)
    whelk_style_detach(self->skin, gdk_display_get_default());

  self->skin = NULL;
  g_clear_object(&self->surface);

  G_OBJECT_CLASS(whelk_bar_parent_class)->dispose(object);
}

static void whelk_bar_finalize(GObject *object) {
  WhelkBar *self = WHELK_BAR(object);

  g_clear_pointer(&self->config, whelk_bar_config_free);

  G_OBJECT_CLASS(whelk_bar_parent_class)->finalize(object);
}

static void whelk_bar_class_init(WhelkBarClass *klass) {
  GObjectClass *object_class = G_OBJECT_CLASS(klass);

  object_class->dispose = whelk_bar_dispose;
  object_class->finalize = whelk_bar_finalize;
}

static void whelk_bar_init(WhelkBar *self) {
  self->modules = g_ptr_array_new_with_free_func(
      (GDestroyNotify)whelk_module_free);
}

WhelkBar *whelk_bar_new(GtkApplication *app, const WhelkBarConfig *config,
                        GdkMonitor *monitor) {
  WhelkBar *bar = g_object_new(WHELK_TYPE_BAR, NULL);

  bar->config = whelk_bar_config_copy(config);
  bar->surface = whelk_surface_new(app, bar->config->name);

  whelk_surface_set_edge(bar->surface, bar->config->edge);
  whelk_surface_set_layer(bar->surface, bar->config->layer);
  whelk_surface_set_margins(bar->surface, bar->config->margin_top,
                            bar->config->margin_right,
                            bar->config->margin_bottom,
                            bar->config->margin_left);
  whelk_surface_set_exclusive(bar->surface, bar->config->exclusive);
  whelk_surface_set_monitor(bar->surface, monitor);
  whelk_surface_set_namespace(bar->surface, "whelk-bar");

  g_autofree char *widget_id = whelk_style_widget_id(bar->config->name);
  whelk_surface_set_widget_id(bar->surface, widget_id);

  apply_skin(bar, widget_id);
  build_layout(bar);

  note_native_modules(bar->config, bar->config->left);
  note_native_modules(bar->config, bar->config->center);
  note_native_modules(bar->config, bar->config->right);
  note_native_modules(bar->config, bar->config->overflow);

  add_modules(bar, bar->config->left, bar->start_box);
  add_modules(bar, bar->config->center, bar->center_box);
  add_modules(bar, bar->config->right, bar->end_box);
  build_overflow(bar);

  gtk_box_append(GTK_BOX(bar->end_box), bar->end_more);

  whelk_surface_set_child(bar->surface, bar->root);
  whelk_surface_set_thickness(bar->surface, bar_thickness(bar));
  whelk_surface_present(bar->surface);

  /* The first allocation is the first moment the row's real widths are known. */
  g_idle_add_full(G_PRIORITY_DEFAULT_IDLE, collapse_end_idle,
                  g_object_ref(bar), (GDestroyNotify)g_object_unref);

  LOG_C_INFO("Bar", "'%s' on the %s edge: %u module(s)", bar->config->name,
             whelk_edge_name(bar->config->edge), bar->modules->len);

  return bar;
}

GtkWidget *whelk_bar_widget(WhelkBar *bar) {
  return bar ? bar->root : NULL;
}

const char *whelk_bar_name(WhelkBar *bar) {
  return bar && bar->config ? bar->config->name : "";
}
