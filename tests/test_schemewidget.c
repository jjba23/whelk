// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The other half of (scheme-module ...): a procedure returns a real GTK
// widget, the bridge hands it over, and the widget survives the Guile call.
// GTK needs a display, so a runner without one skips these checks instead of
// pretending the drawing half is covered.

#include "config.h"
#include "guile_bridge.h"
#include "style.h"

#include <gtk/gtk.h>
#include <glib/gstdio.h>

static void test_scheme_widgets(void) {
  if (!gtk_init_check()) {
    g_test_skip("no display for GTK widget creation");
    return;
  }

  g_autofree char *directory =
      g_dir_make_tmp("whelk-scheme-widgets-XXXXXX", NULL);

  g_assert_nonnull(directory);

  g_autofree char *path = g_build_filename(directory, "settings.scm", NULL);
  const char *settings =
      "(configure-shell!\n"
      " #:bars\n"
      " (list (bar #:left\n"
      "             (list (scheme-module\n"
      "                     #:update\n"
      "                     (lambda ()\n"
      "                       (whelk-widget-box\n"
      "                        'horizontal\n"
      "                        (whelk-widget-label \"hello\")\n"
      "                        (whelk-widget-sparkline '(1 2 3) 40 10)\n"
      "                        (whelk-widget-progress 0.5))))))))\n";

  g_assert_true(g_file_set_contents(path, settings, -1, NULL));
  g_assert_true(whelk_bridge_load(path));

  GPtrArray *bars = whelk_bridge_bars(path);

  g_assert_cmpuint(bars->len, ==, 1);

  WhelkBarConfig *bar = g_ptr_array_index(bars, 0);

  g_assert_cmpuint(bar->left->len, ==, 1);

  WhelkModuleSpec *spec = g_ptr_array_index(bar->left, 0);
  const char *token = whelk_spec_str(spec, "update", NULL);

  g_assert_nonnull(token);

  GtkWidget *widget = whelk_bridge_call_module_widget(token);

  g_assert_nonnull(widget);
  g_assert_true(GTK_IS_BOX(widget));

  guint children = 0;

  for (GtkWidget *child = gtk_widget_get_first_child(widget); child;
       child = gtk_widget_get_next_sibling(child))
    children++;

  g_assert_cmpuint(children, ==, 3);

  GtkWidget *first = gtk_widget_get_first_child(widget);

  g_assert_true(GTK_IS_LABEL(first));
  g_assert_cmpstr(gtk_label_get_text(GTK_LABEL(first)), ==, "hello");

  GtkWidget *second = gtk_widget_get_next_sibling(first);

  g_assert_true(GTK_IS_DRAWING_AREA(second));

  GtkWidget *third = gtk_widget_get_next_sibling(second);

  g_assert_true(GTK_IS_PROGRESS_BAR(third));
  g_assert_cmpfloat_with_epsilon(
      gtk_progress_bar_get_fraction(GTK_PROGRESS_BAR(third)), 0.5, 0.001);

  g_object_unref(widget);

  whelk_config_free_bars(bars);
  g_unlink(path);
  g_rmdir(directory);
}

static void test_description_widget(void) {
  if (!gtk_init_check()) {
    g_test_skip("no display for GTK widget creation");
    return;
  }

  g_autofree char *directory =
      g_dir_make_tmp("whelk-scheme-description-XXXXXX", NULL);

  g_assert_nonnull(directory);

  g_autofree char *path = g_build_filename(directory, "settings.scm", NULL);
  const char *settings =
      "(configure-shell!\n"
      " #:bars\n"
      " (list (bar #:right\n"
      "             (list (scheme-module\n"
      "                     #:update\n"
      "                     (lambda () '(label \"described\")))))))\n";

  g_assert_true(g_file_set_contents(path, settings, -1, NULL));
  g_assert_true(whelk_bridge_load(path));

  GPtrArray *bars = whelk_bridge_bars(path);
  WhelkBarConfig *bar = g_ptr_array_index(bars, 0);
  WhelkModuleSpec *spec = g_ptr_array_index(bar->right, 0);
  GtkWidget *widget = whelk_bridge_call_module_widget(
      whelk_spec_str(spec, "update", NULL));

  g_assert_nonnull(widget);
  g_assert_true(GTK_IS_LABEL(widget));
  g_assert_cmpstr(gtk_label_get_text(GTK_LABEL(widget)), ==, "described");

  g_object_unref(widget);
  whelk_config_free_bars(bars);
  g_unlink(path);
  g_rmdir(directory);
}

static void test_pointer_cursors(void) {
  if (!gtk_init_check()) {
    g_test_skip("no display for GTK cursor creation");
    return;
  }

  GtkWidget *box =
      g_object_ref_sink(gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0));
  GtkWidget *button = gtk_button_new_with_label("ok");
  GtkWidget *disabled = gtk_button_new_with_label("later");
  GtkWidget *label = gtk_label_new("text");

  gtk_widget_set_sensitive(disabled, FALSE);
  gtk_box_append(GTK_BOX(box), button);
  gtk_box_append(GTK_BOX(box), disabled);
  gtk_box_append(GTK_BOX(box), label);

  whelk_style_pointer_cursors(box);

  GdkCursor *cursor = gtk_widget_get_cursor(button);

  g_assert_nonnull(cursor);
  g_assert_cmpstr(gdk_cursor_get_name(cursor), ==, "pointer");
  g_assert_null(gtk_widget_get_cursor(disabled));
  g_assert_null(gtk_widget_get_cursor(label));

  g_object_unref(box);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  whelk_bridge_start_guile();
  whelk_bridge_setup_load_path();
  g_assert_true(whelk_bridge_use_core());

  g_test_add_func("/scheme/widgets", test_scheme_widgets);
  g_test_add_func("/scheme/description", test_description_widget);
  g_test_add_func("/style/pointer-cursors", test_pointer_cursors);

  return g_test_run();
}
