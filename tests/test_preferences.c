// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The settings window has two halves worth testing separately:
//
//   * the bridge writes a typed value and the next load sees it, which is the
//     whole persistence contract; and
//   * a real widget carries the right preference key and a change reaches that
//     same bridge.
//
// The widget half needs a display.  It is skipped rather than failed on a
// headless runner; the Scheme suite still covers precedence and reset.

#include "config.h"
#include "guile_bridge.h"
#include "preferences.h"

#include <gtk/gtk.h>
#include <glib/gstdio.h>

static char *state_root;

static char *write_settings(void) {
  g_autofree char *dir = g_build_filename(g_get_tmp_dir(), "whelk-prefs-c",
                                          NULL);
  g_mkdir_with_parents(dir, 0700);

  char *path = g_build_filename(dir, "settings.scm", NULL);
  g_autofree char *settings = g_strdup_printf(
      "(configure-shell! #:bars\n"
      " (list (bar #:name 'main #:position 'top #:height 32\n"
      "             #:spacing 8 #:padding 12 #:theme 'gloss-2007\n"
      "             #:right\n"
      "             (list (wifi-module #:show-ssid? #f #:interval 5)\n"
      "                   (notifications-module #:format \"{icon}\")\n"
      "                   (menu-module #:shortcuts? #t #:app-name? #t\n"
      "                                #:accelerator-style 'cua)\n"
      "                   (tray-module #:max-items 6)\n"
      "                   (clock-module #:format \"%%H:%%M\")))))\n");

  g_assert_true(g_file_set_contents(path, settings, -1, NULL));
  return path;
}

static WhelkBarConfig *first_bar(GPtrArray *bars) {
  return bars && bars->len > 0 ? g_ptr_array_index(bars, 0) : NULL;
}

static const WhelkModuleSpec *module_of(const WhelkBarConfig *bar,
                                        const char *kind) {
  const GPtrArray *groups[] = {bar->left, bar->center, bar->right};

  for (gsize g = 0; g < G_N_ELEMENTS(groups); g++) {
    for (guint i = 0; i < groups[g]->len; i++) {
      WhelkModuleSpec *spec = g_ptr_array_index(groups[g], i);

      if (g_strcmp0(spec->kind, kind) == 0)
        return spec;
    }
  }

  return NULL;
}

static void test_bridge_round_trip(void) {
  g_autofree char *path = write_settings();

  g_assert_true(whelk_bridge_load(path));

  GPtrArray *bars = whelk_bridge_bars(path);
  WhelkBarConfig *bar = first_bar(bars);

  g_assert_cmpint(bar->height, ==, 32);
  g_assert_cmpstr(bar->theme, ==, "gloss-2007");
  whelk_config_free_bars(bars);

  g_assert_true(whelk_bridge_save_preference_int(path, "bar-height", 44));
  g_assert_true(whelk_bridge_save_preference_symbol(path, "bar-theme", "frost"));
  g_assert_true(
      whelk_bridge_save_preference_bool(path, "wifi-show-ssid?", TRUE));
  g_assert_true(whelk_bridge_save_preference_int(path, "wifi-interval", 2));
  g_assert_true(whelk_bridge_save_preference_bool(
      path, "notifications-show-count?", TRUE));
  g_assert_true(
      whelk_bridge_save_preference_symbol(path, "menu-accelerator-style",
                                          "emacs"));
  g_assert_true(whelk_bridge_save_preference_int(path, "tray-max-items", 9));
  g_assert_true(whelk_bridge_save_preference_string(path, "clock-format",
                                                    "%H:%M:%S"));

  g_autofree char *prefs = g_build_filename(
      state_root, "whelk", "preferences-settings.scm", NULL);

  g_assert_true(g_file_test(prefs, G_FILE_TEST_EXISTS));

  g_assert_true(whelk_bridge_load(path));
  bars = whelk_bridge_bars(path);
  bar = first_bar(bars);

  g_assert_cmpint(bar->height, ==, 44);
  g_assert_cmpstr(bar->theme, ==, "frost");
  g_assert_cmpint(bar->spacing, ==, 8);
  g_assert_true(
      whelk_spec_bool(module_of(bar, "wifi"), "show-ssid?", FALSE));
  g_assert_cmpint(whelk_spec_int(module_of(bar, "wifi"), "interval", 0), ==, 2);
  g_assert_cmpstr(whelk_spec_str(module_of(bar, "notifications"), "format", ""),
                  ==, "{icon} {count}");
  g_assert_cmpstr(
      whelk_spec_str(module_of(bar, "menu"), "accelerator-style", ""), ==,
      "emacs");
  g_assert_cmpint(whelk_spec_int(module_of(bar, "tray"), "max-items", 0), ==,
                  9);
  g_assert_cmpstr(whelk_spec_str(module_of(bar, "clock"), "format", ""), ==,
                  "%H:%M:%S");
  whelk_config_free_bars(bars);

  g_assert_true(whelk_bridge_reset_preferences(path));
  g_assert_false(g_file_test(prefs, G_FILE_TEST_EXISTS));

  g_assert_true(whelk_bridge_load(path));
  bars = whelk_bridge_bars(path);
  bar = first_bar(bars);

  g_assert_cmpint(bar->height, ==, 32);
  g_assert_cmpstr(bar->theme, ==, "gloss-2007");
  g_assert_cmpint(bar->spacing, ==, 8);
  g_assert_cmpstr(whelk_spec_str(module_of(bar, "notifications"), "format", ""),
                  ==, "{icon}");
  whelk_config_free_bars(bars);

  g_unlink(path);
  g_unlink(prefs);
}

static GtkWidget *find_key(GtkWidget *root, const char *key) {
  for (GtkWidget *child = gtk_widget_get_first_child(root); child;
       child = gtk_widget_get_next_sibling(child)) {
    const char *found = g_object_get_data(G_OBJECT(child), "whelk-pref-key");

    if (g_strcmp0(found, key) == 0)
      return child;

    GtkWidget *nested = find_key(child, key);

    if (nested)
      return nested;
  }

  return NULL;
}

static GtkWidget *find_button(GtkWidget *root, const char *label) {
  for (GtkWidget *child = gtk_widget_get_first_child(root); child;
       child = gtk_widget_get_next_sibling(child)) {
    if (GTK_IS_BUTTON(child) &&
        g_strcmp0(gtk_button_get_label(GTK_BUTTON(child)), label) == 0)
      return child;

    GtkWidget *nested = find_button(child, label);

    if (nested)
      return nested;
  }

  return NULL;
}

static void pump_until_preference_written(void) {
  for (int i = 0; i < 40; i++) {
    while (g_main_context_pending(NULL))
      g_main_context_iteration(NULL, FALSE);
    g_usleep(20000);
  }
}

static void test_widget_controls(void) {
  if (!gtk_init_check()) {
    g_test_skip("no display; the widget half needs a GTK display");
    return;
  }

  const char *css_path = g_getenv("WHELK_CSS_PATH");

  if (css_path) {
    g_autoptr(GtkCssProvider) provider = gtk_css_provider_new();

    gtk_css_provider_load_from_path(provider, css_path);
  }

  g_autofree char *path = write_settings();

  g_assert_true(whelk_bridge_load(path));

  GtkWidget *content =
      whelk_preferences_content_for(path, NULL, NULL);

  g_object_ref_sink(content);

  GtkWidget *height = find_key(content, "bar-height");
  GtkWidget *theme = find_key(content, "bar-theme");
  GtkWidget *count = find_key(content, "notifications-show-count?");
  GtkWidget *reset = NULL;

  g_assert_true(GTK_IS_SPIN_BUTTON(height));
  g_assert_true(GTK_IS_DROP_DOWN(theme));
  g_assert_true(GTK_IS_SWITCH(count));

  gtk_spin_button_set_value(GTK_SPIN_BUTTON(height), 48);
  gtk_switch_set_active(GTK_SWITCH(count), TRUE);

  GListModel *model = gtk_drop_down_get_model(GTK_DROP_DOWN(theme));

  for (guint i = 0; i < g_list_model_get_n_items(model); i++) {
    g_autoptr(GObject) item = g_list_model_get_item(model, i);

    if (g_strcmp0(gtk_string_object_get_string(GTK_STRING_OBJECT(item)),
                  "frost") == 0) {
      gtk_drop_down_set_selected(GTK_DROP_DOWN(theme), i);
      break;
    }
  }

  /* Find the reset button by label, not by position: a layout change should
   * not be able to turn this test green by accident. */
  reset = find_button(content, "Reset GUI settings");

  pump_until_preference_written();

  g_assert_true(whelk_bridge_load(path));

  GPtrArray *bars = whelk_bridge_bars(path);
  WhelkBarConfig *bar = first_bar(bars);

  g_assert_cmpint(bar->height, ==, 48);
  g_assert_cmpstr(bar->theme, ==, "frost");
  g_assert_cmpstr(whelk_spec_str(module_of(bar, "notifications"), "format", ""),
                  ==, "{icon} {count}");
  whelk_config_free_bars(bars);

  /* Reset is the same path the user takes; it clears the file and rebuilds the
   * controls from the settings file. */
  g_assert_true(GTK_IS_BUTTON(reset));
  g_signal_emit_by_name(reset, "clicked");
  pump_until_preference_written();

  g_assert_true(whelk_bridge_load(path));
  bars = whelk_bridge_bars(path);
  bar = first_bar(bars);
  g_assert_cmpint(bar->height, ==, 32);
  g_assert_cmpstr(bar->theme, ==, "gloss-2007");
  whelk_config_free_bars(bars);

  g_object_unref(content);
  g_unlink(path);
  g_autofree char *prefs = g_build_filename(
      state_root, "whelk", "preferences-settings.scm", NULL);
  g_unlink(prefs);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_autofree char *root = g_dir_make_tmp("whelk-preferences-XXXXXX", NULL);
  g_assert_nonnull(root);
  state_root = g_strdup(root);
  g_setenv("XDG_STATE_HOME", root, TRUE);
  g_setenv("HOME", root, TRUE);

  whelk_bridge_start_guile();
  whelk_bridge_setup_load_path();
  g_assert_true(whelk_bridge_use_core());

  g_test_add_func("/preferences/bridge-round-trip", test_bridge_round_trip);
  g_test_add_func("/preferences/widget-controls", test_widget_controls);

  int status = g_test_run();

  g_free(state_root);
  return status;
}
