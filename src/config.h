// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_CONFIG_H
#define WHELK_CONFIG_H

#include <gtk4-layer-shell.h>
#include <gtk/gtk.h>

/* =========================================================================
 * THE PLAIN CONFIGURATION
 * =========================================================================
 * Everything the C runtime knows about a configuration, as plain structs with
 * no SCM in sight.  src/guile_bridge.c is the only place that fills them from
 * Scheme, and it hands back the same shapes the C defaults build by hand, so
 * the shell runs on a configuration that never arrived.
 *
 * Ownership: a bar owns its module specs; free the whole array with
 * whelk_config_free_bars() and never keep a spec pointer across a reload.
 */

/**
 * WhelkOptionType:
 * @WHELK_OPTION_STRING: a Scheme string.
 * @WHELK_OPTION_SYMBOL: a Scheme symbol; the settings window writes it back as
 *   one, so a module that compares with `eq?' keeps seeing a symbol.
 * @WHELK_OPTION_BOOLEAN: #t or #f.
 * @WHELK_OPTION_INTEGER: an exact integer.
 * @WHELK_OPTION_NUMBER: another number, such as 0.5.
 * @WHELK_OPTION_LIST: a list; the generic editor leaves it in the file rather
 *   than flattening it into a text value.
 * @WHELK_OPTION_OTHER: a value the editor cannot round-trip.
 *
 * What the Scheme side originally wrote for an option.  The text in
 * WhelkModuleSpec:options is still the authority for readers; this only tells
 * the settings window which Scheme value to write when somebody edits it.
 */
typedef enum {
  WHELK_OPTION_STRING,
  WHELK_OPTION_SYMBOL,
  WHELK_OPTION_BOOLEAN,
  WHELK_OPTION_INTEGER,
  WHELK_OPTION_NUMBER,
  WHELK_OPTION_LIST,
  WHELK_OPTION_OTHER,
} WhelkOptionType;

/**
 * WhelkModuleSpec:
 * @kind: the module's factory name ("clock", "cpu", "custom", ...).
 * @options: (element-type utf8 utf8): every option the settings file gave, as
 *   text.  Numbers, booleans and symbols arrive in their Scheme spelling and
 *   the typed getters below parse them, so a module reads an option the same
 *   way whether a file wrote 5, "5" or 5.0.
 * @option_types: (element-type utf8 gpointer): the original Scheme type of
 *   each option, as a WhelkOptionType stored through GUINT_TO_POINTER().  A
 *   spec built by hand defaults every option to WHELK_OPTION_STRING.
 *
 * One module, as the configuration declared it.
 */
typedef struct {
  char *kind;
  GHashTable *options;
  GHashTable *option_types;
} WhelkModuleSpec;

/**
 * WhelkWidgetConfig:
 * @spec: the widget's kind and options, the same shape a bar module uses.
 * @x: pixels from the monitor's left edge.
 * @y: pixels from the monitor's top edge.
 * @width: the card's width.
 * @height: the card's height.
 * @monitor: "default" or a connector name such as "DP-1".
 *
 * One floating desktop widget, as the settings file described it.  Geometry is
 * separate from @spec because a widget's identity is its kind and options,
 * while where it sits is what a desktop layout decides.
 */
typedef struct {
  WhelkModuleSpec *spec;
  int x;
  int y;
  int width;
  int height;
  char *monitor;
} WhelkWidgetConfig;

/**
 * WhelkBarWhen:
 * @WHELK_BAR_WHEN_ALWAYS: the bar exists whenever its monitor does.
 * @WHELK_BAR_WHEN_SINGLE: only while the machine has one output.
 * @WHELK_BAR_WHEN_MULTI: only while the machine has more than one output.
 *
 * A deliberate monitor rule: "this bar on the laptop, that one on the
 * external, and nothing at all when there is only one screen" is a
 * configuration, not a special case in the shell.  The monitor model is
 * watched, so a hotplug re-evaluates the rule and rebuilds the bars.
 */
typedef enum {
  WHELK_BAR_WHEN_ALWAYS,
  WHELK_BAR_WHEN_SINGLE,
  WHELK_BAR_WHEN_MULTI,
} WhelkBarWhen;

/**
 * WhelkBarConfig:
 * @name: the bar's name; also its CSS id and its log label.
 * @theme: (nullable): the theme the bar was built from, for the settings UI.
 * @edge: the screen edge it is anchored to.
 * @layer: the layer-shell layer it is placed in.  "top" is the default and
 *   stays fixed over a compositor's overview; "bottom" and "background" move
 *   and zoom with the workspace on niri.
 * @monitor: "all", "default", or a connector name such as "DP-1".
 * @monitors: (element-type utf8): the connectors this bar belongs on; when it
 *   is non-empty it wins over @monitor.  A bar with no matching output is not
 *   drawn at all, rather than being moved to whichever output the compositor
 *   calls default.
 * @when: the output-count rule the bar waits for (always, single, multi).
 * @height: its thickness: the row's height on a horizontal edge, the column's
 *   width on a vertical one.
 * @margin_top: pixels between the bar and the screen edge (and so on).
 * @exclusive: whether the compositor reserves the bar's thickness.
 * @spacing: pixels between modules.
 * @padding: pixels between the bar's ends and its first/last module.
 * @font: (nullable): the text family, or NULL for the stylesheet's.
 * @font_size: (nullable): the text size, e.g. "11pt".
 * @icon_font: (nullable): the family module glyphs are drawn from.
 * @foreground: (nullable): the bar's ink.
 * @background: (nullable): the bar's material.
 * @accent: (nullable): the colour of an active workspace and other highlights.
 * @border_color: (nullable): the hairline around the bar.
 * @radius: the bar's corner radius in pixels.
 * @opacity: the multiplier applied to the background's own alpha (0..1).
 * @blur: the soft shadow's blur radius in pixels (0..64).
 * @shine: the strength of the top-edge highlight (0..1).
 * @reflection: the strength of the lower inner reflection (0..1).
 * @flare: the strength of the accent glow along the top edge (0..1).
 * @css: (nullable): free-form CSS appended to this bar's generated skin.
 * @launcher_width: the launcher card's width in pixels.
 * @launcher_height: the launcher card's height in pixels.
 * @launcher_icon_size: the pixel size of a launcher row's icon.
 * @launcher_font_scale: the launcher's text size as a percentage, where 100 is
 *   the bar's configured size.
 * @left: (element-type WhelkModuleSpec): modules at the start of the bar.
 * @center: (element-type WhelkModuleSpec): modules in the middle.
 * @right: (element-type WhelkModuleSpec): modules at the end.
 * @overflow_expanded: whether the right row's overflow area starts open.
 * @overflow: (element-type WhelkModuleSpec): modules behind the right row's
 *   chevron.  They are built with the bar, so they keep their timers and
 *   clicks whether the area is open or closed.
 *
 * One bar, resolved.  This is a description, not a widget: src/bar.c turns it
 * into a surface, and a reload compares descriptions by simply replacing them.
 */
typedef struct {
  char *name;
  char *theme;
  GtkLayerShellEdge edge;
  GtkLayerShellLayer layer;
  char *monitor;
  GPtrArray *monitors;
  WhelkBarWhen when;
  int height;
  int margin_top;
  int margin_right;
  int margin_bottom;
  int margin_left;
  gboolean exclusive;
  int spacing;
  int padding;
  char *font;
  char *font_size;
  char *icon_font;
  char *foreground;
  char *background;
  char *accent;
  char *border_color;
  int radius;
  double opacity;
  int blur;
  double shine;
  double reflection;
  double flare;
  char *css;
  int launcher_width;
  int launcher_height;
  int launcher_icon_size;
  int launcher_font_scale;

  /* Set by src/bar.c from the modules it is about to build: when the bar has
   * its own Wi-Fi or Bluetooth reading, a third-party applet for the same thing
   * is redundant, and the tray hides it (src/modules/tray.c). */
  gboolean native_network;
  gboolean native_bluetooth;

  /* The right row's overflow: modules built with the bar but laid out only
   * while the area is open.  The settings decide the starting state; the
   * chevron toggles it afterwards. */
  gboolean overflow_expanded;
  GPtrArray *overflow;

  GPtrArray *left;
  GPtrArray *center;
  GPtrArray *right;
} WhelkBarConfig;

/**
 * WhelkClipboardConfig:
 * @configured: whether the settings file described the clipboard at all.
 * @persist: whether the history should be kept across restarts.
 * @persist_path: (nullable): the file to use; NULL means the state directory.
 * @persist_only_pinned: keep only pinned entries on disk.
 * @clear_on_lock: clear unpinned entries when logind reports the session locked.
 * @max_entries: history length; 0 means the built-in default.
 * @max_entry_bytes: per-entry memory budget; 0 means the built-in default.
 * @max_total_bytes: whole-history budget; 0 means the built-in default.
 * @ttl_seconds: expiry for unpinned entries; 0 keeps them forever.
 *
 * The clipboard's policy, resolved from `(clipboard-settings)'.  An empty
 * configuration leaves @configured FALSE so the environment variable still
 * works for existing setups.
 */
typedef struct {
  gboolean configured;
  gboolean persist;
  char *persist_path;
  gboolean persist_only_pinned;
  gboolean clear_on_lock;
  guint max_entries;
  guint max_entry_bytes;
  guint max_total_bytes;
  guint ttl_seconds;
} WhelkClipboardConfig;

/* --- specs ---------------------------------------------------------------- */

WhelkModuleSpec *whelk_module_spec_new(const char *kind);
void whelk_module_spec_free(WhelkModuleSpec *spec);
void whelk_module_spec_set(WhelkModuleSpec *spec, const char *key,
                           const char *value);
void whelk_module_spec_set_typed(WhelkModuleSpec *spec, const char *key,
                                 const char *value, WhelkOptionType type);
WhelkOptionType whelk_spec_type(const WhelkModuleSpec *spec, const char *key);

/* Typed option readers.  A missing, empty or unparseable option answers with
 * the fallback, so a settings file can never push garbage into a widget. */
const char *whelk_spec_str(const WhelkModuleSpec *spec, const char *key,
                           const char *fallback);
int whelk_spec_int(const WhelkModuleSpec *spec, const char *key, int fallback);
double whelk_spec_double(const WhelkModuleSpec *spec, const char *key,
                         double fallback);
gboolean whelk_spec_bool(const WhelkModuleSpec *spec, const char *key,
                         gboolean fallback);
gboolean whelk_spec_has(const WhelkModuleSpec *spec, const char *key);

/* --- desktop widgets ------------------------------------------------------ */

WhelkWidgetConfig *whelk_widget_config_new(const char *kind);
WhelkWidgetConfig *whelk_widget_config_copy(const WhelkWidgetConfig *config);
void whelk_widget_config_free(WhelkWidgetConfig *config);
void whelk_config_free_widgets(GPtrArray *widgets);

G_DEFINE_AUTOPTR_CLEANUP_FUNC(WhelkWidgetConfig, whelk_widget_config_free)

/* --- bars ----------------------------------------------------------------- */

WhelkBarConfig *whelk_bar_config_new(const char *name);

/*
 * whelk_bar_config_copy:
 *
 * Returns: (transfer full) (nullable): a deep copy of @bar, module specs
 * included.  A bar owns its copy, so the configuration array it was built from
 * can be freed while the bar is still on screen.
 */
WhelkBarConfig *whelk_bar_config_copy(const WhelkBarConfig *bar);

void whelk_bar_config_free(WhelkBarConfig *bar);
void whelk_config_free_bars(GPtrArray *bars);

/**
 * whelk_config_default_bars:
 *
 * Returns: (transfer full) (element-type WhelkBarConfig): the bar the shell
 * falls back to when no configuration arrived: a top bar with a clock, its
 * memory, the tray, the battery when it has one, and the machine's load on the
 * right row's overflow shelf.
 */
GPtrArray *whelk_config_default_bars(void);

WhelkClipboardConfig *whelk_clipboard_config_new(void);
void whelk_clipboard_config_free(WhelkClipboardConfig *config);

G_DEFINE_AUTOPTR_CLEANUP_FUNC(WhelkClipboardConfig, whelk_clipboard_config_free)

/* Human-readable edge name, for logs. */
const char *whelk_edge_name(GtkLayerShellEdge edge);

/* "always", "single" or "multi"; unknown text falls back to always. */
WhelkBarWhen whelk_bar_when_from_text(const char *text);
const char *whelk_bar_when_name(WhelkBarWhen when);

#endif // WHELK_CONFIG_H
