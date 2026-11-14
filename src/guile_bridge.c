// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "guile_bridge.h"

#include <libguile.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "logger.h"
#include "util/chord.h"
#include "util/menudict.h"
#include "util/menutree.h"

/* =========================================================================
 * CALLING INTO SCHEME
 * ========================================================================= */

/* Looks up an exported procedure.  A missing module or binding is a warning
 * rather than an abort: the C defaults are a working shell. */
static SCM public_proc(const char *module_name, const char *name) {
  SCM proc = scm_c_public_ref(module_name, name);

  if (!scm_is_true(proc) || !scm_is_true(scm_procedure_p(proc))) {
    LOG_C_DEBUG("Guile", "(%s) has no procedure %s", module_name, name);
    return SCM_BOOL_F;
  }

  return proc;
}

static SCM call0(const char *module_name, const char *name) {
  SCM proc = public_proc(module_name, name);

  return scm_is_true(proc) ? scm_call_0(proc) : SCM_BOOL_F;
}

static SCM call1(const char *module_name, const char *name, SCM argument) {
  SCM proc = public_proc(module_name, name);

  return scm_is_true(proc) ? scm_call_1(proc, argument) : SCM_BOOL_F;
}

/* The path argument, or no argument at all when there is no path. */
static SCM call_with_path(const char *module_name, const char *name,
                          const char *path) {
  if (path && *path != '\0')
    return call1(module_name, name, scm_from_utf8_string(path));

  return call0(module_name, name);
}

/* =========================================================================
 * CONVERTING SCHEME VALUES
 * =========================================================================
 * The Scheme side hands over alists of plain values (see `bar->alist' in
 * scheme/whelk/state.scm): SRFI-9 accessors are macros, and a macro is not
 * something this file can call.  Everything below is therefore an alist walk.
 */

/* A string or a symbol as a fresh C string, or NULL. */
static char *scm_text(SCM value) {
  SCM text = SCM_BOOL_F;

  if (scm_is_string(value))
    text = value;
  else if (scm_is_symbol(value))
    text = scm_symbol_to_string(value);
  else
    return NULL;

  char *utf8 = scm_to_utf8_string(text);

  if (!utf8)
    return NULL;

  char *copy = g_strdup(utf8);
  free(utf8);

  return copy;
}

/* One option value as text.  Numbers, booleans and symbols keep their Scheme
 * spelling; the typed readers in config.c parse them back. */
static char *scm_value_text(SCM value) {
  char *text = scm_text(value);

  if (text)
    return text;

  if (scm_is_bool(value))
    return g_strdup(scm_is_true(value) ? "#t" : "#f");

  if (scm_is_number(value)) {
    if (scm_is_exact_integer(value))
      return g_strdup_printf("%ld", (long)scm_to_long(value));

    return g_strdup_printf("%g", scm_to_double(value));
  }

  if (scm_is_null(value))
    return g_strdup("");

  if (scm_is_pair(value)) {
    /* A list of values becomes one comma-separated option, which is what a
     * module that wants "all the colours" can split on. */
    GString *joined = g_string_new(NULL);
    SCM cursor = value;

    while (scm_is_pair(cursor)) {
      g_autofree char *item = scm_value_text(SCM_CAR(cursor));

      if (item) {
        if (joined->len > 0)
          g_string_append_c(joined, ',');
        g_string_append(joined, item);
      }

      cursor = SCM_CDR(cursor);
    }

    return g_string_free(joined, FALSE);
  }

  LOG_C_DEBUG("Guile", "an option of an unsupported type is ignored");
  return NULL;
}

/* --- alists --------------------------------------------------------------- */

static SCM alist_get(SCM alist, const char *key) {
  SCM symbol = scm_from_utf8_symbol(key);

  for (SCM cursor = alist; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    SCM entry = SCM_CAR(cursor);

    if (scm_is_pair(entry) && scm_is_eq(SCM_CAR(entry), symbol))
      return SCM_CDR(entry);
  }

  return SCM_BOOL_F;
}

static char *alist_text(SCM alist, const char *key, const char *fallback) {
  char *text = scm_text(alist_get(alist, key));

  return (text && *text != '\0') ? text : g_strdup(fallback);
}

/* An option that is genuinely optional: absent or #f answers NULL. */
static char *alist_optional_text(SCM alist, const char *key) {
  return scm_text(alist_get(alist, key));
}

static int alist_int(SCM alist, const char *key, int fallback) {
  SCM value = alist_get(alist, key);

  return scm_is_number(value) ? (int)scm_to_long(value) : fallback;
}

static gboolean alist_bool(SCM alist, const char *key, gboolean fallback) {
  SCM value = alist_get(alist, key);

  if (scm_is_eq(value, SCM_BOOL_T))
    return TRUE;
  if (scm_is_eq(value, SCM_BOOL_F))
    return FALSE;

  return fallback;
}

static GtkLayerShellEdge edge_from_text(const char *text) {
  if (g_strcmp0(text, "bottom") == 0)
    return GTK_LAYER_SHELL_EDGE_BOTTOM;
  if (g_strcmp0(text, "left") == 0)
    return GTK_LAYER_SHELL_EDGE_LEFT;
  if (g_strcmp0(text, "right") == 0)
    return GTK_LAYER_SHELL_EDGE_RIGHT;

  return GTK_LAYER_SHELL_EDGE_TOP;
}

static GtkLayerShellLayer layer_from_text(const char *text) {
  if (g_strcmp0(text, "background") == 0)
    return GTK_LAYER_SHELL_LAYER_BACKGROUND;
  if (g_strcmp0(text, "bottom") == 0)
    return GTK_LAYER_SHELL_LAYER_BOTTOM;
  if (g_strcmp0(text, "overlay") == 0)
    return GTK_LAYER_SHELL_LAYER_OVERLAY;

  return GTK_LAYER_SHELL_LAYER_TOP;
}

/* Only a non-empty option is a configuration; an absent one is the module's
 * own default. */
static void collect_options(WhelkModuleSpec *spec, SCM alist) {
  for (SCM cursor = alist; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    SCM pair = SCM_CAR(cursor);

    if (!scm_is_pair(pair))
      continue;

    g_autofree char *key = scm_text(SCM_CAR(pair));
    g_autofree char *value = scm_value_text(SCM_CDR(pair));

    if (key && value && *key != '\0' && *value != '\0')
      whelk_module_spec_set(spec, key, value);
  }
}

static void collect_modules(GPtrArray *target, SCM list) {
  for (SCM cursor = list; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    SCM entry = SCM_CAR(cursor);
    g_autofree char *kind = scm_text(alist_get(entry, "kind"));

    if (!kind || *kind == '\0')
      continue;

    WhelkModuleSpec *spec = whelk_module_spec_new(kind);

    collect_options(spec, alist_get(entry, "options"));
    g_ptr_array_add(target, spec);
  }
}

static WhelkBarConfig *bar_from_alist(SCM alist) {
  g_autofree char *name = alist_text(alist, "name", "main");
  g_autofree char *edge = alist_text(alist, "edge", "top");
  g_autofree char *layer = alist_text(alist, "layer", "top");
  WhelkBarConfig *bar = whelk_bar_config_new(name);

  bar->theme = alist_optional_text(alist, "theme");
  bar->edge = edge_from_text(edge);
  bar->layer = layer_from_text(layer);
  bar->height = alist_int(alist, "height", 32);
  bar->margin_top = alist_int(alist, "margin-top", 0);
  bar->margin_right = alist_int(alist, "margin-right", 0);
  bar->margin_bottom = alist_int(alist, "margin-bottom", 0);
  bar->margin_left = alist_int(alist, "margin-left", 0);
  bar->exclusive = alist_bool(alist, "exclusive?", TRUE);
  bar->spacing = alist_int(alist, "spacing", 6);
  bar->padding = alist_int(alist, "padding", 12);
  bar->radius = alist_int(alist, "radius", 0);

  g_free(bar->monitor);
  bar->monitor = alist_text(alist, "monitor", "all");

  g_autofree char *when = alist_text(alist, "when", "always");

  bar->when = whelk_bar_when_from_text(when);

  /* A list of connectors is the deliberate form: each output in it gets this
   * bar, and an output that is not in it gets none. */
  SCM monitors = alist_get(alist, "monitors");
  gboolean monitors_all = FALSE;

  for (SCM cursor = monitors; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    g_autofree char *connector = scm_text(SCM_CAR(cursor));

    if (!connector || *connector == '\0')
      continue;

    if (g_strcmp0(connector, "all") == 0) {
      monitors_all = TRUE;
      break;
    }

    g_ptr_array_add(bar->monitors, g_strdup(connector));
  }

  /* "all" in a list is the same rule as #:monitor "all". */
  if (monitors_all) {
    g_ptr_array_set_size(bar->monitors, 0);
    g_free(bar->monitor);
    bar->monitor = g_strdup("all");
  }

  bar->font = alist_optional_text(alist, "font");
  bar->font_size = alist_optional_text(alist, "font-size");
  bar->icon_font = alist_optional_text(alist, "icon-font");
  bar->foreground = alist_optional_text(alist, "foreground");
  bar->background = alist_optional_text(alist, "background");
  bar->accent = alist_optional_text(alist, "accent");
  bar->border_color = alist_optional_text(alist, "border-color");
  bar->css = alist_optional_text(alist, "css");

  collect_modules(bar->left, alist_get(alist, "modules-left"));
  collect_modules(bar->center, alist_get(alist, "modules-center"));
  collect_modules(bar->right, alist_get(alist, "modules-right"));

  return bar;
}

/* =========================================================================
 * WIDGETS FROM SCHEME
 * =========================================================================
 * A `scheme-module' returns a GTK widget.  Guile has no GTK bindings here, so
 * the primitives below are the small, deliberate surface through which a
 * settings file builds one: a label, markup, a progress bar, a level bar, a
 * sparkline, an icon, a box and a button.  A procedure may also return a
 * plain string or a list such as `(label "42")' for the same widgets; the
 * explicit primitives exist because they make the common case read like the
 * thing it creates and because a button needs a Scheme closure.
 *
 * Ownership is explicit.  A widget created by a primitive lives in a smob
 * until the module takes it; taking it clears the smob's ownership, so a GC
 * after the handover cannot free a widget the bar is drawing.  A smob that is
 * never taken destroys its widget when Guile collects it.
 */

typedef struct {
  GtkWidget *widget;
  gboolean transferred;
} ScmWidget;

static scm_t_bits scm_widget_tag;

static char *scm_text_copy(SCM value) {
  SCM text = value;

  if (scm_is_symbol(value))
    text = scm_symbol_to_string(value);
  else if (!scm_is_string(value))
    return NULL;

  char *raw = scm_to_utf8_string(text);

  if (!raw)
    return NULL;

  char *copy = g_strdup(raw);

  free(raw);
  return copy;
}

static size_t scm_widget_free(SCM smob) {
  ScmWidget *box = (ScmWidget *)SCM_SMOB_DATA(smob);

  if (box) {
    if (!box->transferred && box->widget) {
      if (gtk_widget_get_parent(box->widget))
        gtk_widget_unparent(box->widget);

      g_object_unref(box->widget);
    }

    scm_gc_free(box, sizeof *box, "whelk widget");
  }

  return 0;
}

static SCM widget_smob(GtkWidget *widget) {
  ScmWidget *box = scm_gc_malloc(sizeof *box, "whelk widget");

  box->widget = widget;
  box->transferred = FALSE;

  /* The smob owns a real reference, so a widget that left a box can still be
   * handed back by the procedure and outlive the expression that made it. */
  g_object_ref_sink(widget);

  SCM_RETURN_NEWSMOB(scm_widget_tag, box);
}

static GtkWidget *widget_from_description(SCM value);

/* Takes the widget out of VALUE, or builds it from a description.  The caller
 * owns a reference and must release it after appending the widget. */
static GtkWidget *take_widget(SCM value) {
  if (SCM_SMOB_PREDICATE(scm_widget_tag, value)) {
    ScmWidget *box = (ScmWidget *)SCM_SMOB_DATA(value);

    if (!box || box->transferred)
      return NULL;

    box->transferred = TRUE;
    return box->widget;
  }

  GtkWidget *widget = widget_from_description(value);

  if (widget)
    g_object_ref_sink(widget);

  return widget;
}

static GtkWidget *label_widget(const char *text, const char *css) {
  GtkWidget *label = gtk_label_new(text ? text : "");

  gtk_widget_set_valign(label, GTK_ALIGN_CENTER);

  if (css && *css)
    gtk_widget_add_css_class(label, css);

  return label;
}

typedef struct {
  double *values;
  guint count;
  GdkRGBA ink;
} Sparkline;

static void sparkline_free(gpointer data) {
  Sparkline *spark = data;

  if (!spark)
    return;

  g_free(spark->values);
  g_free(spark);
}

static void draw_sparkline(GtkDrawingArea *area, cairo_t *cr, int width,
                           int height, gpointer data) {
  (void)area;

  Sparkline *spark = data;

  if (!spark || spark->count == 0 || width <= 2 || height <= 2)
    return;

  double minimum = spark->values[0];
  double maximum = spark->values[0];

  for (guint i = 1; i < spark->count; i++) {
    minimum = MIN(minimum, spark->values[i]);
    maximum = MAX(maximum, spark->values[i]);
  }

  if (maximum - minimum < 0.000001)
    maximum = minimum + 1.0;

  const double step = spark->count > 1
                          ? (double)(width - 2) / (double)(spark->count - 1)
                          : 0.0;

  for (guint i = 0; i < spark->count; i++) {
    const double x = 1.0 + step * (double)i;
    const double y = (double)(height - 1) -
                     ((spark->values[i] - minimum) / (maximum - minimum)) *
                         (double)(height - 2);

    if (i == 0)
      cairo_move_to(cr, x, y);
    else
      cairo_line_to(cr, x, y);
  }

  cairo_set_line_width(cr, 1.4);
  cairo_set_line_join(cr, CAIRO_LINE_JOIN_ROUND);
  cairo_set_source_rgba(cr, spark->ink.red, spark->ink.green, spark->ink.blue,
                        0.92);
  cairo_stroke_preserve(cr);

  cairo_line_to(cr, (double)(width - 1), (double)(height - 1));
  cairo_line_to(cr, 1.0, (double)(height - 1));
  cairo_close_path(cr);
  cairo_set_source_rgba(cr, spark->ink.red, spark->ink.green, spark->ink.blue,
                        0.16);
  cairo_fill(cr);
}

static GtkWidget *sparkline_widget(SCM values, int width, int height,
                                   const char *color) {
  if (!scm_is_pair(values) && !scm_is_null(values))
    return NULL;

  guint count = 0;

  for (SCM cursor = values; scm_is_pair(cursor); cursor = SCM_CDR(cursor))
    if (scm_is_number(SCM_CAR(cursor)))
      count++;

  if (count == 0)
    return NULL;

  Sparkline *spark = g_new0(Sparkline, 1);

  spark->values = g_new0(double, count);
  spark->count = count;
  spark->ink = (GdkRGBA){0.92, 0.92, 0.94, 1.0};

  if (color && *color)
    gdk_rgba_parse(&spark->ink, color);

  guint at = 0;

  for (SCM cursor = values; scm_is_pair(cursor) && at < count;
       cursor = SCM_CDR(cursor))
    if (scm_is_number(SCM_CAR(cursor)))
      spark->values[at++] = scm_to_double(SCM_CAR(cursor));

  GtkWidget *area = gtk_drawing_area_new();

  gtk_widget_add_css_class(area, "whelk-scheme-sparkline");
  gtk_widget_set_size_request(area, width, height);
  gtk_drawing_area_set_content_width(GTK_DRAWING_AREA(area), width);
  gtk_drawing_area_set_content_height(GTK_DRAWING_AREA(area), height);
  gtk_drawing_area_set_draw_func(GTK_DRAWING_AREA(area), draw_sparkline, spark,
                                 sparkline_free);

  return area;
}

typedef struct {
  SCM proc;
} ScmButton;

static void scm_button_free(gpointer data, GClosure *closure) {
  (void)closure;

  ScmButton *button = data;

  if (!button)
    return;

  scm_gc_unprotect_object(button->proc);
  g_free(button);
}

static SCM call_scm_button(void *data) {
  return scm_call_0(*(SCM *)data);
}

static SCM on_scm_button_error(void *data, SCM tag, SCM args) {
  (void)data;
  (void)tag;
  (void)args;

  LOG_C_WARN("Guile", "a scheme-module button's procedure raised");

  return SCM_BOOL_F;
}

static void on_scm_button_clicked(GtkButton *button, gpointer data) {
  (void)button;

  ScmButton *context = data;

  scm_internal_catch(SCM_BOOL_T, call_scm_button, &context->proc,
                     on_scm_button_error, NULL);
}

static GtkWidget *button_widget(const char *label, SCM proc) {
  GtkWidget *button = gtk_button_new_with_label(label ? label : "");
  ScmButton *context = g_new0(ScmButton, 1);

  context->proc = proc;
  scm_gc_protect_object(proc);
  g_signal_connect_data(button, "clicked", G_CALLBACK(on_scm_button_clicked),
                        context, scm_button_free, 0);

  return button;
}

static GtkWidget *box_widget(SCM orientation, SCM children) {
  g_autofree char *direction = scm_text_copy(orientation);
  const GtkOrientation horizontal =
      (direction && g_strcmp0(direction, "vertical") == 0)
          ? GTK_ORIENTATION_VERTICAL
          : GTK_ORIENTATION_HORIZONTAL;
  GtkWidget *box = gtk_box_new(horizontal, 6);

  gtk_widget_set_valign(box, GTK_ALIGN_CENTER);

  for (SCM cursor = children; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    GtkWidget *child = take_widget(SCM_CAR(cursor));

    if (!child)
      continue;

    gtk_box_append(GTK_BOX(box), child);
    g_object_unref(child);
  }

  return box;
}

static GtkWidget *widget_from_description(SCM value) {
  if (scm_is_string(value) || scm_is_number(value) || scm_is_symbol(value)) {
    g_autofree char *text = scm_is_number(value)
                                ? g_strdup_printf("%g", scm_to_double(value))
                                : scm_text_copy(value);

    return label_widget(text, NULL);
  }

  if (!scm_is_pair(value))
    return NULL;

  g_autofree char *kind = scm_text_copy(SCM_CAR(value));

  if (!kind)
    return NULL;

  SCM args = SCM_CDR(value);

  if (g_strcmp0(kind, "label") == 0 || g_strcmp0(kind, "markup") == 0) {
    g_autofree char *text = scm_is_pair(args) ? scm_text_copy(SCM_CAR(args))
                                              : NULL;
    g_autofree char *css =
        (scm_is_pair(args) && scm_is_pair(SCM_CDR(args)))
            ? scm_text_copy(SCM_CADR(args))
            : NULL;
    GtkWidget *label = label_widget(NULL, css);

    if (g_strcmp0(kind, "markup") == 0)
      gtk_label_set_markup(GTK_LABEL(label), text ? text : "");
    else
      gtk_label_set_text(GTK_LABEL(label), text ? text : "");

    return label;
  }

  if (g_strcmp0(kind, "progress") == 0 || g_strcmp0(kind, "level") == 0) {
    const double fraction =
        scm_is_pair(args) && scm_is_number(SCM_CAR(args))
            ? CLAMP(scm_to_double(SCM_CAR(args)), 0.0, 1.0)
            : 0.0;

    if (g_strcmp0(kind, "level") == 0) {
      GtkWidget *level = gtk_level_bar_new_for_interval(0.0, 1.0);

      gtk_widget_add_css_class(level, "whelk-scheme-level");
      gtk_level_bar_set_value(GTK_LEVEL_BAR(level), fraction);
      gtk_widget_set_hexpand(level, TRUE);
      return level;
    }

    GtkWidget *progress = gtk_progress_bar_new();

    gtk_widget_add_css_class(progress, "whelk-scheme-progress");
    gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(progress), fraction);
    gtk_progress_bar_set_show_text(GTK_PROGRESS_BAR(progress), FALSE);
    gtk_widget_set_hexpand(progress, TRUE);
    return progress;
  }

  if (g_strcmp0(kind, "sparkline") == 0 && scm_is_pair(args)) {
    const int width =
        scm_is_pair(SCM_CDR(args)) && scm_is_number(SCM_CADR(args))
            ? scm_to_int(SCM_CADR(args))
            : 90;
    const int height =
        scm_is_pair(SCM_CDR(args)) && scm_is_pair(SCM_CDDR(args)) &&
                scm_is_number(SCM_CADDR(args))
            ? scm_to_int(SCM_CADDR(args))
            : 22;
    g_autofree char *color =
        scm_is_pair(SCM_CDR(args)) && scm_is_pair(SCM_CDDR(args)) &&
                scm_is_pair(SCM_CDDDR(args))
            ? scm_text_copy(SCM_CADDDR(args))
            : NULL;

    return sparkline_widget(SCM_CAR(args), MAX(16, width), MAX(8, height),
                            color);
  }

  if (g_strcmp0(kind, "image") == 0 && scm_is_pair(args)) {
    g_autofree char *name = scm_text_copy(SCM_CAR(args));
    const int size =
        scm_is_pair(SCM_CDR(args)) && scm_is_number(SCM_CADR(args))
            ? scm_to_int(SCM_CADR(args))
            : 16;
    GtkWidget *image = gtk_image_new_from_icon_name(name ? name : "") ;

    gtk_image_set_pixel_size(GTK_IMAGE(image), size);
    return image;
  }

  if (g_strcmp0(kind, "box") == 0 && scm_is_pair(args))
    return box_widget(SCM_CAR(args), SCM_CDR(args));

  if (g_strcmp0(kind, "button") == 0 && scm_is_pair(args) &&
      scm_is_pair(SCM_CDR(args))) {
    g_autofree char *label = scm_text_copy(SCM_CAR(args));
    SCM proc = SCM_CADR(args);

    if (scm_is_true(scm_procedure_p(proc)))
      return button_widget(label, proc);
  }

  return NULL;
}

/* --- the primitives ------------------------------------------------------- */

static SCM scm_widget_label(SCM text, SCM css) {
  g_autofree char *value = scm_text_copy(text);
  g_autofree char *class = scm_text_copy(css);

  return widget_smob(label_widget(value, class));
}

static SCM scm_widget_markup(SCM markup, SCM css) {
  g_autofree char *value = scm_text_copy(markup);
  g_autofree char *class = scm_text_copy(css);
  GtkWidget *label = label_widget(NULL, class);

  gtk_label_set_markup(GTK_LABEL(label), value ? value : "");
  return widget_smob(label);
}

static SCM scm_widget_progress(SCM fraction) {
  const double value = scm_is_number(fraction) ? scm_to_double(fraction) : 0.0;
  GtkWidget *progress = gtk_progress_bar_new();

  gtk_widget_add_css_class(progress, "whelk-scheme-progress");
  gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(progress),
                                CLAMP(value, 0.0, 1.0));
  gtk_progress_bar_set_show_text(GTK_PROGRESS_BAR(progress), FALSE);
  gtk_widget_set_hexpand(progress, TRUE);

  return widget_smob(progress);
}

static SCM scm_widget_level(SCM fraction) {
  const double value = scm_is_number(fraction) ? scm_to_double(fraction) : 0.0;
  GtkWidget *level = gtk_level_bar_new_for_interval(0.0, 1.0);

  gtk_widget_add_css_class(level, "whelk-scheme-level");
  gtk_level_bar_set_value(GTK_LEVEL_BAR(level), CLAMP(value, 0.0, 1.0));
  gtk_widget_set_hexpand(level, TRUE);

  return widget_smob(level);
}

static SCM scm_widget_sparkline(SCM values, SCM width, SCM height, SCM color) {
  const int w = scm_is_number(width) ? scm_to_int(width) : 90;
  const int h = scm_is_number(height) ? scm_to_int(height) : 22;
  g_autofree char *ink = scm_text_copy(color);
  GtkWidget *area = sparkline_widget(values, MAX(16, w), MAX(8, h), ink);

  return area ? widget_smob(area) : SCM_BOOL_F;
}

static SCM scm_widget_image(SCM name, SCM size) {
  g_autofree char *icon = scm_text_copy(name);
  const int pixels = scm_is_number(size) ? scm_to_int(size) : 16;
  GtkWidget *image = gtk_image_new_from_icon_name(icon ? icon : "");

  gtk_image_set_pixel_size(GTK_IMAGE(image), pixels);
  return widget_smob(image);
}

static SCM scm_widget_box(SCM orientation, SCM children) {
  return widget_smob(box_widget(orientation, children));
}

static SCM scm_widget_button(SCM label, SCM proc) {
  g_autofree char *text = scm_text_copy(label);

  if (!scm_is_true(scm_procedure_p(proc)))
    return SCM_BOOL_F;

  return widget_smob(button_widget(text, proc));
}

static SCM scm_widget_add_class(SCM widget, SCM css) {
  g_autofree char *class = scm_text_copy(css);

  if (!SCM_SMOB_PREDICATE(scm_widget_tag, widget) || !class)
    return widget;

  ScmWidget *box = (ScmWidget *)SCM_SMOB_DATA(widget);

  if (box && box->widget)
    gtk_widget_add_css_class(box->widget, class);

  return widget;
}

static void define_widget_primitive(const char *name, scm_t_subr fn, int required,
                                    int optional, int rest) {
  scm_c_define_gsubr(name, required, optional, rest, fn);
  scm_c_export(name, NULL);
}

void whelk_bridge_register_widget_primitives(void) {
  static gboolean registered = FALSE;

  if (registered)
    return;

  SCM module = scm_c_resolve_module("whelk core");
  SCM previous = scm_set_current_module(module);

  scm_widget_tag = scm_make_smob_type("whelk-widget", sizeof(ScmWidget));
  scm_set_smob_free(scm_widget_tag, scm_widget_free);

  define_widget_primitive("whelk-widget-label",
                          (scm_t_subr)(intptr_t)scm_widget_label, 1, 1, 0);
  define_widget_primitive("whelk-widget-markup",
                          (scm_t_subr)(intptr_t)scm_widget_markup, 1, 1, 0);
  define_widget_primitive("whelk-widget-progress",
                          (scm_t_subr)(intptr_t)scm_widget_progress, 1, 0, 0);
  define_widget_primitive("whelk-widget-level",
                          (scm_t_subr)(intptr_t)scm_widget_level, 1, 0, 0);
  define_widget_primitive("whelk-widget-sparkline",
                          (scm_t_subr)(intptr_t)scm_widget_sparkline, 1, 3, 0);
  define_widget_primitive("whelk-widget-image",
                          (scm_t_subr)(intptr_t)scm_widget_image, 1, 1, 0);
  define_widget_primitive("whelk-widget-box",
                          (scm_t_subr)(intptr_t)scm_widget_box, 1, 0, 1);
  define_widget_primitive("whelk-widget-button",
                          (scm_t_subr)(intptr_t)scm_widget_button, 2, 0, 0);
  define_widget_primitive("whelk-widget-add-class",
                          (scm_t_subr)(intptr_t)scm_widget_add_class, 2, 0, 0);

  scm_set_current_module(previous);
  registered = TRUE;
}

static SCM call_module_widget_thunk(void *data) {
  return call1("whelk modules", "call-module-procedure",
               scm_from_utf8_string((const char *)data));
}

static SCM on_module_widget_error(void *data, SCM tag, SCM args) {
  (void)data;
  (void)tag;
  (void)args;

  LOG_C_WARN("Guile",
             "a scheme-module update raised: keeping its previous widget");

  return SCM_BOOL_F;
}

GtkWidget *whelk_bridge_call_module_widget(const char *token) {
  if (!token || *token == '\0')
    return NULL;

  SCM result = scm_internal_catch(SCM_BOOL_T, call_module_widget_thunk,
                                  (void *)token, on_module_widget_error, NULL);

  if (scm_is_eq(result, SCM_BOOL_F) || scm_is_eq(result, SCM_UNSPECIFIED) ||
      scm_is_null(result))
    return NULL;

  GtkWidget *widget = take_widget(result);

  if (!widget)
    LOG_C_DEBUG("Guile",
                "a scheme-module update returned something that is not a "
                "widget");

  return widget;
}

/* =========================================================================
 * THE PUBLIC BRIDGE
 * ========================================================================= */

void whelk_bridge_start_guile(void) {
  static gboolean started = FALSE;

  if (started)
    return;

  scm_init_guile();
  started = TRUE;
}

void whelk_bridge_setup_load_path(void) {
  /* The checkout's own modules first: running from the build tree must read
   * the tree, not a stale copy installed under the Guile site directory. */
  scm_c_eval_string("(set! %load-path (cons \"scheme\" %load-path))");

#ifdef WHELK_SCHEME_DIR
  scm_c_eval_string("(set! %load-path (cons \"" WHELK_SCHEME_DIR
                    "\" %load-path))");
#endif
}

/* Loads the module; a missing or broken one is a warning, not a crash. */
static SCM use_module_thunk(void *data) {
  scm_c_use_module((const char *)data);
  return SCM_BOOL_T;
}

static SCM on_use_core_error(void *data, SCM tag, SCM args) {
  (void)data;
  (void)tag;
  (void)args;

  LOG_C_WARN("Guile",
             "could not load (whelk core): the Scheme modules are not on the "
             "load path; run whelk from the project root or install it");

  return SCM_BOOL_F;
}

static gboolean core_loaded = FALSE;

gboolean whelk_bridge_use_core(void) {
  scm_internal_catch(SCM_BOOL_T, use_module_thunk, (void *)"whelk core",
                     on_use_core_error, NULL);

  core_loaded = public_proc("whelk core", "init-shell!") != SCM_BOOL_F;

  if (core_loaded)
    whelk_bridge_register_widget_primitives();

  return core_loaded;
}

char *whelk_bridge_config_path(void) {
  return g_build_filename(g_get_user_config_dir(), "whelk", "settings.scm",
                          NULL);
}

gboolean whelk_bridge_init(void) {
  SCM proc = public_proc("whelk core", "init-shell!");

  if (!scm_is_true(proc))
    return FALSE;

  scm_call_0(proc);
  return TRUE;
}

gboolean whelk_bridge_load(const char *path) {
  SCM proc = public_proc("whelk core", "load-shell-config!");

  if (!scm_is_true(proc) || !path)
    return FALSE;

  return scm_is_true(scm_call_1(proc, scm_from_utf8_string(path)));
}

void whelk_bridge_start_repl(const char *path) {
  SCM proc = public_proc("whelk core", "start-repl!");

  if (!scm_is_true(proc))
    return;

  if (path && *path)
    scm_call_1(proc, scm_from_utf8_string(path));
  else
    scm_call_0(proc);
}

void whelk_bridge_stop_repl(void) {
  if (!core_loaded)
    return;

  SCM proc = public_proc("whelk core", "stop-repl!");

  if (scm_is_true(proc))
    scm_call_0(proc);
}

GPtrArray *whelk_bridge_bars(const char *path) {
  GPtrArray *bars = g_ptr_array_new_with_free_func(
      (GDestroyNotify)whelk_bar_config_free);
  SCM list = call_with_path("whelk state", "shell-bars->alists", path);

  for (SCM cursor = list; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    WhelkBarConfig *bar = bar_from_alist(SCM_CAR(cursor));

    if (bar)
      g_ptr_array_add(bars, bar);
  }

  return bars;
}

GPtrArray *whelk_bridge_widgets(const char *path) {
  GPtrArray *widgets = g_ptr_array_new_with_free_func(
      (GDestroyNotify)whelk_widget_config_free);
  SCM list = call_with_path("whelk state", "shell-widgets->alists", path);

  for (SCM cursor = list; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    SCM entry = SCM_CAR(cursor);
    g_autofree char *kind = scm_text(alist_get(entry, "kind"));

    if (!kind || *kind == '\0')
      continue;

    WhelkWidgetConfig *widget = whelk_widget_config_new(kind);
    const int x = alist_int(entry, "x", 48);
    const int y = alist_int(entry, "y", 64);
    const int width = alist_int(entry, "width", 320);
    const int height = alist_int(entry, "height", 220);

    widget->x = MAX(0, x);
    widget->y = MAX(0, y);
    widget->width = MAX(100, width);
    widget->height = MAX(100, height);

    g_free(widget->monitor);
    widget->monitor = alist_text(entry, "monitor", "default");

    collect_options(widget->spec, alist_get(entry, "options"));
    g_ptr_array_add(widgets, widget);
  }

  return widgets;
}

WhelkClipboardConfig *whelk_bridge_clipboard(const char *path) {
  WhelkClipboardConfig *config = whelk_clipboard_config_new();
  SCM alist = call_with_path("whelk state", "shell-clipboard->alist", path);

  if (!scm_is_pair(alist))
    return config;

  config->configured = alist_bool(alist, "configured?", TRUE);
  config->persist_only_pinned =
      alist_bool(alist, "persist-only-pinned?", FALSE);
  config->clear_on_lock = alist_bool(alist, "clear-on-lock?", FALSE);

  g_autofree char *persist = alist_optional_text(alist, "persist");

  if (persist && *persist && g_strcmp0(persist, "#f") != 0) {
    config->persist = TRUE;

    if (g_strcmp0(persist, "auto") != 0 && g_strcmp0(persist, "#t") != 0)
      config->persist_path = g_strdup(persist);
  }

  const int max_entries = alist_int(alist, "max-entries", 0);
  const int max_entry = alist_int(alist, "max-entry-bytes", 0);
  const int max_total = alist_int(alist, "max-total-bytes", 0);
  const int ttl = alist_int(alist, "ttl-seconds", 0);

  config->max_entries = max_entries > 0 ? (guint)max_entries : 0;
  config->max_entry_bytes = max_entry > 0 ? (guint)max_entry : 0;
  config->max_total_bytes = max_total > 0 ? (guint)max_total : 0;
  config->ttl_seconds = ttl > 0 ? (guint)ttl : 0;

  return config;
}

/* =========================================================================
 * THE MENU DICTIONARY
 * =========================================================================
 * Scheme holds the applications (scheme/whelk/menudict.scm); this is the one
 * walk over the alists it exports, at startup.  Everything here becomes plain
 * C strings and structs owned by the table, so no Scheme value outlives the
 * call.
 */

static SCM on_use_module_error(void *data, SCM tag, SCM args) {
  (void)tag;
  (void)args;

  LOG_C_WARN("Guile", "could not load (%s): what it describes is missing",
             (const char *)data);

  return SCM_BOOL_F;
}

static gboolean load_module(const char *name) {
  SCM result = scm_internal_catch(SCM_BOOL_T, use_module_thunk, (void *)name,
                                  on_use_module_error, (void *)name);

  return scm_is_true(result);
}

static guint scheme_list_length(SCM list) {
  guint length = 0;

  for (SCM cursor = list; scm_is_pair(cursor); cursor = SCM_CDR(cursor))
    length++;

  return length;
}

static SCM scheme_list_ref(SCM list, guint index) {
  for (guint i = 0; i < index && scm_is_pair(list); i++)
    list = SCM_CDR(list);

  return scm_is_pair(list) ? SCM_CAR(list) : SCM_BOOL_F;
}

/* A list of strings as the NULL-terminated array the lookup walks.  A missing
 * word keeps its place as an empty string rather than cutting the list short. */
static char **scheme_text_list(SCM list) {
  guint length = scheme_list_length(list);
  char **words = g_new0(char *, length + 1);

  for (guint i = 0; i < length; i++) {
    char *text = scm_text(scheme_list_ref(list, i));

    words[i] = text ? text : g_strdup("");
  }

  return words;
}

/* (label accel modifiers key), with #f for a bare key, or the shorter
 * (label chord), where the chord may be an Emacs sequence (C-x C-s) or one
 * chord in either notation.  This is where it stops being text and becomes
 * the modifiers and keysyms the runtime types. */
static WhelkDictItem *scheme_items(SCM list, guint *n_items) {
  guint length = scheme_list_length(list);
  WhelkDictItem *items = whelk_dict_items_new(length);

  for (guint i = 0; i < length; i++) {
    SCM spec = scheme_list_ref(list, i);

    items[i].label = scm_text(scheme_list_ref(spec, 0));

    if (scheme_list_length(spec) == 2) {
      g_autofree char *written = scm_text(scheme_list_ref(spec, 1));
      WhelkChordSequence sequence;

      if (written && whelk_chord_sequence_parse(written, &sequence)) {
        /* What the dictionary stores is not what the menu shows: the table
         * keeps the sequence as CUA, and the bar writes it in the style its
         * configuration asked for. */
        items[i].accel = whelk_chord_sequence_format(&sequence,
                                                     WHELK_CHORD_CUA);
        items[i].n_chords = sequence.n_chords;
        items[i].modifiers = g_new0(char *, sequence.n_chords);
        items[i].keys = g_new0(char *, sequence.n_chords);

        for (guint j = 0; j < sequence.n_chords; j++) {
          items[i].modifiers[j] = whelk_chord_modifiers(&sequence.chords[j]);
          items[i].keys[j] = g_strdup(sequence.chords[j].key);
        }

        whelk_chord_sequence_clear(&sequence);
      } else {
        /* A chord this shell cannot read still prints: the entry is shown
         * disabled rather than silently dropped. */
        items[i].accel = g_strdup(written ? written : "");
      }

      continue;
    }

    items[i].accel = scm_text(scheme_list_ref(spec, 1));
    items[i].modifiers = g_new0(char *, 1);
    items[i].keys = g_new0(char *, 1);
    items[i].modifiers[0] = scm_text(scheme_list_ref(spec, 2));
    items[i].keys[0] = scm_text(scheme_list_ref(spec, 3));
    items[i].n_chords = 1;
  }

  *n_items = length;
  return items;
}

/* (label item ...), in the order the Scheme wrote them. */
static WhelkDictMenu *scheme_menus(SCM list, guint *n_menus) {
  guint length = scheme_list_length(list);
  WhelkDictMenu *menus = whelk_dict_menus_new(length);

  for (guint i = 0; i < length; i++) {
    SCM entry = scheme_list_ref(list, i);

    menus[i].label = scm_text(scheme_list_ref(entry, 0));
    menus[i].items = scheme_items(scm_is_pair(entry) ? SCM_CDR(entry) : SCM_EOL,
                                  &menus[i].n_items);
  }

  *n_menus = length;
  return menus;
}

GPtrArray *whelk_bridge_menudict(void) {
  GPtrArray *table = whelk_menudict_table_new();

  if (!load_module("whelk menudict"))
    return table;

  SCM list = call0("whelk menudict", "menudict->alists");

  for (SCM cursor = list; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    SCM entry = SCM_CAR(cursor);
    char *name = alist_text(entry, "name", "unnamed");
    char **ids = scheme_text_list(alist_get(entry, "ids"));
    guint n_menus = 0;
    WhelkDictMenu *menus = scheme_menus(alist_get(entry, "menus"), &n_menus);

    g_ptr_array_add(table, whelk_dict_app_new(name, ids, menus, n_menus));
  }

  return table;
}

/* =========================================================================
 * THE SCHEME SHORTCUT CORPUS
 * =========================================================================
 * scheme/whelk/orbit.scm is the Orbit corpus as data: 293 applications, each
 * with the identifiers it answers to and its menus.  The bridge walks it only
 * for the focused application, so the 10,000 entries cost one list traversal
 * and not an allocation of the whole corpus.
 *
 * Shortcuts stay in the source's GTK notation until here; the chord parser
 * turns one into the text shown in the row and the command typed into the
 * focused window, exactly as the JSON importer did.
 */

static gboolean orbit_tag_is(SCM value, const char *tag) {
  g_autofree char *text = scm_text(value);

  return text && g_strcmp0(text, tag) == 0;
}

static gboolean orbit_has_entries(const WhelkMenu *menu) {
  return menu && menu->entries && menu->entries->len > 0;
}

static void orbit_separator(WhelkMenu *menu) {
  if (!orbit_has_entries(menu))
    return;

  const WhelkMenuEntry *last =
      g_ptr_array_index(menu->entries, menu->entries->len - 1);

  if (last->kind != WHELK_MENU_SEPARATOR)
    whelk_menu_add_separator(menu);
}

static void orbit_trim_separators(WhelkMenu *menu) {
  while (orbit_has_entries(menu)) {
    WhelkMenuEntry *last =
        g_ptr_array_index(menu->entries, menu->entries->len - 1);

    if (last->kind != WHELK_MENU_SEPARATOR)
      break;

    g_ptr_array_remove_index(menu->entries, menu->entries->len - 1);
  }
}

static void fill_orbit_items(WhelkMenu *menu, SCM entries,
                             const char *shortcut_tool,
                             WhelkChordStyle style);

static gboolean fill_orbit_submenu(WhelkMenu *menu, SCM unit,
                                   const char *shortcut_tool,
                                   WhelkChordStyle style) {
  SCM rest = SCM_CDR(unit);
  g_autofree char *label = scm_text(scheme_list_ref(rest, 0));

  if (!label || *label == '\0')
    return TRUE;

  WhelkMenu *submenu = whelk_menu_new(label);

  fill_orbit_items(submenu, SCM_CDR(rest), shortcut_tool, style);

  if (!orbit_has_entries(submenu)) {
    whelk_menu_free(submenu);
    return TRUE;
  }

  WhelkMenuEntry *entry = whelk_menu_add_entry(menu, label);

  entry->submenu = submenu;

  return TRUE;
}

static void fill_orbit_item(WhelkMenu *menu, SCM unit,
                            const char *shortcut_tool,
                            WhelkChordStyle style) {
  if (orbit_tag_is(SCM_CAR(unit), "separator")) {
    orbit_separator(menu);
    return;
  }

  if (orbit_tag_is(SCM_CAR(unit), "submenu")) {
    fill_orbit_submenu(menu, unit, shortcut_tool, style);
    return;
  }

  if (!orbit_tag_is(SCM_CAR(unit), "item"))
    return;

  SCM rest = SCM_CDR(unit);
  g_autofree char *label = scm_text(scheme_list_ref(rest, 0));

  if (!label || *label == '\0')
    return;

  g_autofree char *shortcut = scm_text(scheme_list_ref(rest, 1));
  g_autofree char *action = scm_text(scheme_list_ref(rest, 2));
  WhelkMenuEntry *entry = whelk_menu_add_entry(menu, label);

  if (shortcut && *shortcut) {
    WhelkChordSequence sequence;

    if (whelk_chord_sequence_parse_gtk(shortcut, &sequence)) {
      entry->accel = whelk_chord_sequence_format(&sequence, style);
      entry->command = whelk_chord_sequence_command(shortcut_tool, &sequence);
      entry->enabled = entry->command != NULL;

      whelk_chord_sequence_clear(&sequence);
    } else {
      /* Keep the text its author wrote: a mapping problem to see, not an
       * invitation to invent a different chord. */
      entry->accel = g_strdup(shortcut);
      entry->enabled = FALSE;
    }

    return;
  }

  if (action && *action) {
    entry->action = g_strdup(action);
    return;
  }

  /* A placeholder: reachable through a menu of its own or a mouse gesture. */
  entry->enabled = FALSE;
}

static void fill_orbit_items(WhelkMenu *menu, SCM entries,
                             const char *shortcut_tool,
                             WhelkChordStyle style) {
  for (SCM cursor = entries; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    SCM unit = SCM_CAR(cursor);

    if (scm_is_pair(unit))
      fill_orbit_item(menu, unit, shortcut_tool, style);
  }

  orbit_trim_separators(menu);
}

static GPtrArray *orbit_menubar_from_app(SCM app, const char *shortcut_tool,
                                         WhelkChordStyle style) {
  GPtrArray *bar = whelk_menubar_new();
  SCM rest = SCM_CDR(app);

  /* (app name ids blank menu ...): the blank flag says the source deliberately
   * describes no menus, so an empty answer is not a reason to use fallbacks. */
  SCM blank = scheme_list_ref(rest, 2);

  if (scm_is_true(blank))
    return bar;

  for (SCM cursor = SCM_CDDR(SCM_CDR(rest)); scm_is_pair(cursor);
       cursor = SCM_CDR(cursor)) {
    SCM unit = SCM_CAR(cursor);

    if (!scm_is_pair(unit) || !orbit_tag_is(SCM_CAR(unit), "menu"))
      continue;

    SCM menu_rest = SCM_CDR(unit);
    g_autofree char *label = scm_text(scheme_list_ref(menu_rest, 0));

    if (!label || *label == '\0')
      continue;

    WhelkMenu *menu = whelk_menu_new(label);

    fill_orbit_items(menu, SCM_CDR(menu_rest), shortcut_tool, style);

    if (orbit_has_entries(menu))
      g_ptr_array_add(bar, menu);
    else
      whelk_menu_free(menu);
  }

  return bar;
}

GPtrArray *whelk_bridge_orbit_menubar(const char *app_id,
                                      const char *shortcut_tool,
                                      WhelkChordStyle style,
                                      char **out_name) {
  if (out_name)
    *out_name = NULL;

  GPtrArray *bar = whelk_menubar_new();

  if (!app_id || *app_id == '\0' || !shortcut_tool || *shortcut_tool == '\0')
    return bar;

  if (!load_module("whelk orbit"))
    return bar;

  /* orbit-apps is data, not a procedure: read the exported binding rather
   * than calling it. */
  SCM apps = scm_c_public_ref("whelk orbit", "orbit-apps");

  for (SCM cursor = apps; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    SCM app = SCM_CAR(cursor);

    if (!scm_is_pair(app) || !orbit_tag_is(SCM_CAR(app), "app"))
      continue;

    SCM rest = SCM_CDR(app);
    SCM ids = scheme_list_ref(rest, 1);
    gboolean matched = FALSE;

    for (SCM id = ids; scm_is_pair(id) && !matched; id = SCM_CDR(id)) {
      g_autofree char *text = scm_text(SCM_CAR(id));

      if (text && whelk_menudict_ids_match(app_id, text))
        matched = TRUE;
    }

    if (!matched)
      continue;

    g_ptr_array_unref(bar);
    bar = orbit_menubar_from_app(app, shortcut_tool, style);

    if (out_name) {
      g_autofree char *name = scm_text(scheme_list_ref(rest, 0));

      *out_name = g_strdup(name);
    }

    break;
  }

  return bar;
}

/* =========================================================================
 * THE SHELL'S OWN MENU
 * =========================================================================
 * The popover behind the mark at the far left of the bar is configuration, not
 * a C table: a settings file writes it with `(whelk menu)' units, and the
 * bridge turns those into the same menu tree the global menu uses.  An action
 * spelled `shell:name' is a thing the shell itself does (show the launcher,
 * open the notification centre, offer the windows to close); anything else is a
 * command line, run the way every other click on this bar runs one.
 *
 * A file that configures no menu gets `default-shell-menu', so the walk below
 * is the same in both cases -- and so the default lives where a person can read
 * it and a machine can override it.
 */

static void fill_shell_units(WhelkMenu *menu, SCM units) {
  for (SCM cursor = units; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    SCM unit = SCM_CAR(cursor);

    if (!scm_is_pair(unit))
      continue;

    g_autofree char *tag = scm_text(SCM_CAR(unit));
    SCM rest = SCM_CDR(unit);

    if (!tag)
      continue;

    if (g_strcmp0(tag, "separator") == 0) {
      whelk_menu_add_separator(menu);
      continue;
    }

    if (g_strcmp0(tag, "item") == 0) {
      g_autofree char *label = scm_text(scheme_list_ref(rest, 0));
      g_autofree char *action = scm_text(scheme_list_ref(rest, 1));

      if (!label || *label == '\0')
        continue;

      WhelkMenuEntry *entry = whelk_menu_add_entry(menu, label);

      if (!action || *action == '\0')
        entry->enabled = FALSE; /* a row that says what it cannot do */
      else if (g_str_has_prefix(action, "shell:"))
        entry->action = g_strdup(action);
      else
        entry->command = g_strdup(action);

      continue;
    }

    if (g_strcmp0(tag, "submenu") == 0 || g_strcmp0(tag, "menu") == 0) {
      g_autofree char *label = scm_text(scheme_list_ref(rest, 0));
      SCM body = scm_is_pair(rest) ? SCM_CDR(rest) : SCM_EOL;

      if (!label || *label == '\0')
        continue;

      WhelkMenu *submenu = whelk_menu_add_submenu(menu, label);
      WhelkMenuEntry *entry = menu->entries->len > 0
                                  ? g_ptr_array_index(menu->entries,
                                                      menu->entries->len - 1)
                                  : NULL;

      /* A submenu whose body is one string is a slot the shell fills itself
       * when it opens -- `(submenu "Recent Items" "shell:recent-items")'. */
      SCM marker_arg = scm_is_pair(body) ? SCM_CAR(body) : SCM_BOOL_F;

      if ((scm_is_string(marker_arg) || scm_is_symbol(marker_arg)) &&
          scheme_list_length(body) == 1) {
        g_autofree char *marker = scm_text(marker_arg);

        if (entry && marker && *marker != '\0')
          entry->action = g_strdup(marker);

        continue;
      }

      fill_shell_units(submenu, body);
      continue;
    }
  }
}

GPtrArray *whelk_bridge_shell_menu(const char *path) {
  GPtrArray *bar = whelk_menubar_new();
  SCM units = call_with_path("whelk state", "shell-menu->units", path);

  if (!scm_is_pair(units)) {
    if (!load_module("whelk defaults"))
      return bar;

    units = call0("whelk defaults", "default-shell-menu");
  }

  WhelkMenu *root = whelk_menu_new("shell");

  fill_shell_units(root, units);

  if (root->entries->len > 0)
    g_ptr_array_add(bar, root);
  else
    whelk_menu_free(root);

  return bar;
}

/* =========================================================================
 * GUI PREFERENCES
 * =========================================================================
 * The settings window stores the same values a settings file writes, but in
 * the state directory and one key at a time.  The Scheme layer owns the file
 * format and the validation; this is just a typed door into it.
 */

GPtrArray *whelk_bridge_themes(void) {
  GPtrArray *names = g_ptr_array_new_with_free_func(g_free);

  if (!load_module("whelk themes"))
    return names;

  SCM list = call0("whelk themes", "themes");

  for (SCM cursor = list; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    char *name = scm_text(SCM_CAR(cursor));

    if (name && *name)
      g_ptr_array_add(names, name);
    else
      g_free(name);
  }

  return names;
}

static gboolean save_preference(const char *path, const char *key, SCM value) {
  SCM proc = public_proc("whelk prefs", "save-preference!");

  if (!scm_is_true(proc) || !path || !key)
    return FALSE;

  return scm_is_true(scm_call_3(proc, scm_from_utf8_string(path),
                                scm_from_utf8_symbol(key), value));
}

gboolean whelk_bridge_save_preference_bool(const char *path, const char *key,
                                           gboolean value) {
  return save_preference(path, key, scm_from_bool(value));
}

gboolean whelk_bridge_save_preference_int(const char *path, const char *key,
                                          int value) {
  return save_preference(path, key, scm_from_int(value));
}

gboolean whelk_bridge_save_preference_string(const char *path, const char *key,
                                             const char *value) {
  return save_preference(path, key,
                         scm_from_utf8_string(value ? value : ""));
}

gboolean whelk_bridge_save_preference_symbol(const char *path, const char *key,
                                             const char *value) {
  return save_preference(path, key,
                         scm_from_utf8_symbol(value ? value : ""));
}

gboolean whelk_bridge_reset_preferences(const char *path) {
  SCM proc = public_proc("whelk prefs", "reset-preferences!");

  if (!scm_is_true(proc) || !path)
    return FALSE;

  return scm_is_true(scm_call_1(proc, scm_from_utf8_string(path)));
}
