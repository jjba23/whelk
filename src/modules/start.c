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
 * one glyph.  The glyph is the configuration's; the default is the whelk the
 * shell is named after.
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

WhelkModule *whelk_start_module_new(const WhelkModuleSpec *spec,
                                    const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("start", spec, bar);

  gtk_widget_add_css_class(module->root, "whelk-start");

  GtkWidget *button = gtk_menu_button_new();

  gtk_widget_add_css_class(button, "flat");
  gtk_widget_add_css_class(button, "whelk-start-button");
  /* #:label is a glyph, so it is drawn in the bar's icon font: a star, a
   * snail or a grid only exists in the face the configuration named. */
  const char *glyph = whelk_spec_str(spec, "label", "\xf0\x9f\x90\x9a");
  g_autofree char *markup =
      whelk_icon_markup(glyph, bar ? bar->icon_font : NULL);
  GtkWidget *label = gtk_label_new(NULL);

  gtk_label_set_markup(GTK_LABEL(label), markup);
  gtk_menu_button_set_child(GTK_MENU_BUTTON(button), label);
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
