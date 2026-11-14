// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "guile_bridge.h"
#include "logger.h"
#include "shell.h"
#include "util/desktop.h"
#include "util/session.h"

#include "style.h"
#include "util/atspimenus.h"
#include "util/command.h"
#include "util/dbusmenu.h"
#include "util/menuaction.h"
#include "util/menusearch.h"
#include "util/menuprobe.h"
#include "util/gtkmenu.h"
#include "util/json.h"
#include "util/menudict.h"
#include "util/menutree.h"
#include "util/shortcutmaps.h"

#include <gdk/gdkkeysyms.h>
#include <gio/gdesktopappinfo.h>

/* =========================================================================
 * THE GLOBAL MENU
 * =========================================================================
 * The menu bar of the application in front, in the shell's own bar: File, Edit,
 * View, whichever menus the application has.
 *
 * Two producers, in that order.  An application that exports its menubar --
 * GTK's `gtk_application_set_menubar()' publishes it through `org.gtk.Menus' on
 * the application's bus name -- is read over D-Bus, so the labels and the
 * actions are its own.  Everything else gets a menubar the shell synthesizes:
 * the actions its desktop entry declares, closing its window, and the shortcuts
 * every application shares, typed by wtype when it is installed.
 *
 * The D-Bus read is a chain of asynchronous calls, so a slow or absent bus
 * costs a frame and never the bar.
 */

#define WHELK_MENU_ENTRY_POINTER "whelk-menu-entry"

typedef struct {
  char *app_id;
  char *bus_name;
  char *app_path;
  gint pid; /* the focused application, when the session names it */
  WhelkFocusedPaths focused; /* X11 object paths, empty on Wayland */
  char *app_name;
  GDesktopAppInfo *desktop;
  char *shortcut_tool;
  gboolean show_shortcuts;
  gboolean show_app_name;
  WhelkChordStyle style;
  GPtrArray *menus;       /* of WhelkMenu */
  GtkWidget *row;
  GPtrArray *buttons;     /* top-level buttons, borrowed from the row */
  GtkWidget *open_button; /* whose popover is up, borrowed */
  WhelkDBusMenuWatch *dbus_menu; /* follows a live DBusMenu, NULL otherwise */
  WhelkGtkMenuWatch *gtk_menu;   /* follows org.gtk.Menus Changed */
  WhelkAtspiMenuWatch *atspi;    /* follows the accessibility tree */
  guint generation;
} MenuData;

typedef struct {
  WhelkModule *module;
  guint generation;
  GDBusConnection *bus;
  GPtrArray *paths;      /* org.gtk.Menus object paths, tried first */
  guint index;
  char *dbus_bus_name;   /* the registrar's answer, when it supplied one */
  GPtrArray *dbus_paths; /* com.canonical.dbusmenu paths, tried next */
  guint dbus_index;
  gboolean registrar_tried;
} MenuProbe;

/* One popover's place in the menu tree: whose top-level menu it belongs to
 * (NULL for a submenu), and the module whose data tracks what is open. */
typedef struct {
  WhelkModule *module;
  GtkWidget *button;
  GtkWidget *popover;
} MenuPopover;

/* =========================================================================
 * MOVING THROUGH A MENU
 * =========================================================================
 * A menu bar is usable from the keyboard or it is a picture of one: Down and
 * Up walk the items, Home and End go to the ends, Left and Right move between
 * the top-level menus, and a submenu opens with Right.
 */

static GtkWidget *focusable_item(GtkWidget *child) {
  return (child && GTK_IS_BUTTON(child) && gtk_widget_get_sensitive(child))
             ? child
             : NULL;
}

static GtkWidget *first_item(GtkWidget *box) {
  if (!box)
    return NULL;

  for (GtkWidget *child = gtk_widget_get_first_child(box); child;
       child = gtk_widget_get_next_sibling(child))
    if (focusable_item(child))
      return child;

  return NULL;
}

static GtkWidget *last_item(GtkWidget *box);

static GtkWidget *next_item(GtkWidget *box, GtkWidget *from, gboolean forward) {
  GtkWidget *first = NULL;
  GtkWidget *previous = NULL;

  for (GtkWidget *child = gtk_widget_get_first_child(box); child;
       child = gtk_widget_get_next_sibling(child)) {
    if (!focusable_item(child))
      continue;

    if (!first)
      first = child;

    if (forward && previous == from)
      return child;

    if (!forward && child == from)
      return previous ? previous : last_item(box);

    previous = child;
  }

  /* Past the end, the menu wraps: a menu is a loop, not a page. */
  return forward ? first : previous;
}

static GtkWidget *last_item(GtkWidget *box) {
  GtkWidget *last = NULL;

  for (GtkWidget *child = gtk_widget_get_first_child(box); child;
       child = gtk_widget_get_next_sibling(child))
    if (focusable_item(child))
      last = child;

  return last;
}

static GtkWidget *focused_widget(GtkWidget *popover) {
  GtkRoot *root = gtk_widget_get_root(popover);

  return root ? gtk_root_get_focus(root) : NULL;
}

static void switch_top_level(WhelkModule *module, gint direction) {
  MenuData *data = module->data;
  guint index = 0;

  if (!data->buttons || data->buttons->len < 2)
    return;

  for (guint i = 0; i < data->buttons->len; i++)
    if (g_ptr_array_index(data->buttons, i) == data->open_button) {
      index = i;
      break;
    }

  const guint count = data->buttons->len;
  const guint next = direction > 0 ? (index + 1) % count
                                   : (index + count - 1) % count;
  GtkWidget *button = g_ptr_array_index(data->buttons, next);

  if (button == data->open_button)
    return;

  if (data->open_button) {
    GtkPopover *old =
        gtk_menu_button_get_popover(GTK_MENU_BUTTON(data->open_button));

    if (old)
      gtk_popover_popdown(old);
  }

  GtkPopover *popover = gtk_menu_button_get_popover(GTK_MENU_BUTTON(button));

  if (popover)
    gtk_popover_popup(popover);
}

static gboolean on_menu_key(GtkEventControllerKey *controller, guint keyval,
                            guint keycode, GdkModifierType state,
                            gpointer user_data) {
  (void)controller;
  (void)keycode;
  (void)state;

  MenuPopover *context = user_data;
  MenuData *data = context->module->data;
  GtkWidget *box = gtk_popover_get_child(GTK_POPOVER(context->popover));

  if (!box)
    return FALSE;

  GtkWidget *focus = focused_widget(context->popover);

  if (focus && !gtk_widget_is_ancestor(focus, context->popover))
    focus = NULL;

  switch (keyval) {
  case GDK_KEY_Down:
    focus = next_item(box, focus, TRUE);
    break;
  case GDK_KEY_Up:
    focus = next_item(box, focus, FALSE);
    break;
  case GDK_KEY_Home:
    focus = first_item(box);
    break;
  case GDK_KEY_End:
    focus = last_item(box);
    break;
  case GDK_KEY_Right:
    if (focus && GTK_IS_MENU_BUTTON(focus)) {
      GtkPopover *submenu =
          gtk_menu_button_get_popover(GTK_MENU_BUTTON(focus));

      if (submenu) {
        gtk_popover_popup(submenu);
        return TRUE;
      }
    }

    if (context->button) {
      switch_top_level(context->module, 1);
      return TRUE;
    }

    return FALSE;
  case GDK_KEY_Left:
    if (context->button) {
      switch_top_level(context->module, -1);
      return TRUE;
    }

    gtk_popover_popdown(GTK_POPOVER(context->popover));

    if (data->open_button)
      gtk_widget_grab_focus(data->open_button);

    return TRUE;
  default:
    return FALSE;
  }

  if (focus)
    gtk_widget_grab_focus(focus);

  return TRUE;
}

static void on_popover_visible(GObject *object, GParamSpec *pspec,
                               gpointer user_data) {
  (void)pspec;

  MenuPopover *context = user_data;
  MenuData *data = context->module->data;

  if (gtk_widget_get_visible(GTK_WIDGET(object))) {
    if (!context->button)
      return;

    data->open_button = context->button;

    GtkWidget *first = first_item(
        gtk_popover_get_child(GTK_POPOVER(context->popover)));

    if (first)
      gtk_widget_grab_focus(first);
  } else if (context->button && data->open_button == context->button) {
    data->open_button = NULL;
  }
}

static void on_button_enter(GtkEventControllerMotion *controller, double x,
                            double y, gpointer user_data) {
  (void)x;
  (void)y;

  WhelkModule *module = user_data;
  MenuData *data = module->data;
  GtkWidget *button =
      gtk_event_controller_get_widget(GTK_EVENT_CONTROLLER(controller));

  /* Once a menu is down, sliding along the bar opens the next one: the
   * behavior every menu bar has, and the reason the labels are a row. */
  if (!data->open_button || data->open_button == button)
    return;

  GtkPopover *old =
      gtk_menu_button_get_popover(GTK_MENU_BUTTON(data->open_button));
  GtkPopover *popover = gtk_menu_button_get_popover(GTK_MENU_BUTTON(button));

  if (old)
    gtk_popover_popdown(old);

  if (popover)
    gtk_popover_popup(popover);
}

static void attach_popover(WhelkModule *module, GtkPopover *popover,
                           GtkWidget *button) {
  MenuPopover *context = g_new0(MenuPopover, 1);

  context->module = module;
  context->button = button;
  context->popover = GTK_WIDGET(popover);

  g_object_set_data_full(G_OBJECT(popover), "whelk-menu-popover", context,
                         g_free);

  GtkEventController *keys = gtk_event_controller_key_new();

  gtk_event_controller_set_propagation_phase(keys, GTK_PHASE_CAPTURE);
  g_signal_connect(keys, "key-pressed", G_CALLBACK(on_menu_key), context);
  gtk_widget_add_controller(GTK_WIDGET(popover), keys);

  if (button)
    g_signal_connect(popover, "notify::visible", G_CALLBACK(on_popover_visible),
                     context);
}

/* --- activation ----------------------------------------------------------- */

/* The entry's own activation lives in util/menuaction.c, because the
 * launcher's menu search chooses the same entries and has to do exactly what
 * this popover does. */
static void menu_context(MenuData *data, WhelkMenuActionContext *context) {
  whelk_menu_action_context_set(
      context, data->bus_name, data->app_path,
      data->desktop ? g_desktop_app_info_get_filename(data->desktop) : NULL);
}

static void activate_entry(WhelkModule *module, WhelkMenuEntry *entry) {
  MenuData *data = module->data;
  WhelkMenuActionContext context = {0};

  menu_context(data, &context);
  whelk_menu_entry_activate(entry, &context);
  whelk_menu_action_context_clear(&context);
}

static void on_entry_clicked(GtkButton *button, gpointer user_data) {
  WhelkModule *module = user_data;
  WhelkMenuEntry *entry =
      g_object_get_data(G_OBJECT(button), WHELK_MENU_ENTRY_POINTER);

  /* A click closes the menu it came from, the way every menubar does. */
  GtkWidget *popover = gtk_widget_get_ancestor(GTK_WIDGET(button),
                                               GTK_TYPE_POPOVER);

  if (popover)
    gtk_popover_popdown(GTK_POPOVER(popover));

  activate_entry(module, entry);
}

/* --- rendering ------------------------------------------------------------ */

static void clear_row(MenuData *data) {
  GtkWidget *child = NULL;

  while ((child = gtk_widget_get_first_child(data->row)) != NULL)
    gtk_box_remove(GTK_BOX(data->row), child);
}

static void apply_popover_skin(WhelkModule *module, GtkPopover *popover) {
  whelk_style_popover(popover, module->bar);
}

/* =========================================================================
 * THE APPLICATION'S OWN MENU
 * =========================================================================
 * The name before the menus is not a label: it is the one menu that belongs
 * to the shell rather than to the focused application -- the actions that are
 * true whatever window is in front: the compositor's arranging verbs and
 * Close Window.  The launcher, panels, notifications and session control live
 * on their own modules and in the shell menu, not in a second copy on every
 * application's menu.
 */

static void fill_popover(WhelkModule *module, GtkPopover *popover,
                         WhelkMenu *menu);

static GtkWidget *app_menu_button(WhelkModule *module) {
  MenuData *data = module->data;
  GtkWidget *button = gtk_menu_button_new();
  GtkWidget *label = gtk_label_new(data->app_name);
  GtkWidget *popover = gtk_popover_new();

  gtk_label_set_ellipsize(GTK_LABEL(label), PANGO_ELLIPSIZE_END);
  gtk_label_set_max_width_chars(GTK_LABEL(label), 22);
  gtk_menu_button_set_child(GTK_MENU_BUTTON(button), label);
  gtk_widget_add_css_class(button, "flat");
  gtk_widget_add_css_class(button, "whelk-menu-app-button");
  gtk_widget_set_tooltip_text(button, data->app_name);

  WhelkSessionWindowActions actions;

  whelk_session_window_actions(whelk_session_detect(), &actions);

  WhelkMenu *menu = whelk_app_menu(&actions);

  apply_popover_skin(module, GTK_POPOVER(popover));
  fill_popover(module, GTK_POPOVER(popover), menu);
  gtk_menu_button_set_popover(GTK_MENU_BUTTON(button), popover);

  whelk_menu_free(menu);
  whelk_session_window_actions_clear(&actions);

  return button;
}

static void fill_popover(WhelkModule *module, GtkPopover *popover,
                         WhelkMenu *menu) {
  GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);

  for (guint i = 0; i < menu->entries->len; i++) {
    WhelkMenuEntry *entry = g_ptr_array_index(menu->entries, i);

    if (entry->kind == WHELK_MENU_SEPARATOR) {
      gtk_box_append(GTK_BOX(box), gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));
      continue;
    }

    GtkWidget *button = gtk_button_new();

    /* A shortcut entry shows the chord it types where a menu shows one: on
     * the right, dim, and out of the way of the label. */
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

    if (entry->submenu && entry->submenu->entries->len > 0) {
      GtkWidget *submenu_button = gtk_menu_button_new();

      /* A label rather than a child: the button's own arrow is the chevron a
       * menu row uses to say "there is more behind this one". */
      gtk_menu_button_set_has_frame(GTK_MENU_BUTTON(submenu_button), FALSE);
      gtk_menu_button_set_label(GTK_MENU_BUTTON(submenu_button), entry->label);
      gtk_menu_button_set_direction(GTK_MENU_BUTTON(submenu_button),
                                    GTK_ARROW_RIGHT);
      gtk_widget_add_css_class(submenu_button, "flat");
      gtk_widget_add_css_class(submenu_button, "whelk-menu-item");
      gtk_widget_set_halign(submenu_button, GTK_ALIGN_FILL);

      GtkWidget *submenu_popover = gtk_popover_new();

      apply_popover_skin(module, GTK_POPOVER(submenu_popover));
      fill_popover(module, GTK_POPOVER(submenu_popover), entry->submenu);
      attach_popover(module, GTK_POPOVER(submenu_popover), NULL);
      gtk_menu_button_set_popover(GTK_MENU_BUTTON(submenu_button),
                                  submenu_popover);
      gtk_box_append(GTK_BOX(box), submenu_button);
      continue;
    }

    g_object_set_data(G_OBJECT(button), WHELK_MENU_ENTRY_POINTER, entry);
    g_signal_connect(button, "clicked", G_CALLBACK(on_entry_clicked), module);
    gtk_box_append(GTK_BOX(box), button);
  }

  gtk_popover_set_child(popover, box);
}

static void render(WhelkModule *module) {
  MenuData *data = module->data;

  data->open_button = NULL;
  g_ptr_array_set_size(data->buttons, 0);
  clear_row(data);

  /* A menu bar is the application's name and then its menus, the way macOS
   * puts the application first.  The name is the shell's own menu, so it is
   * present even when the application offers no menus at all: otherwise a
   * session with no shortcut tool (or no focused window) would lose the
   * launcher, the panels and the way out as well. */
  const gboolean show_app_menu =
      data->show_app_name && data->app_name && *data->app_name != '\0';

  if (show_app_menu) {
    GtkWidget *name = app_menu_button(module);

    gtk_widget_set_valign(name, GTK_ALIGN_CENTER);
    gtk_box_append(GTK_BOX(data->row), name);
  }

  for (guint i = 0; i < data->menus->len; i++) {
    WhelkMenu *menu = g_ptr_array_index(data->menus, i);
    GtkWidget *button = gtk_menu_button_new();
    GtkWidget *popover = gtk_popover_new();

    /* A child instead of a label: a menu button grows the little down arrow
     * only when it has neither, and a menubar's labels do not point at
     * anything. */
    gtk_menu_button_set_has_frame(GTK_MENU_BUTTON(button), FALSE);
    gtk_menu_button_set_child(GTK_MENU_BUTTON(button),
                              gtk_label_new(menu->label));
    gtk_widget_add_css_class(button, "flat");
    gtk_widget_add_css_class(button, "whelk-menu-button");

    apply_popover_skin(module, GTK_POPOVER(popover));
    fill_popover(module, GTK_POPOVER(popover), menu);
    attach_popover(module, GTK_POPOVER(popover), button);
    gtk_menu_button_set_popover(GTK_MENU_BUTTON(button), popover);

    GtkEventController *motion = gtk_event_controller_motion_new();

    g_signal_connect(motion, "enter", G_CALLBACK(on_button_enter), module);
    gtk_widget_add_controller(button, motion);

    g_ptr_array_add(data->buttons, button);
    gtk_box_append(GTK_BOX(data->row), button);
  }

  gtk_widget_set_visible(data->row, show_app_menu || data->menus->len > 0);
}

/* --- what the application exports ----------------------------------------- */

/* A tree is about to be replaced.  Whatever popover is open belongs to the
 * old one: its entries' ids may already mean something else, and a click on a
 * stale id is a click on an unrelated command.  Closing the menu is the only
 * honest answer. */
static void close_open_menus(WhelkModule *module) {
  MenuData *data = module->data;

  if (!data->buttons)
    return;

  for (guint i = 0; i < data->buttons->len; i++) {
    GtkWidget *button = g_ptr_array_index(data->buttons, i);

    if (!button || !GTK_IS_MENU_BUTTON(button))
      continue;

    GtkPopover *popover = gtk_menu_button_get_popover(GTK_MENU_BUTTON(button));

    if (popover)
      gtk_popover_popdown(popover);
  }

  data->open_button = NULL;
}

static void replace_menus(WhelkModule *module, GPtrArray *menus) {
  MenuData *data = module->data;

  close_open_menus(module);

  g_clear_pointer(&data->menus, g_ptr_array_unref);
  data->menus = menus;

  /* The launcher's menu search reads this snapshot: the same tree, flattened,
   * so a menu can be found by what it says rather than by opening it. */
  WhelkMenuActionContext context = {0};

  menu_context(data, &context);
  whelk_menu_search_publish(data->app_name, &context, menus);
  whelk_menu_action_context_clear(&context);

  render(module);
}

static void stop_dbus_menu_watch(MenuData *data) {
  if (!data->dbus_menu)
    return;

  whelk_dbusmenu_watch_free(data->dbus_menu);
  data->dbus_menu = NULL;
}

static void stop_gtk_menu_watch(MenuData *data) {
  if (!data->gtk_menu)
    return;

  whelk_gtkmenu_watch_free(data->gtk_menu);
  data->gtk_menu = NULL;
}

/* LayoutUpdated means the old ids may already belong to other rows: close
 * first, read second.  The read itself replaces the tree and repaints. */
static void on_dbus_menu_invalidated(gpointer user_data) {
  close_open_menus(user_data);
}

/* Firefox or Thunderbird changed a menu while the bar was showing it: the
 * answer is a fresh tree, ids and all, and the open menu goes away rather than
 * keeping ids that now point elsewhere. */
static void on_dbus_menu_changed(GPtrArray *menus, gpointer user_data) {
  WhelkModule *module = user_data;

  replace_menus(module, menus);
}

/* org.gtk.Menus Changed carries add/remove tuples, but the tree we render is
 * not diffable in place: close the popover, then let Start return the current
 * tree.  This is the same contract as the DBusMenu watch. */
static void on_gtk_menu_changed(GPtrArray *menus, gpointer user_data) {
  WhelkModule *module = user_data;

  replace_menus(module, menus);
}

static void probe_next(MenuProbe *probe);

static void probe_free(MenuProbe *probe) {
  g_clear_object(&probe->bus);
  g_clear_pointer(&probe->paths, g_ptr_array_unref);
  g_clear_pointer(&probe->dbus_bus_name, g_free);
  g_clear_pointer(&probe->dbus_paths, g_ptr_array_unref);
  g_free(probe);
}

static void on_start_reply(GObject *source, GAsyncResult *result,
                           gpointer user_data) {
  MenuProbe *probe = user_data;
  MenuData *data = probe->module->data;
  gboolean adopted = FALSE;

  g_autoptr(GError) error = NULL;
  GVariant *reply =
      g_dbus_connection_call_finish(G_DBUS_CONNECTION(source), result, &error);

  if (reply) {
    if (probe->generation == data->generation) {
      GVariant *parameters = g_variant_get_child_value(reply, 0);
      GPtrArray *menus =
          whelk_menubar_from_gtk(parameters, 0, 0, data->style);

      if (menus->len > 0) {
        const char *path =
            g_ptr_array_index(probe->paths, probe->index - 1);

        LOG_C_DEBUG("Menu", "%s exports a menubar: %u menu(s) from D-Bus",
                    data->app_id ? data->app_id : "(anonymous)", menus->len);
        stop_dbus_menu_watch(data);
        replace_menus(probe->module, menus);

        /* The GTK menu can change while its popover is open (a File menu that
         * grows after Recent Documents is loaded, for instance).  Follow the
         * protocol's Changed signal so the bar is not a photograph. */
        stop_gtk_menu_watch(data);
        data->gtk_menu = whelk_gtkmenu_watch_new(
            probe->bus, data->bus_name, path, data->style,
            on_gtk_menu_changed, on_dbus_menu_invalidated, probe->module);

        adopted = TRUE;
      } else {
        g_ptr_array_unref(menus);
      }

      g_variant_unref(parameters);
    }

    g_variant_unref(reply);
  }

  if (adopted || probe->generation != data->generation)
    probe_free(probe);
  else
    probe_next(probe);
}

/* =========================================================================
 * THE OTHER MENU PROTOCOL
 * =========================================================================
 * `org.gtk.Menus' is how a GTK application publishes a menubar; Qt, KDE and
 * everything that registers with `com.canonical.AppMenu.Registrar' answers
 * `com.canonical.dbusmenu' instead.  Both are asked, in that order, because a
 * menu bar that only speaks one toolkit's protocol is a menu bar for one
 * toolkit's applications.
 *
 * The conventional paths are tried, not discovered: an application may put its
 * menu anywhere, but the ones that publish one put it where the protocol says.
 */

static void probe_dbus_next(MenuProbe *probe);

static void on_layout_reply(GObject *source, GAsyncResult *result,
                            gpointer user_data) {
  MenuProbe *probe = user_data;
  MenuData *data = probe->module->data;
  gboolean adopted = FALSE;

  g_autoptr(GError) error = NULL;
  GVariant *reply =
      g_dbus_connection_call_finish(G_DBUS_CONNECTION(source), result, &error);

  if (reply) {
    if (probe->generation == data->generation && probe->dbus_index > 0) {
      const char *path =
          g_ptr_array_index(probe->dbus_paths, probe->dbus_index - 1);
      const char *bus_name = probe->dbus_bus_name ? probe->dbus_bus_name
                                                   : data->bus_name;
      GPtrArray *menus = whelk_menubar_from_dbusmenu(
          reply, bus_name, path, data->style);

      if (menus->len > 0) {
        LOG_C_DEBUG("Menu", "%s exports DBusMenu at %s: %u menu(s)",
                    data->app_id ? data->app_id : "(anonymous)", path,
                    menus->len);
        replace_menus(probe->module, menus);

        /* From here the menu is followed, not remembered: every LayoutUpdated
         * or ItemsPropertiesUpdated costs one GetLayout and a new id set. */
        stop_dbus_menu_watch(data);
        stop_gtk_menu_watch(data);
        data->dbus_menu = whelk_dbusmenu_watch_new(
            probe->bus, bus_name, path, data->style,
            on_dbus_menu_changed, on_dbus_menu_invalidated, probe->module);

        adopted = TRUE;
      } else {
        g_ptr_array_unref(menus);
      }
    }

    g_variant_unref(reply);
  }

  if (adopted || probe->generation != data->generation)
    probe_free(probe);
  else
    probe_dbus_next(probe);
}

/* The last producer: the application's own menus from its accessibility tree.
 * It runs only after org.gtk.Menus and DBusMenu found nothing, and it is
 * asynchronous because walking a toolkit's tree is slow.  After the first
 * read the watch stays alive: AT-SPI object events make it walk again, the
 * same way org.gtk.Menus' Changed signal makes the GTK producer re-read. */
static void on_atspi_menus(GPtrArray *menus, gpointer user_data) {
  WhelkModule *module = user_data;
  MenuData *data = module->data;

  if (!menus)
    return;

  if (menus->len == 0) {
    g_ptr_array_unref(menus);
    return;
  }

  LOG_C_DEBUG("Menu", "%s: %u menu(s) from AT-SPI", data->app_name, menus->len);
  replace_menus(module, menus);
}

static void start_atspi_probe(WhelkModule *module) {
  MenuData *data = module->data;

  if (!data || data->atspi)
    return;

  data->atspi = whelk_atspi_menubar_watch_new(
      data->app_name, data->pid, data->style, on_atspi_menus, module);
}

static void probe_dbus_next(MenuProbe *probe) {
  MenuData *data = probe->module->data;
  const char *bus_name =
      probe->dbus_bus_name ? probe->dbus_bus_name : data->bus_name;

  if (!bus_name || !g_dbus_is_name(bus_name) ||
      probe->dbus_index >= probe->dbus_paths->len) {
    if (probe->generation == data->generation)
      start_atspi_probe(probe->module);

    probe_free(probe);
    return;
  }

  const char *path =
      g_ptr_array_index(probe->dbus_paths, probe->dbus_index++);

  /* parent 0, depth -1: the whole tree in one answer, which is what a menubar
   * needs and what a menu that changes while it is open cannot use anyway. */
  g_dbus_connection_call(probe->bus, bus_name, path,
                         "com.canonical.dbusmenu", "GetLayout",
                         g_variant_new("(ii)", 0, -1),
                         G_VARIANT_TYPE("(u(ia{sv}av))"),
                         G_DBUS_CALL_FLAGS_NONE, 1500, NULL, on_layout_reply,
                         probe);
}

/* =========================================================================
 * THE REGISTRAR
 * =========================================================================
 * A Qt or KDE window can put its DBusMenu at an object path of its choosing.
 * The registrar maps an X11 window id to that exact bus and path, which is the
 * one piece of information a WM_CLASS cannot provide.  The answer is metadata
 * for the ordinary DBusMenu reader: the path is tried first, then the
 * conventional ones, so a registrar that knows only half the truth is still an
 * improvement rather than a dead end. */

static void on_registrar_reply(GObject *source, GAsyncResult *result,
                               gpointer user_data);

static void probe_registrar(MenuProbe *probe) {
  MenuData *data = probe->module->data;

  probe->registrar_tried = TRUE;

  if (data->focused.window_id == 0) {
    probe_dbus_next(probe);
    return;
  }

  g_dbus_connection_call(
      probe->bus, "com.canonical.AppMenu.Registrar",
      "/com/canonical/AppMenu/Registrar", "com.canonical.AppMenu.Registrar",
      "GetMenuForWindow", g_variant_new("(u)", data->focused.window_id),
      G_VARIANT_TYPE("(so)"), G_DBUS_CALL_FLAGS_NONE, 1200, NULL,
      on_registrar_reply, probe);
}

static void on_registrar_reply(GObject *source, GAsyncResult *result,
                               gpointer user_data) {
  MenuProbe *probe = user_data;
  MenuData *data = probe->module->data;
  g_autoptr(GError) error = NULL;
  GVariant *reply =
      g_dbus_connection_call_finish(G_DBUS_CONNECTION(source), result, &error);

  if (reply && probe->generation == data->generation) {
    const char *bus_name = NULL;
    const char *path = NULL;

    g_variant_get(reply, "(&s&o)", &bus_name, &path);

    if (bus_name && g_dbus_is_name(bus_name) && path &&
        g_variant_is_object_path(path)) {
      g_free(probe->dbus_bus_name);
      probe->dbus_bus_name = g_strdup(bus_name);
      whelk_menu_probe_use_registrar_path(probe->dbus_paths, path);
      probe->dbus_index = 0;
    }
  }

  if (reply)
    g_variant_unref(reply);

  if (probe->generation != data->generation)
    probe_free(probe);
  else
    probe_dbus_next(probe);
}

static void probe_next(MenuProbe *probe) {
  MenuData *data = probe->module->data;

  if (probe->index >= probe->paths->len) {
    if (!probe->registrar_tried && data->focused.window_id != 0)
      probe_registrar(probe);
    else
      probe_dbus_next(probe);
    return;
  }

  const char *path = g_ptr_array_index(probe->paths, probe->index++);
  guint32 zero = 0;
  GVariant *groups = g_variant_new_fixed_array(G_VARIANT_TYPE_UINT32, &zero, 1,
                                               sizeof(guint32));

  g_dbus_connection_call(probe->bus, data->bus_name, path, "org.gtk.Menus",
                         "Start", g_variant_new("(@au)", groups),
                         G_VARIANT_TYPE("(a(uuaa{sv}))"),
                         G_DBUS_CALL_FLAGS_NONE, 1500, NULL, on_start_reply,
                         probe);
}

static void probe_menus(WhelkModule *module) {
  MenuData *data = module->data;

  g_autoptr(GError) error = NULL;
  g_autoptr(GDBusConnection) bus =
      g_bus_get_sync(G_BUS_TYPE_SESSION, NULL, &error);

  if (!bus)
    return;

  /* An application id is not a bus name: sway reports alacritty's as
   * "Alacritty", which has no dot and is not a valid D-Bus name, so asking the
   * bus for it is a programming error that GLib answers with a critical.  The
   * registrar can still resolve the focused X11 window; without one there is
   * nothing to ask. */
  const gboolean valid_bus =
      data->bus_name && g_dbus_is_name(data->bus_name);

  /* A Wayland application whose id is not a bus name ("Freebuff", "Alacritty")
   * has no protocol menus, but it may still publish a menu bar to AT-SPI. */
  if (!valid_bus && data->focused.window_id == 0) {
    start_atspi_probe(module);
    return;
  }

  MenuProbe *probe = g_new0(MenuProbe, 1);

  probe->module = module;
  probe->generation = data->generation;
  probe->bus = g_object_ref(bus);
  WhelkMenuProbeOrder *order =
      whelk_menu_probe_order_new(&data->focused, valid_bus, data->app_path);

  probe->paths = order->gtk_paths;
  probe->dbus_paths = order->dbus_paths;
  order->gtk_paths = NULL;
  order->dbus_paths = NULL;
  g_free(order);

  probe_next(probe);
}

/* --- the application in front --------------------------------------------- */

/* A menu a configuration writes itself: `#:menu-command' names a program that
 * prints `Menu<TAB>Item<TAB>Chord<TAB>Command' lines, and its answer becomes
 * the application's menubar.  That is the escape hatch for an application no
 * bus describes, and the reason adding one does not need a rebuild. */
static void on_script_menus(WhelkModule *module, const char *text,
                            gpointer user_data) {
  MenuData *data = module->data;

  if (GPOINTER_TO_UINT(user_data) != data->generation)
    return;

  GPtrArray *menus = whelk_menubar_from_text(text, data->style);

  if (menus->len > 0)
    replace_menus(module, menus);
  else
    g_ptr_array_unref(menus);
}

static gboolean focused_paths_equal(const WhelkFocusedPaths *a,
                                    const WhelkFocusedPaths *b) {
  return a->window_id == b->window_id &&
         g_strcmp0(a->bus_name, b->bus_name) == 0 &&
         g_strcmp0(a->app_path, b->app_path) == 0 &&
         g_strcmp0(a->window_path, b->window_path) == 0 &&
         g_strcmp0(a->menubar_path, b->menubar_path) == 0 &&
         g_strcmp0(a->appmenu_path, b->appmenu_path) == 0;
}

static void copy_focused_paths(WhelkFocusedPaths *target,
                               const WhelkFocusedPaths *source) {
  whelk_focused_paths_clear(target);

  if (!source)
    return;

  target->window_id = source->window_id;
  target->bus_name = g_strdup(source->bus_name);
  target->app_path = g_strdup(source->app_path);
  target->window_path = g_strdup(source->window_path);
  target->menubar_path = g_strdup(source->menubar_path);
  target->appmenu_path = g_strdup(source->appmenu_path);
}

/* An imported corpus describes the application's own menus, not the
 * compositor's.  Keep the synthesized menus whose labels the import does not
 * provide (Window, or a File holding this window's close action), and let the
 * application's own menu win when both have the same label. */
static GPtrArray *merge_imported_menus(GPtrArray *menus,
                                       GPtrArray *imported) {
  for (guint i = 0; i < menus->len;) {
    WhelkMenu *fallback = g_ptr_array_steal_index(menus, i);
    gboolean duplicate = FALSE;

    for (guint j = 0; j < imported->len; j++) {
      WhelkMenu *present = g_ptr_array_index(imported, j);

      if (g_strcmp0(present->label, fallback->label) == 0) {
        duplicate = TRUE;
        break;
      }
    }

    if (duplicate)
      whelk_menu_free(fallback);
    else
      g_ptr_array_add(imported, fallback);
  }

  g_ptr_array_unref(menus);

  return imported;
}

/* `paths' is the X11 window's published object graph, or NULL when the caller
 * has none.  NULL keeps the current paths while the application is unchanged;
 * changing applications always starts clean. */
static void build_for_app(WhelkModule *module, const char *app_id,
                          const WhelkFocusedPaths *paths) {
  MenuData *data = module->data;
  const gboolean same_app = g_strcmp0(app_id, data->app_id) == 0;

  /* The previous application's menu is gone, whatever published it. */
  stop_dbus_menu_watch(data);
  stop_gtk_menu_watch(data);
  whelk_atspi_menubar_watch_free(data->atspi);
  data->atspi = NULL;

  data->generation++;

  g_free(data->app_id);
  data->app_id = g_strdup(app_id ? app_id : "");

  g_clear_object(&data->desktop);
  data->desktop = whelk_desktop_for_app_id(data->app_id);

  /* What the application calls itself: the dictionary's name, the desktop
   * entry's, or what the compositor reported. */
  g_free(data->app_name);

  const WhelkDictApp *named = whelk_menudict_for_app(data->app_id);

  if (named)
    data->app_name = g_strdup(named->name);
  else if (*data->app_id == '\0')
    data->app_name = g_strdup("whelk");
  else
    data->app_name = whelk_desktop_short_name(data->app_id, data->desktop);

  g_free(data->bus_name);
  data->bus_name = g_strdup(data->app_id);

  g_free(data->app_path);

  if (*data->app_id) {
    g_autofree char *dots = g_strdup(data->app_id);

    for (char *cursor = dots; *cursor; cursor++)
      if (*cursor == '.')
        *cursor = '/';

    data->app_path = g_strdup_printf("/%s", dots);
  } else {
    data->app_path = g_strdup("/org/gtk/Application/anonymous");
  }

  if (paths)
    copy_focused_paths(&data->focused, paths);
  else if (!same_app)
    whelk_focused_paths_clear(&data->focused);

  /* The properties are more specific than an application id: they name the
   * exact bus and object paths GTK exported.  Use them when present, and keep
   * the id-derived values otherwise. */
  if (data->focused.bus_name && *data->focused.bus_name) {
    g_free(data->bus_name);
    data->bus_name = g_strdup(data->focused.bus_name);
  }

  if (data->focused.app_path && *data->focused.app_path) {
    g_free(data->app_path);
    data->app_path = g_strdup(data->focused.app_path);
  }

  const char *const *action_ids = NULL;
  const char *const *action_labels = NULL;

  /* The action list belongs to the app info -- listing it hands out its own
   * storage, not a copy -- so it is read and never freed: freeing it here left
   * the app info to free what was already gone on its next unref.  The labels
   * are ours, and are freed when this function returns. */
  g_auto(GStrv) labels = NULL;

  if (data->desktop) {
    action_ids = g_desktop_app_info_list_actions(data->desktop);

    if (action_ids) {
      const guint count = g_strv_length((char **)action_ids);

      labels = g_new0(char *, count + 1);

      for (guint i = 0; i < count; i++) {
        char *name =
            g_desktop_app_info_get_action_name(data->desktop, action_ids[i]);

        labels[i] = g_strdup(name ? name : action_ids[i]);
        g_free(name);
      }

      action_labels = (const char *const *)labels;
    }
  }

  const guint action_count =
      action_ids ? g_strv_length((char **)action_ids) : 0;

  const WhelkSession session = whelk_session_detect();
  WhelkSessionWindowActions window_actions;

  whelk_session_window_actions(session, &window_actions);

  /* The desktop entry's Exec is the command that starts another window.  Its
   * field codes are for a file manager or a URL handler to fill in; a menu
   * opening the application without a document drops them. */
  g_autofree char *new_window_command = NULL;

  if (data->desktop) {
    g_autofree char *exec = g_desktop_app_info_get_string(
        data->desktop, G_KEY_FILE_DESKTOP_KEY_EXEC);

    if (exec && *exec) {
      GString *command = g_string_new(NULL);

      for (const char *cursor = exec; *cursor; cursor++) {
        if (*cursor != '%') {
          g_string_append_c(command, *cursor);
          continue;
        }

        const char code = cursor[1];

        if (code == '%')
          g_string_append_c(command, '%');

        if (code == '%' || (code && strchr("fFuUick", code)))
          cursor++;
      }

      g_strstrip(command->str);
      new_window_command = g_string_free(command, FALSE);

      if (*new_window_command == '\0')
        g_clear_pointer(&new_window_command, g_free);
    }
  }

  const char *source = NULL;

  GPtrArray *menus = whelk_menubar_for_app(
      data->app_id, action_ids, action_labels, action_count, new_window_command,
      &window_actions,
      data->show_shortcuts ? data->shortcut_tool : NULL, data->style, &source);

  whelk_session_window_actions_clear(&window_actions);

  if (source)
    LOG_C_DEBUG("Menu", "%s: the dictionary's menus, typed as its shortcuts",
                source);

  /* A user-installed JSON mapping wins over the built-in Scheme corpus: that
   * is how a local correction to an application is tested without rebuilding
   * the shell.  The curated dictionary still wins over both. */
  gboolean imported = FALSE;

  if (!source && data->shortcut_tool && *data->shortcut_tool &&
      data->app_id && *data->app_id) {
    GPtrArray *orbit = whelk_shortcutmaps_menubar_for_app(
        data->app_id, data->shortcut_tool, data->style);

    if (orbit->len > 0) {
      LOG_C_DEBUG("Menu", "%s: %u menu(s) from an installed shortcut map",
                  data->app_id, orbit->len);
      menus = merge_imported_menus(menus, orbit);
      imported = TRUE;
    } else {
      g_ptr_array_unref(orbit);
    }
  }

  /* The Orbit corpus ships with whelk as Scheme (`(whelk orbit)'): 293
   * applications, already converted from its JSON, so the common case needs
   * no download.  The curated dictionary still wins; the corpus fills the gap
   * before the shared Edit and View are used.  Its data is GPL-3.0-or-later
   * and keeps that licence in its own module. */
  char *orbit_name = NULL;

  if (!source && !imported && data->shortcut_tool && *data->shortcut_tool &&
      data->app_id && *data->app_id) {
    GPtrArray *orbit = whelk_bridge_orbit_menubar(
        data->app_id, data->shortcut_tool, data->style, &orbit_name);

    if (orbit->len > 0) {
      LOG_C_DEBUG("Menu", "%s: %u menu(s) from the Scheme shortcut corpus",
                  orbit_name ? orbit_name : data->app_id, orbit->len);
      menus = merge_imported_menus(menus, orbit);
    } else {
      g_ptr_array_unref(orbit);
    }
  }

  g_free(orbit_name);

  replace_menus(module, menus);

  /* A configuration's own menu wins over every producer: it was asked for by
   * name, for this application, and nothing the bus says is more specific. */
  const char *menu_command =
      whelk_spec_str(module->spec, "menu-command", NULL);

  if (menu_command && *menu_command != '\0') {
    whelk_module_command(module, menu_command, on_script_menus,
                         GUINT_TO_POINTER(data->generation));
    return;
  }

  probe_menus(module);
}

static void on_focused_window(WhelkModule *module, const char *text,
                              gpointer user_data) {
  (void)user_data;

  MenuData *data = module->data;
  char *title = NULL;
  char *app_id = NULL;
  WhelkFocusedPaths paths = {0};
  const WhelkSession session = whelk_session_detect();

  if (!whelk_session_parse_focused(session, text, &title, &app_id)) {
    data->pid = 0;
    build_for_app(module, "", NULL);
    return;
  }

  data->pid = whelk_session_focused_pid(session, text);
  whelk_session_parse_focused_paths(session, text, &paths);

  if (g_strcmp0(app_id, data->app_id) != 0 ||
      !focused_paths_equal(&paths, &data->focused))
    build_for_app(module, app_id, &paths);

  g_free(title);
  g_free(app_id);
  whelk_focused_paths_clear(&paths);
}

static void menu_update(WhelkModule *module) {
  g_autofree char *command =
      whelk_session_focused_window_command(whelk_session_detect());

  if (!command) {
    build_for_app(module, "", NULL);
    return;
  }

  whelk_module_command(module, command, on_focused_window, NULL);
}

/* --- lifecycle ------------------------------------------------------------ */

static void menu_destroy(WhelkModule *module) {
  MenuData *data = module->data;

  if (!data)
    return;

  stop_dbus_menu_watch(data);
  stop_gtk_menu_watch(data);
  whelk_atspi_menubar_watch_free(data->atspi);
  data->atspi = NULL;

  g_clear_pointer(&data->menus, g_ptr_array_unref);
  g_clear_pointer(&data->buttons, g_ptr_array_unref);
  g_clear_object(&data->desktop);
  g_free(data->app_id);
  g_free(data->app_name);
  g_free(data->bus_name);
  g_free(data->app_path);
  whelk_focused_paths_clear(&data->focused);
  g_free(data->shortcut_tool);
  g_free(data);
}

WhelkModule *whelk_menu_module_new(const WhelkModuleSpec *spec,
                                   const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("menu", spec, bar);

  MenuData *data = g_new0(MenuData, 1);

  data->menus = whelk_menubar_new();
  data->buttons = g_ptr_array_new();
  /* The README has always spelled these with the question mark a boolean
   * carries; accept the older spelling too, so neither is silently ignored. */
  data->show_shortcuts =
      whelk_spec_bool(spec, "shortcuts?",
                      whelk_spec_bool(spec, "shortcuts", TRUE));
  data->show_app_name =
      whelk_spec_bool(spec, "app-name?",
                      whelk_spec_bool(spec, "app-name", TRUE));
  data->style = whelk_chord_style_from_name(
      whelk_spec_str(spec, "accelerator-style", "cua"));

  /* wtype types on Wayland, xdotool on X11; #:shortcut-tool overrides the
   * choice, and a session with neither simply gets no shortcut entries. */
  const char *requested = whelk_spec_str(spec, "shortcut-tool", NULL);
  g_autofree char *tool =
      requested ? g_find_program_in_path(requested)
                : whelk_session_shortcut_tool(whelk_session_detect());

  data->shortcut_tool = tool ? g_strdup(tool) : NULL;

  data->row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 2);

  gtk_widget_add_css_class(data->row, "whelk-menu");
  gtk_box_append(GTK_BOX(module->root), data->row);

  module->data = data;
  module->update = menu_update;
  module->destroy = menu_destroy;

  whelk_module_install(module, 1);

  return module;
}
