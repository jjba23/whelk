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
  GtkWidget *end_more;        /* the chevron for the right row's overflow */
  GtkWidget *end_overflow_box;
  GtkWidget *context_menu;    /* the bar's own right-click menu */
  GPtrArray *modules;   /* WhelkModule* */
  GtkCssProvider *skin; /* this bar's generated CSS */
  GdkDisplay *display;  /* borrowed */
};

G_DEFINE_TYPE(WhelkBar, whelk_bar, G_TYPE_OBJECT)

/* =========================================================================
 * THE SKIN
 * =========================================================================
 * The bar's material is generated from its configuration and scoped to its
 * window id, so two bars can have two skins: the structural stylesheet stays
 * the structure, and this is the one rule that paints a bar.
 */

static char *css_id_for(const char *name) {
  GString *id = g_string_new("whelk-");
  const char *cursor = name ? name : "main";

  for (const char *p = cursor; *p; p++)
    g_string_append_c(id, (g_ascii_isalnum(*p) || *p == '-' || *p == '_')
                              ? *p
                              : '-');

  return g_string_free(id, FALSE);
}

static char *build_skin(const WhelkBarConfig *config, const char *widget_id) {
  GString *css = g_string_new(NULL);

  g_string_append_printf(css, "window#%s .whelk-bar {\n", widget_id);

  if (config->foreground && *config->foreground)
    g_string_append_printf(css, "  color: %s;\n", config->foreground);

  if (config->background && *config->background)
    g_string_append_printf(css, "  background-color: %s;\n",
                           config->background);

  if (config->border_color && *config->border_color)
    g_string_append_printf(css, "  border: 1px solid %s;\n",
                           config->border_color);

  g_string_append_printf(css, "  border-radius: %dpx;\n", config->radius);
  g_string_append_printf(css, "  padding: 0 %dpx;\n", config->padding);

  if (config->font && *config->font)
    g_string_append_printf(css, "  font-family: \"%s\";\n", config->font);

  if (config->font_size && *config->font_size)
    g_string_append_printf(css, "  font-size: %s;\n", config->font_size);

  g_string_append(css, "}\n");

  if (config->icon_font && *config->icon_font) {
    g_string_append_printf(css,
                           "window#%s .whelk-icon {\n  font-family: \"%s\";\n}\n",
                           widget_id, config->icon_font);
  }

  /* The accent is what an active workspace is painted in.  It is the one
   * colour that has to be the palette's rather than the bar's. */
  if (config->accent && *config->accent) {
    g_string_append_printf(
        css,
        "window#%s .whelk-workspace.active {\n  color: %s;\n}\n"
        "window#%s .whelk-battery.charging {\n  color: %s;\n}\n",
        widget_id, config->accent, widget_id, config->accent);
  }

  if (config->css && *config->css) {
    g_string_append(css, "/* The configuration's own rules */\n");
    g_string_append(css, config->css);
    g_string_append_c(css, '\n');
  }

  return g_string_free(css, FALSE);
}

static void apply_skin(WhelkBar *bar) {
  g_autofree char *widget_id = css_id_for(bar->config->name);
  g_autofree char *css = build_skin(bar->config, widget_id);
  g_autoptr(GError) error = NULL;

  bar->skin = gtk_css_provider_new();
  gtk_css_provider_load_from_string(bar->skin, css);

  if (error)
    LOG_C_WARN("Bar", "the skin for '%s' did not load: %s", bar->config->name,
               error->message);

  if (bar->display)
    gtk_style_context_add_provider_for_display(
        bar->display, GTK_STYLE_PROVIDER(bar->skin),
        GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);
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
 * when they no longer fit, leaving a chevron where the rest went.  The modules
 * stay alive behind the chevron, so their timers and clicks still work.
 *
 * This runs once, after the first allocation.  A bar is pinned to a monitor
 * and its modules are stable, so there is nothing to reflow every frame.
 */

#define WHELK_OVERFLOW_BUTTON_WIDTH 28

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
  if (!bar || !bar->end_box || !bar->end_more || !bar->end_overflow_box)
    return;

  const int root_width = gtk_widget_get_width(bar->root);

  /* No allocation yet (a headless test, or a surface the compositor has not
   * mapped): leave the row alone rather than hiding everything. */
  if (root_width <= 1)
    return;

  const int spacing = bar->config->spacing;
  const int used_by_others =
      widget_width(bar->start_box, 0) + widget_width(bar->center_box, 0) +
      2 * bar->config->padding;
  const int available = MAX(0, root_width - used_by_others - 16);

  GPtrArray *modules = g_ptr_array_new();

  for (GtkWidget *child = gtk_widget_get_first_child(bar->end_box); child;
       child = gtk_widget_get_next_sibling(child)) {
    if (child != bar->end_more)
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

  gtk_widget_set_visible(bar->end_more, overflowed);
  g_ptr_array_unref(modules);
}

static gboolean collapse_end_idle(gpointer user_data) {
  collapse_end(user_data);
  return G_SOURCE_REMOVE;
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
  GtkWidget *chevron = gtk_image_new_from_icon_name("pan-down-symbolic");

  gtk_image_set_pixel_size(GTK_IMAGE(chevron), 16);
  gtk_menu_button_set_child(GTK_MENU_BUTTON(bar->end_more), chevron);
  gtk_widget_add_css_class(bar->end_more, "flat");
  gtk_widget_add_css_class(bar->end_more, "whelk-overflow-more");
  gtk_widget_set_tooltip_text(bar->end_more, "More bar items");

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

  if (self->skin && self->display) {
    gtk_style_context_remove_provider_for_display(
        self->display, GTK_STYLE_PROVIDER(self->skin));
  }

  g_clear_object(&self->skin);
  g_clear_object(&self->surface);
  g_clear_object(&self->display);

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
  bar->display = gdk_display_get_default();

  if (bar->display)
    g_object_ref(bar->display);

  whelk_surface_set_edge(bar->surface, bar->config->edge);
  whelk_surface_set_layer(bar->surface, bar->config->layer);
  whelk_surface_set_margins(bar->surface, bar->config->margin_top,
                            bar->config->margin_right,
                            bar->config->margin_bottom,
                            bar->config->margin_left);
  whelk_surface_set_exclusive(bar->surface, bar->config->exclusive);
  whelk_surface_set_monitor(bar->surface, monitor);
  whelk_surface_set_namespace(bar->surface, "whelk-bar");

  g_autofree char *widget_id = css_id_for(bar->config->name);
  whelk_surface_set_widget_id(bar->surface, widget_id);

  apply_skin(bar);
  build_layout(bar);

  note_native_modules(bar->config, bar->config->left);
  note_native_modules(bar->config, bar->config->center);
  note_native_modules(bar->config, bar->config->right);

  add_modules(bar, bar->config->left, bar->start_box);
  add_modules(bar, bar->config->center, bar->center_box);
  add_modules(bar, bar->config->right, bar->end_box);

  /* The chevron belongs after every ordinary right-hand module, and it is
   * created before them because its popover is part of the layout. */
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
