// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include <gio/gio.h>
#include <string.h>

#include "logger.h"
#include "style.h"
#include "util/applets.h"
#include "util/dbusmenu.h"
#include "util/menuaction.h"
#include "util/menutree.h"
#include "util/trayicon.h"

/* =========================================================================
 * TRAY
 * =========================================================================
 * Other programs' status icons, over the StatusNotifierItem protocol: the only
 * tray a Wayland client can host, and the one Waybar's `tray` module speaks.
 *
 * The watcher owns the list.  This module watches org.kde.StatusNotifierWatcher
 * on the session bus, asks it for RegisteredStatusNotifierItems, and follows
 * the Registered/Unregistered signals from then on; every item is a GDBusProxy of
 * its own, with its icon read from the proxy's cached properties and a left
 * click calling Activate.  An item's own menu lives on com.canonical.dbusmenu:
 * a right click (and a left click when the item says its menu is its primary
 * action) opens it in a popover of whelk's own.  The applet is told first --
 * AboutToShow -- and then the tree is read, because that is the order the
 * protocol keeps ids valid in; the layout reader is the one the global menu
 * uses.  An item with no Menu path still gets ContextMenu, which is all the
 * protocol offers it.
 */

typedef struct TrayState TrayState;

typedef struct {
  TrayState *state;
  char *entry;
  char *service;
  char *path;
  GDBusProxy *proxy;
  GtkWidget *image;

  /* An item's menu, opened lazily and kept for the next right click.  The tree
   * is owned here because the rows hold pointers into it; it is freed only
   * after the popover that drew it has lost its child. */
  GtkWidget *menu;
  WhelkDBusMenuRequest *menu_request;
  GPtrArray *menu_tree;
} TrayItem;

struct TrayState {
  GDBusConnection *bus;
  GDBusProxy *watcher;
  guint watcher_id;
  GPtrArray *items;
  GtkWidget *box;
  GtkWidget *more;
  GtkWidget *overflow_box;
  guint max_items;

  /* App directories already added to GTK's icon search path. */
  GHashTable *icon_paths;

  /* "id, title fragments to leave out", as configured; and the two applets a
   * bar with its own Wi-Fi and Bluetooth readings does not need. */
  char *hide;
  gboolean hide_network;
  gboolean hide_bluetooth;

  WhelkBarConfig *style;
};

static void tray_add_entry(TrayState *state, const char *entry);
static void tray_reflow(TrayState *state);

/* =========================================================================
 * WHAT NOT TO SHOW
 * =========================================================================
 * A bar that reads the network itself does not also need an applet that reads
 * it, and two controls for one radio is how a user loses confidence in both.
 * The rule is the bar's: it knows which modules it built, and it says so in
 * WhelkBarConfig.native_network / native_bluetooth (src/bar.c).
 *
 * An explicit #:hide overrides the automatic list: a configuration that names
 * what it wants hidden is not second-guessed.
 *
 * The automatic list names applets by their StatusNotifierItem **Id**, matched
 * whole and case-insensitively.  Matching title words instead was too eager:
 * it hid a tray item whose tooltip happened to mention "network", and a bar
 * that hides an icon it was not asked to hide looks broken.  An item whose id
 * is not one of these applets stays, whatever its title says.
 */

/* A configured #:hide names words, not ids: it looks for them anywhere. */
static gboolean contains_fragment(const char *text, const char *fragments) {
  if (!text || !fragments)
    return FALSE;

  g_auto(GStrv) parts = g_strsplit(fragments, ",", -1);

  for (guint i = 0; parts[i]; i++) {
    const char *fragment = g_strstrip(parts[i]);

    if (*fragment == '\0')
      continue;

    g_autofree char *lower_text = g_ascii_strdown(text, -1);
    g_autofree char *lower_fragment = g_ascii_strdown(fragment, -1);

    if (strstr(lower_text, lower_fragment))
      return TRUE;
  }

  return FALSE;
}

static gboolean tray_should_hide(TrayState *state, const char *id,
                                 const char *title) {
  /* A configured fragment still looks at the title too: #:hide is the user's
   * own rule, and they asked for words, not ids. */
  if (state->hide && *state->hide)
    return contains_fragment(id, state->hide) ||
           contains_fragment(title, state->hide);

  if (state->hide_network && whelk_applet_is_network(id))
    return TRUE;

  if (state->hide_bluetooth && whelk_applet_is_bluetooth(id))
    return TRUE;

  return FALSE;
}

/* =========================================================================
 * ITEMS
 * ========================================================================= */

/* The first MAX_ITEMS icons stay on the bar; the rest live behind the
 * chevron.  The widgets are the same either way, so a click still reaches the
 * item wherever it is drawn. */
static void tray_reflow(TrayState *state) {
  if (!state || !state->box || !state->overflow_box)
    return;

  const guint visible = MIN(state->items->len, state->max_items);

  for (guint i = 0; i < state->items->len; i++) {
    TrayItem *item = g_ptr_array_index(state->items, i);
    GtkWidget *target = i < visible ? state->box : state->overflow_box;
    GtkWidget *parent = gtk_widget_get_parent(item->image);

    if (parent == target)
      continue;

    if (parent)
      gtk_box_remove(GTK_BOX(parent), item->image);

    gtk_box_append(GTK_BOX(target), item->image);
  }

  /* Whatever moved, the chevron belongs after every visible icon. */
  if (gtk_widget_get_parent(state->more) == state->box)
    gtk_box_reorder_child_after(GTK_BOX(state->box), state->more, NULL);

  gtk_widget_set_visible(state->more, state->items->len > state->max_items);
}

static void tray_item_free(gpointer data) {
  TrayItem *item = data;

  if (!item)
    return;

  if (item->menu_request) {
    whelk_dbusmenu_request_free(item->menu_request);
    item->menu_request = NULL;
  }

  if (item->menu) {
    if (gtk_widget_get_parent(item->menu))
      gtk_widget_unparent(item->menu);

    item->menu = NULL;
  }

  g_clear_pointer(&item->menu_tree, g_ptr_array_unref);

  if (item->image && gtk_widget_get_parent(item->image))
    gtk_widget_unparent(item->image);

  g_clear_object(&item->proxy);
  g_free(item->entry);
  g_free(item->service);
  g_free(item->path);
  g_free(item);
}

/* Builds a paintable out of an item's ARGB32 pixmap ("a(iiay)"). */
static GdkPaintable *paintable_from_pixmaps(GVariant *pixmaps) {
  if (!pixmaps || g_variant_n_children(pixmaps) == 0)
    return NULL;

  g_autoptr(GVariant) first = g_variant_get_child_value(pixmaps, 0);
  gint32 width = 0;
  gint32 height = 0;
  g_autoptr(GVariant) bytes = NULL;

  g_variant_get(first, "(ii@ay)", &width, &height, &bytes);

  if (width <= 0 || height <= 0)
    return NULL;

  const gsize expected = (gsize)width * (gsize)height * 4;

  if (g_variant_get_size(bytes) < expected)
    return NULL;

  /* The protocol's pixmap is ARGB32 in network byte order. */
  const guchar *data = g_variant_get_data(bytes);
  guchar *rgba = g_malloc(expected);

  for (gsize i = 0; i < (gsize)width * (gsize)height; i++) {
    rgba[i * 4 + 0] = data[i * 4 + 1]; /* R */
    rgba[i * 4 + 1] = data[i * 4 + 2]; /* G */
    rgba[i * 4 + 2] = data[i * 4 + 3]; /* B */
    rgba[i * 4 + 3] = data[i * 4 + 0]; /* A */
  }

  GBytes *shared = g_bytes_new_take(rgba, expected);
  GdkTexture *texture = gdk_memory_texture_new(
      width, height, GDK_MEMORY_R8G8B8A8_PREMULTIPLIED, shared,
      (gsize)width * 4);

  g_bytes_unref(shared);

  return texture ? GDK_PAINTABLE(texture) : NULL;
}

/* StatusNotifierItem publishes the app's own icon directory separately from
 * IconName.  GTK does not search that directory by default: an item can then
 * resolve a same-named icon from the desktop theme instead -- Tela's
 * "toolbox-tray-color" is a currentColor glyph, which is black on a dark bar.
 * Remember every app directory so a NewIcon signal does not add it forever. */
static void tray_use_icon_path(TrayState *state, const char *path) {
  if (!state || !path || !*path || !g_path_is_absolute(path))
    return;

  if (!state->icon_paths)
    state->icon_paths = g_hash_table_new_full(g_str_hash, g_str_equal, g_free,
                                              NULL);

  if (g_hash_table_contains(state->icon_paths, path))
    return;

  g_hash_table_add(state->icon_paths, g_strdup(path));

  GdkDisplay *display = gdk_display_get_default();

  if (display)
    gtk_icon_theme_add_search_path(gtk_icon_theme_get_for_display(display),
                                   path);
}

static void tray_item_refresh(TrayItem *item) {
  if (!item || !item->proxy)
    return;

  g_autoptr(GVariant) icon_name =
      g_dbus_proxy_get_cached_property(item->proxy, "IconName");
  g_autoptr(GVariant) icon_pixmap =
      g_dbus_proxy_get_cached_property(item->proxy, "IconPixmap");
  g_autoptr(GVariant) icon_theme_path =
      g_dbus_proxy_get_cached_property(item->proxy, "IconThemePath");

  const char *name = icon_name ? g_variant_get_string(icon_name, NULL) : NULL;
  const char *theme_path =
      icon_theme_path ? g_variant_get_string(icon_theme_path, NULL) : NULL;

  tray_use_icon_path(item->state, theme_path);

  if (name && *name) {
    g_autofree char *app_icon = whelk_tray_icon_file(theme_path, name);

    if (app_icon)
      gtk_image_set_from_file(GTK_IMAGE(item->image), app_icon);
    else
      gtk_image_set_from_icon_name(GTK_IMAGE(item->image), name);
  } else {
    GdkPaintable *paintable = paintable_from_pixmaps(icon_pixmap);

    if (paintable)
      gtk_image_set_from_paintable(GTK_IMAGE(item->image), paintable);
    else
      gtk_image_set_from_icon_name(GTK_IMAGE(item->image),
                                   "application-x-executable");
  }

  g_autoptr(GVariant) title =
      g_dbus_proxy_get_cached_property(item->proxy, "Title");
  g_autoptr(GVariant) id = g_dbus_proxy_get_cached_property(item->proxy, "Id");

  const char *label = title && *g_variant_get_string(title, NULL)
                          ? g_variant_get_string(title, NULL)
                          : (id ? g_variant_get_string(id, NULL) : NULL);

  gtk_widget_set_tooltip_text(item->image, label ? label : "");
}

static void tray_item_call(TrayItem *item, const char *method) {
  if (!item || !item->proxy)
    return;

  g_dbus_proxy_call(item->proxy, method, g_variant_new("(ii)", 0, 0),
                    G_DBUS_CALL_FLAGS_NONE, -1, NULL, NULL, NULL);
}

/* =========================================================================
 * THE ITEM'S OWN MENU
 * =========================================================================
 * StatusNotifierItem keeps its menu on com.canonical.dbusmenu, at the path the
 * `Menu' property names.  The menu is built while it is being opened --
 * AboutToShow -- so the read comes second: an id from GetLayout then belongs to
 * the layout the applet just made.  The tree is the same shape the global menu
 * reads, so the rows are the same rows.
 */

static char *tray_menu_path(TrayItem *item) {
  g_autoptr(GVariant) menu =
      g_dbus_proxy_get_cached_property(item->proxy, "Menu");

  if (!menu)
    return NULL;

  const char *path = g_variant_get_string(menu, NULL);

  /* "/" is the object root, which is not a menu any tray item publishes;
   * the property is empty when there is none. */
  if (!path || *path == '\0' || g_strcmp0(path, "/") == 0)
    return NULL;

  return g_strdup(path);
}

static gboolean tray_item_is_menu(TrayItem *item) {
  g_autoptr(GVariant) is_menu =
      g_dbus_proxy_get_cached_property(item->proxy, "ItemIsMenu");

  return is_menu && g_variant_get_boolean(is_menu);
}

static void on_tray_entry_clicked(GtkButton *button, gpointer user_data) {
  WhelkMenuEntry *entry =
      g_object_get_data(G_OBJECT(button), "whelk-tray-entry");

  (void)user_data;

  GtkWidget *popover =
      gtk_widget_get_ancestor(GTK_WIDGET(button), GTK_TYPE_POPOVER);

  if (popover)
    gtk_popover_popdown(GTK_POPOVER(popover));

  whelk_menu_entry_activate(entry, NULL);
}

static GtkWidget *tray_menu_box(WhelkMenu *menu, TrayItem *item);

static GtkWidget *tray_menu_row(WhelkMenuEntry *entry, TrayItem *item) {
  GtkWidget *button = gtk_button_new();

  if (entry->accel) {
    GtkWidget *row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 24);
    GtkWidget *label = gtk_label_new(entry->label);
    GtkWidget *accel = gtk_label_new(entry->accel);

    gtk_widget_set_hexpand(label, TRUE);
    gtk_widget_set_halign(label, GTK_ALIGN_START);
    gtk_label_set_xalign(GTK_LABEL(label), 0.0f);
    gtk_widget_add_css_class(accel, "whelk-menu-accel");
    gtk_box_append(GTK_BOX(row), label);
    gtk_box_append(GTK_BOX(row), accel);
    gtk_button_set_child(GTK_BUTTON(button), row);
  } else {
    gtk_button_set_child(GTK_BUTTON(button), gtk_label_new(entry->label));
  }

  gtk_widget_add_css_class(button, "flat");
  gtk_widget_add_css_class(button, "whelk-menu-item");
  gtk_widget_set_halign(button, GTK_ALIGN_FILL);
  gtk_button_set_has_frame(GTK_BUTTON(button), FALSE);

  if (!entry->enabled)
    gtk_widget_set_sensitive(button, FALSE);

  g_object_set_data(G_OBJECT(button), "whelk-tray-entry", entry);
  g_signal_connect(button, "clicked", G_CALLBACK(on_tray_entry_clicked),
                   item);

  return button;
}

static GtkWidget *tray_menu_box(WhelkMenu *menu, TrayItem *item) {
  GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);

  for (guint i = 0; i < menu->entries->len; i++) {
    WhelkMenuEntry *entry = g_ptr_array_index(menu->entries, i);

    if (entry->kind == WHELK_MENU_SEPARATOR) {
      gtk_box_append(GTK_BOX(box),
                     gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));
      continue;
    }

    if (entry->submenu && entry->submenu->entries->len > 0) {
      GtkWidget *button = gtk_menu_button_new();
      GtkWidget *popover = gtk_popover_new();

      gtk_menu_button_set_has_frame(GTK_MENU_BUTTON(button), FALSE);
      gtk_menu_button_set_label(GTK_MENU_BUTTON(button), entry->label);
      gtk_menu_button_set_direction(GTK_MENU_BUTTON(button),
                                    GTK_ARROW_RIGHT);
      gtk_widget_add_css_class(button, "flat");
      gtk_widget_add_css_class(button, "whelk-menu-item");
      gtk_widget_set_halign(button, GTK_ALIGN_FILL);

      if (item->state->style)
        whelk_style_popover(GTK_POPOVER(popover), item->state->style);

      gtk_popover_set_child(GTK_POPOVER(popover),
                            tray_menu_box(entry->submenu, item));
      gtk_menu_button_set_popover(GTK_MENU_BUTTON(button), popover);
      gtk_box_append(GTK_BOX(box), button);
      continue;
    }

    gtk_box_append(GTK_BOX(box), tray_menu_row(entry, item));
  }

  return box;
}

static GtkWidget *tray_menu_popover(TrayItem *item) {
  if (item->menu)
    return item->menu;

  GtkWidget *popover = gtk_popover_new();

  gtk_widget_set_parent(popover, item->image);
  gtk_popover_set_position(GTK_POPOVER(popover), GTK_POS_BOTTOM);
  gtk_popover_set_has_arrow(GTK_POPOVER(popover), FALSE);

  if (item->state->style)
    whelk_style_popover(GTK_POPOVER(popover), item->state->style);

  item->menu = popover;

  return popover;
}

static void on_tray_menu(GPtrArray *menus, gpointer user_data) {
  TrayItem *item = user_data;
  GtkWidget *popover = tray_menu_popover(item);

  /* The request that called us is done; free it before the tree replaces the
   * rows it fed. */
  whelk_dbusmenu_request_free(item->menu_request);
  item->menu_request = NULL;

  if (!menus) {
    GtkWidget *empty = gtk_label_new("This applet has no menu");

    gtk_widget_add_css_class(empty, "whelk-tray-empty");
    gtk_popover_set_child(GTK_POPOVER(popover), empty);
    g_clear_pointer(&item->menu_tree, g_ptr_array_unref);
    return;
  }

  /* GetLayout's top level is usually one menu holding the entries; a tree
   * with several top levels is drawn as one after another. */
  GtkWidget *box;

  if (menus->len == 1) {
    box = tray_menu_box(g_ptr_array_index(menus, 0), item);
  } else {
    box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);

    for (guint i = 0; i < menus->len; i++)
      gtk_box_append(GTK_BOX(box),
                     tray_menu_box(g_ptr_array_index(menus, i), item));
  }

  /* Replacing the child destroys the rows that pointed into the old tree,
   * and only then is it safe to free it. */
  gtk_popover_set_child(GTK_POPOVER(popover), box);
  g_clear_pointer(&item->menu_tree, g_ptr_array_unref);
  item->menu_tree = menus;

  LOG_C_DEBUG("Tray", "%s publishes %u menu(s)", item->entry, menus->len);
}

static void tray_item_show_menu(TrayItem *item) {
  if (!item || !item->proxy)
    return;

  char *path = tray_menu_path(item);

  if (!path) {
    tray_item_call(item, "ContextMenu");
    return;
  }

  const char *owner = g_dbus_proxy_get_name_owner(item->proxy);
  const char *service = owner ? owner : item->service;

  if (!item->state->bus || !service) {
    g_free(path);
    tray_item_call(item, "ContextMenu");
    return;
  }

  if (item->menu_request) {
    whelk_dbusmenu_request_free(item->menu_request);
    item->menu_request = NULL;
  }

  GtkWidget *popover = tray_menu_popover(item);
  GtkWidget *loading = gtk_label_new("Loading menu…");

  gtk_widget_add_css_class(loading, "whelk-tray-empty");
  gtk_popover_set_child(GTK_POPOVER(popover), loading);
  g_clear_pointer(&item->menu_tree, g_ptr_array_unref);
  gtk_popover_popup(GTK_POPOVER(popover));

  item->menu_request = whelk_dbusmenu_fetch(
      item->state->bus, service, path, WHELK_CHORD_CUA, on_tray_menu, item);

  g_free(path);
}

static void on_item_click(GtkGestureClick *gesture, int n_press, double x,
                          double y, gpointer user_data) {
  (void)gesture;
  (void)n_press;
  (void)x;
  (void)y;

  TrayItem *item = user_data;

  /* An item whose menu is its primary action opens it on a left click too,
   * the way a tray icon with a menu and no window does. */
  if (tray_item_is_menu(item))
    tray_item_show_menu(item);
  else
    tray_item_call(item, "Activate");
}

static void on_item_right_click(GtkGestureClick *gesture, int n_press, double x,
                                double y, gpointer user_data) {
  (void)gesture;
  (void)n_press;
  (void)x;
  (void)y;

  tray_item_show_menu(user_data);
}

static void on_item_properties(GDBusProxy *proxy, GVariant *changed,
                               GStrv invalidated, gpointer user_data) {
  (void)proxy;
  (void)changed;
  (void)invalidated;

  tray_item_refresh(user_data);
}

static void on_item_signal(GDBusProxy *proxy, const char *sender_name,
                           const char *signal_name, GVariant *parameters,
                           gpointer user_data) {
  (void)proxy;
  (void)sender_name;
  (void)parameters;

  if (g_strcmp0(signal_name, "NewIcon") == 0 ||
      g_strcmp0(signal_name, "NewTitle") == 0 ||
      g_strcmp0(signal_name, "NewToolTip") == 0 ||
      g_strcmp0(signal_name, "NewStatus") == 0)
    tray_item_refresh(user_data);
}

/* Splits "service:/object/path" into its two halves; a bare bus name means the
 * protocol's default object path. */
static void split_entry(const char *entry, char **service, char **path) {
  const char *slash = strchr(entry, '/');

  if (slash) {
    *service = g_strndup(entry, (gsize)(slash - entry));
    *path = g_strdup(slash);
  } else {
    *service = g_strdup(entry);
    *path = g_strdup("/StatusNotifierItem");
  }
}

typedef struct {
  TrayState *state;
  char *entry;
} ItemRequest;

static void item_request_free(ItemRequest *request) {
  g_free(request->entry);
  g_free(request);
}

static void on_item_proxy_ready(GObject *source, GAsyncResult *result,
                                gpointer user_data) {
  ItemRequest *request = user_data;
  g_autoptr(GError) error = NULL;
  GDBusProxy *proxy =
      g_dbus_proxy_new_finish(result, &error);

  (void)source;

  if (!proxy) {
    LOG_C_DEBUG("Tray", "item %s is not reachable: %s", request->entry,
                error->message);
    item_request_free(request);
    return;
  }

  /* Ids are what the protocol says to match on; a Title is a fallback for the
   * applets that only fill that in. */
  g_autoptr(GVariant) item_id =
      g_dbus_proxy_get_cached_property(proxy, "Id");
  g_autoptr(GVariant) item_title =
      g_dbus_proxy_get_cached_property(proxy, "Title");
  const char *id = item_id ? g_variant_get_string(item_id, NULL) : NULL;
  const char *title = item_title ? g_variant_get_string(item_title, NULL) : NULL;

  if (tray_should_hide(request->state, id, title)) {
    LOG_C_INFO("Tray", "hiding '%s': whelk reads this itself",
               id ? id : (title ? title : request->entry));
    g_object_unref(proxy);
    item_request_free(request);
    return;
  }

  TrayItem *item = g_new0(TrayItem, 1);
  item->state = request->state;
  item->entry = g_strdup(request->entry);
  split_entry(request->entry, &item->service, &item->path);
  item->proxy = proxy;
  item->image = gtk_image_new();

  gtk_widget_add_css_class(item->image, "whelk-tray-item");
  gtk_image_set_pixel_size(GTK_IMAGE(item->image), 16);

  GtkGesture *left = gtk_gesture_click_new();
  gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(left), GDK_BUTTON_PRIMARY);
  g_signal_connect(left, "pressed", G_CALLBACK(on_item_click), item);

  GtkGesture *right = gtk_gesture_click_new();
  gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(right), GDK_BUTTON_SECONDARY);
  g_signal_connect(right, "pressed", G_CALLBACK(on_item_right_click), item);

  gtk_widget_add_controller(item->image, GTK_EVENT_CONTROLLER(left));
  gtk_widget_add_controller(item->image, GTK_EVENT_CONTROLLER(right));

  g_signal_connect(proxy, "g-properties-changed",
                   G_CALLBACK(on_item_properties), item);
  g_signal_connect(proxy, "g-signal", G_CALLBACK(on_item_signal), item);

  tray_item_refresh(item);

  g_ptr_array_add(request->state->items, item);
  gtk_box_append(GTK_BOX(request->state->box), item->image);
  tray_reflow(request->state);

  item_request_free(request);
}

/* =========================================================================
 * THE WATCHER
 * ========================================================================= */

static TrayItem *find_item(TrayState *state, const char *entry) {
  for (guint i = 0; i < state->items->len; i++) {
    TrayItem *item = g_ptr_array_index(state->items, i);

    if (g_strcmp0(item->entry, entry) == 0)
      return item;
  }

  return NULL;
}

static void tray_add_entry(TrayState *state, const char *entry) {
  if (!state || !entry || *entry == '\0' || find_item(state, entry))
    return;

  char *service = NULL;
  char *path = NULL;

  split_entry(entry, &service, &path);

  ItemRequest *request = g_new0(ItemRequest, 1);
  request->state = state;
  request->entry = g_strdup(entry);

  g_dbus_proxy_new(state->bus, G_DBUS_PROXY_FLAGS_NONE, NULL, service, path,
                   "org.kde.StatusNotifierItem", NULL, on_item_proxy_ready,
                   request);

  g_free(service);
  g_free(path);
}

static void tray_remove_entry(TrayState *state, const char *entry) {
  for (guint i = 0; i < state->items->len; i++) {
    TrayItem *item = g_ptr_array_index(state->items, i);

    if (g_strcmp0(item->entry, entry) != 0)
      continue;

    g_ptr_array_remove_index(state->items, i);
    tray_reflow(state);
    return;
  }
}

static void on_watcher_signal(GDBusProxy *proxy, const char *sender_name,
                              const char *signal_name, GVariant *parameters,
                              gpointer user_data) {
  (void)proxy;
  (void)sender_name;

  TrayState *state = user_data;
  const char *entry = NULL;

  g_variant_get(parameters, "(&s)", &entry);

  if (g_strcmp0(signal_name, "StatusNotifierItemRegistered") == 0)
    tray_add_entry(state, entry);
  else if (g_strcmp0(signal_name, "StatusNotifierItemUnregistered") == 0)
    tray_remove_entry(state, entry);
}

static void on_watcher_ready(GObject *source, GAsyncResult *result,
                             gpointer user_data) {
  TrayState *state = user_data;
  g_autoptr(GError) error = NULL;
  GDBusProxy *watcher = g_dbus_proxy_new_finish(result, &error);

  (void)source;

  if (!watcher) {
    LOG_C_DEBUG("Tray", "StatusNotifierWatcher is not reachable: %s",
                error->message);
    return;
  }

  state->watcher = watcher;

  g_signal_connect(watcher, "g-signal", G_CALLBACK(on_watcher_signal), state);

  /* The standard property is RegisteredStatusNotifierItems.  Some early
   * watchers called it RegisteredStatusItems; read both, because an item that
   * was already running when the bar started is only in this list. */
  g_autoptr(GVariant) items = g_dbus_proxy_get_cached_property(
      watcher, "RegisteredStatusNotifierItems");

  if (!items)
    items = g_dbus_proxy_get_cached_property(watcher, "RegisteredStatusItems");

  /* A watcher may keep the list for its hosts only.  Registering this bar as
   * a host is harmless when another host (a second bar) is already there. */
  const char *unique = g_dbus_connection_get_unique_name(state->bus);

  if (unique) {
    g_dbus_proxy_call(watcher, "RegisterStatusNotifierHost",
                      g_variant_new("(s)", unique), G_DBUS_CALL_FLAGS_NONE,
                      -1, NULL, NULL, NULL);
  }

  if (items) {
    GVariantIter iter;
    const char *entry;

    g_variant_iter_init(&iter, items);
    while (g_variant_iter_next(&iter, "&s", &entry))
      tray_add_entry(state, entry);
  }
}

static void on_watcher_appeared(GDBusConnection *connection, const char *name,
                                const char *owner, gpointer user_data) {
  (void)owner;

  TrayState *state = user_data;

  LOG_C_DEBUG("Tray", "%s appeared (%s)", name, owner ? owner : "?");

  g_dbus_proxy_new(connection, G_DBUS_PROXY_FLAGS_NONE, NULL, name,
                   "/StatusNotifierWatcher", "org.kde.StatusNotifierWatcher",
                   NULL, on_watcher_ready, state);
}

static void on_watcher_vanished(GDBusConnection *connection, const char *name,
                                gpointer user_data) {
  (void)connection;

  TrayState *state = user_data;

  LOG_C_DEBUG("Tray", "%s vanished", name);

  g_clear_object(&state->watcher);
  g_ptr_array_set_size(state->items, 0);
  tray_reflow(state);
}

/* =========================================================================
 * THE MODULE
 * ========================================================================= */

static void tray_destroy(WhelkModule *module) {
  TrayState *state = module->data;

  if (!state)
    return;

  if (state->watcher_id != 0)
    g_bus_unwatch_name(state->watcher_id);

  g_clear_object(&state->watcher);
  g_clear_pointer(&state->items, g_ptr_array_unref);
  g_clear_object(&state->bus);
  g_clear_pointer(&state->icon_paths, g_hash_table_unref);
  g_clear_pointer(&state->style, whelk_bar_config_free);
  g_free(state->hide);
  g_free(state);

  module->data = NULL;
}

WhelkModule *whelk_tray_module_new(const WhelkModuleSpec *spec,
                                   const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("tray", spec, bar);
  TrayState *state = g_new0(TrayState, 1);
  g_autoptr(GError) error = NULL;

  state->box = module->root;
  state->items = g_ptr_array_new_with_free_func(tray_item_free);
  state->style = bar ? whelk_bar_config_copy(bar) : whelk_bar_config_new("tray");
  state->hide = g_strdup(whelk_spec_str(spec, "hide", NULL));
  state->hide_network = bar ? bar->native_network : FALSE;
  state->hide_bluetooth = bar ? bar->native_bluetooth : FALSE;
  state->max_items = MAX(1, whelk_spec_int(spec, "max-items", 6));

  gtk_box_set_spacing(GTK_BOX(module->root),
                      whelk_spec_int(spec, "spacing", 6));
  gtk_widget_add_css_class(module->root, "whelk-tray-box");

  state->overflow_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 4);
  gtk_widget_add_css_class(state->overflow_box, "whelk-tray-overflow");

  state->more = gtk_menu_button_new();
  GtkWidget *chevron = gtk_image_new_from_icon_name("pan-down-symbolic");

  gtk_image_set_pixel_size(GTK_IMAGE(chevron), 16);
  gtk_menu_button_set_child(GTK_MENU_BUTTON(state->more), chevron);
  gtk_widget_add_css_class(state->more, "flat");
  gtk_widget_add_css_class(state->more, "whelk-tray-more");
  gtk_widget_set_tooltip_text(state->more, "More status icons");

  GtkWidget *overflow = gtk_popover_new();

  whelk_style_popover(GTK_POPOVER(overflow), bar);
  gtk_popover_set_child(GTK_POPOVER(overflow), state->overflow_box);
  gtk_menu_button_set_popover(GTK_MENU_BUTTON(state->more), overflow);
  gtk_box_append(GTK_BOX(module->root), state->more);
  gtk_widget_set_visible(state->more, FALSE);

  state->bus = g_bus_get_sync(G_BUS_TYPE_SESSION, NULL, &error);

  if (!state->bus) {
    LOG_C_WARN("Tray", "no session bus, so there is no tray: %s",
               error->message);
    module->data = state;
    module->destroy = tray_destroy;
    whelk_module_install(module, 0);
    return module;
  }

  state->watcher_id = g_bus_watch_name_on_connection(
      state->bus, "org.kde.StatusNotifierWatcher",
      G_BUS_NAME_WATCHER_FLAGS_NONE, on_watcher_appeared, on_watcher_vanished,
      state, NULL);

  module->data = state;
  module->destroy = tray_destroy;
  whelk_module_install(module, 0);

  return module;
}
