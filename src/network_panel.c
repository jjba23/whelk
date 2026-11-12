// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "network_panel.h"

#include <stdlib.h>
#include <string.h>

#include "logger.h"
#include "style.h"
#include "surface.h"
#include "util/command.h"

/* =========================================================================
 * THE NETWORK PANEL
 * =========================================================================
 * Two pages in one right-edge surface.  Each page is a live view: it re-reads
 * its state when it is shown, after an action, and a second later, because a
 * network does not finish associating when the command returns.
 *
 * nmcli's terse mode escapes ':' inside a field, which matters: an SSID is
 * allowed to contain one, and splitting on ':' without honouring the escape
 * would cut "Cafe:Guest" in half.
 */

#define WHELK_NETWORK_WIDTH 400

static const char wifi_command[] =
    "printf 'radio:%s\\n' \"$(nmcli radio wifi 2>/dev/null)\"; "
    "printf 'active:%s\\n' \"$(nmcli -t -f ACTIVE,SSID dev wifi 2>/dev/null | "
    "sed -n 's/^yes://p' | head -n1)\"; "
    "nmcli -t -f SSID,SIGNAL,SECURITY dev wifi list 2>/dev/null";

static const char bluetooth_command[] =
    "printf 'powered:%s\\n' \"$(bluetoothctl show 2>/dev/null | "
    "sed -n 's/^[[:space:]]*Powered: //p')\"; "
    "echo --paired--; bluetoothctl devices Paired 2>/dev/null; "
    "echo --connected--; bluetoothctl devices Connected 2>/dev/null";

typedef struct {
  char *ssid;
  int signal;
  gboolean secured;
} AccessPoint;

static void access_point_free(gpointer data) {
  AccessPoint *point = data;

  if (!point)
    return;

  g_free(point->ssid);
  g_free(point);
}

typedef struct {
  char *name;
  char *mac;
  gboolean connected;
} BtDevice;

struct _WhelkNetworkPanel {
  GObject parent_instance;

  GtkApplication *app;
  WhelkBarConfig *style;
  GtkCssProvider *skin;
  WhelkSurface *surface;

  GtkWidget *stack;
  GtkWidget *wifi_list;
  GtkWidget *wifi_status;
  GtkWidget *wifi_switch;
  GtkWidget *password_row;
  GtkWidget *password_label;
  GtkWidget *password_entry;
  char *pending_ssid;

  GtkWidget *bt_list;
  GtkWidget *bt_status;
  GtkWidget *bt_switch;

  guint refresh_timeout;

  /* TRUE while a switch is being set from a reading: a switch that is
   * redrawn from nmcli must not turn round and ask nmcli to change again. */
  gboolean updating;
};

G_DEFINE_TYPE(WhelkNetworkPanel, whelk_network_panel, G_TYPE_OBJECT)

/* =========================================================================
 * HELPERS
 * ========================================================================= */

static void clear_box(GtkWidget *box) {
  GtkWidget *child = NULL;

  while ((child = gtk_widget_get_first_child(box)) != NULL)
    gtk_box_remove(GTK_BOX(box), child);
}

/* Splits one line of `nmcli -t' output, honouring its backslash escapes. */
static GPtrArray *split_terse(const char *line) {
  GPtrArray *fields = g_ptr_array_new_with_free_func(g_free);
  GString *current = g_string_new(NULL);

  for (const char *p = line; *p; p++) {
    if (*p == '\\' && p[1] != '\0') {
      g_string_append_c(current, p[1]);
      p++;
      continue;
    }

    if (*p == ':') {
      g_ptr_array_add(fields, g_strdup(current->str));
      g_string_truncate(current, 0);
      continue;
    }

    g_string_append_c(current, *p);
  }

  g_ptr_array_add(fields, g_strdup(current->str));
  g_string_free(current, TRUE);

  return fields;
}

static void schedule_refresh(WhelkNetworkPanel *self, guint delay_ms);

/* =========================================================================
 * WI-FI
 * ========================================================================= */

static void wifi_action_done(const char *text, gpointer user_data) {
  WhelkNetworkPanel *self = user_data;

  if (text && *text && self->wifi_status) {
    g_autofree char *first = g_strdup(text);
    char *newline = strchr(first, '\n');

    if (newline)
      *newline = '\0';

    gtk_label_set_text(GTK_LABEL(self->wifi_status), first);
  }

  schedule_refresh(self, 1200);
}

static void wifi_run(WhelkNetworkPanel *self, const char *command) {
  g_autofree char *with_stderr = g_strdup_printf("%s 2>&1", command);

  whelk_command_run(with_stderr, wifi_action_done, self);
}

/* The key goes down nmcli's stdin, never on its command line: an argument is
 * readable by every user on the machine through `ps`, a pipe is not. */
static void wifi_run_input(WhelkNetworkPanel *self, const char *command,
                           const char *input) {
  g_autofree char *with_stderr = g_strdup_printf("%s 2>&1", command);
  g_autofree char *line = g_strdup_printf("%s\n", input);

  whelk_command_run_input(with_stderr, line, wifi_action_done, self);
}

typedef struct {
  WhelkNetworkPanel *self;
  char *ssid;
} WifiTarget;

static void wifi_target_free(gpointer data, GClosure *closure) {
  WifiTarget *target = data;

  (void)closure;

  if (!target)
    return;

  g_free(target->ssid);
  g_free(target);
}

static void wifi_connect_with_password(WhelkNetworkPanel *self,
                                       const char *ssid,
                                       const char *password) {
  g_autofree char *quoted_ssid = g_shell_quote(ssid);

  if (password && *password) {
    /* --ask makes nmcli read the missing key from its standard input, which is
     * where wifi_run_input() puts it. */
    g_autofree char *command = g_strdup_printf(
        "nmcli --ask --wait 20 device wifi connect %s", quoted_ssid);

    wifi_run_input(self, command, password);
  } else {
    g_autofree char *command = g_strdup_printf(
        "nmcli --wait 20 device wifi connect %s", quoted_ssid);

    wifi_run(self, command);
  }
}

static void on_password_activate(GtkEntry *entry, gpointer user_data) {
  (void)entry;

  WhelkNetworkPanel *self = user_data;

  if (!self->pending_ssid)
    return;

  wifi_connect_with_password(self, self->pending_ssid,
                             gtk_editable_get_text(GTK_EDITABLE(
                                 self->password_entry)));

  gtk_widget_set_visible(self->password_row, FALSE);
  gtk_editable_set_text(GTK_EDITABLE(self->password_entry), "");
  g_free(self->pending_ssid);
  self->pending_ssid = NULL;
}

static void on_password_connect(GtkButton *button, gpointer user_data) {
  (void)button;

  on_password_activate(NULL, user_data);
}

static void on_wifi_row(GtkButton *button, gpointer user_data) {
  (void)button;

  WifiTarget *target = user_data;
  WhelkNetworkPanel *self = target->self;
  AccessPoint *point = g_object_get_data(G_OBJECT(button), "whelk-ap");

  g_free(self->pending_ssid);
  self->pending_ssid = g_strdup(target->ssid);

  if (!point || !point->secured) {
    wifi_connect_with_password(self, target->ssid, NULL);
    return;
  }

  /* A secured network: ask for the key in the panel, where the keyboard is.
   * An empty key means "use the connection NetworkManager already has". */
  g_autofree char *label =
      g_strdup_printf("Password for %s (blank: use saved)", target->ssid);

  gtk_label_set_text(GTK_LABEL(self->password_label), label);
  gtk_widget_set_visible(self->password_row, TRUE);
  gtk_widget_grab_focus(self->password_entry);
}

static void add_wifi_row(WhelkNetworkPanel *self, AccessPoint *point) {
  GtkWidget *row = gtk_button_new();
  GtkWidget *box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 10);
  GtkWidget *name = gtk_label_new(point->ssid);
  GtkWidget *signal = gtk_label_new(NULL);

  g_autofree char *signal_text = g_strdup_printf("%d%%", point->signal);

  gtk_label_set_text(GTK_LABEL(signal), signal_text);
  gtk_label_set_xalign(GTK_LABEL(name), 0.0);
  gtk_label_set_ellipsize(GTK_LABEL(name), PANGO_ELLIPSIZE_END);
  gtk_widget_set_hexpand(name, TRUE);
  gtk_widget_add_css_class(box, "whelk-network-row");
  gtk_widget_add_css_class(name, "whelk-network-name");
  gtk_widget_add_css_class(signal, "whelk-network-detail");

  gtk_box_append(GTK_BOX(box), name);

  if (point->secured) {
    GtkWidget *lock = gtk_image_new_from_icon_name("changes-prevent-symbolic");

    gtk_image_set_pixel_size(GTK_IMAGE(lock), 14);
    gtk_box_append(GTK_BOX(box), lock);
  }

  gtk_box_append(GTK_BOX(box), signal);
  gtk_button_set_child(GTK_BUTTON(row), box);
  gtk_widget_add_css_class(row, "whelk-network-item");

  WifiTarget *target = g_new0(WifiTarget, 1);

  target->self = self;
  target->ssid = g_strdup(point->ssid);

  g_object_set_data_full(G_OBJECT(row), "whelk-ap", point,
                         access_point_free);
  g_signal_connect_data(row, "clicked", G_CALLBACK(on_wifi_row), target,
                        wifi_target_free, 0);

  gtk_box_append(GTK_BOX(self->wifi_list), row);
}

static gint compare_ap(gconstpointer a, gconstpointer b) {
  const AccessPoint *left = *(AccessPoint *const *)a;
  const AccessPoint *right = *(AccessPoint *const *)b;

  return right->signal - left->signal;
}

static void on_wifi_state(const char *text, gpointer user_data) {
  WhelkNetworkPanel *self = user_data;

  if (!self->wifi_list)
    return;

  g_auto(GStrv) lines = g_strsplit(text ? text : "", "\n", -1);
  gboolean radio = FALSE;
  char *active = NULL;
  GHashTable *seen = g_hash_table_new_full(g_str_hash, g_str_equal, g_free,
                                           g_free);
  GPtrArray *points = g_ptr_array_new();
  int strongest = 0;

  for (guint i = 0; lines[i]; i++) {
    if (g_str_has_prefix(lines[i], "radio:")) {
      radio = g_strcmp0(lines[i] + 6, "enabled") == 0;
      continue;
    }

    if (g_str_has_prefix(lines[i], "active:")) {
      g_free(active);
      active = g_strdup(lines[i] + 7);
      continue;
    }

    if (*lines[i] == '\0')
      continue;

    g_autoptr(GPtrArray) fields = split_terse(lines[i]);

    if (fields->len < 3)
      continue;

    const char *ssid = g_ptr_array_index(fields, 0);
    const char *signal = g_ptr_array_index(fields, 1);
    const char *security = g_ptr_array_index(fields, 2);

    if (*ssid == '\0')
      continue;

    if (g_hash_table_contains(seen, ssid))
      continue;

    AccessPoint *point = g_new0(AccessPoint, 1);

    point->ssid = g_strdup(ssid);
    point->signal = atoi(signal);
    point->secured = *security != '\0' && g_strcmp0(security, "--") != 0;

    g_hash_table_insert(seen, g_strdup(ssid), point);
    g_ptr_array_add(points, point);
  }

  g_ptr_array_sort(points, compare_ap);
  clear_box(self->wifi_list);

  for (guint i = 0; i < points->len; i++) {
    AccessPoint *point = g_ptr_array_index(points, i);

    add_wifi_row(self, point);

    if (point->signal > strongest)
      strongest = point->signal;
  }

  if (points->len == 0) {
    GtkWidget *empty = gtk_label_new(radio ? "No networks in range"
                                           : "Wi-Fi is off");
    gtk_widget_add_css_class(empty, "whelk-panel-empty");
    gtk_box_append(GTK_BOX(self->wifi_list), empty);
  }

  if (self->wifi_status) {
    g_autofree char *status = NULL;

    if (!radio)
      status = g_strdup("Wi-Fi is off");
    else if (active && *active)
      status = g_strdup_printf("Connected to %s", active);
    else
      status = g_strdup("Not connected");

    gtk_label_set_text(GTK_LABEL(self->wifi_status), status);
  }

  if (self->wifi_switch) {
    self->updating = TRUE;
    gtk_switch_set_active(GTK_SWITCH(self->wifi_switch), radio);
    self->updating = FALSE;
  }

  g_hash_table_destroy(seen);
  g_clear_pointer(&points, g_ptr_array_unref);
  g_free(active);
}

static void on_wifi_switch(GObject *object, GParamSpec *pspec,
                           gpointer user_data) {
  (void)pspec;

  WhelkNetworkPanel *self = user_data;

  if (self->updating)
    return;

  const gboolean on = gtk_switch_get_active(GTK_SWITCH(object));
  g_autofree char *command =
      g_strdup_printf("nmcli radio wifi %s", on ? "on" : "off");

  wifi_run(self, command);
}

static void on_rescan(GtkButton *button, gpointer user_data) {
  (void)button;

  WhelkNetworkPanel *self = user_data;

  whelk_command_run("nmcli device wifi rescan 2>&1", wifi_action_done, self);
}

/* =========================================================================
 * BLUETOOTH
 * ========================================================================= */

typedef struct {
  WhelkNetworkPanel *self;
  char *mac;
  gboolean connected;
} BtTarget;

static void bt_target_free(gpointer data, GClosure *closure) {
  BtTarget *target = data;

  (void)closure;

  if (!target)
    return;

  g_free(target->mac);
  g_free(target);
}

static void bt_action_done(const char *text, gpointer user_data) {
  WhelkNetworkPanel *self = user_data;

  if (text && *text && self->bt_status) {
    g_autofree char *first = g_strdup(text);
    char *newline = strchr(first, '\n');

    if (newline)
      *newline = '\0';

    gtk_label_set_text(GTK_LABEL(self->bt_status), first);
  }

  schedule_refresh(self, 1500);
}

static void on_bt_row(GtkButton *button, gpointer user_data) {
  (void)button;

  BtTarget *target = user_data;
  g_autofree char *command =
      g_strdup_printf("bluetoothctl %s %s 2>&1",
                      target->connected ? "disconnect" : "connect",
                      target->mac);

  whelk_command_run(command, bt_action_done, target->self);
}

static void add_bt_row(WhelkNetworkPanel *self, BtDevice *device) {
  GtkWidget *row = gtk_button_new();
  GtkWidget *box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 10);
  GtkWidget *name = gtk_label_new(device->name);
  GtkWidget *state =
      gtk_label_new(device->connected ? "Connected" : device->mac);

  gtk_label_set_xalign(GTK_LABEL(name), 0.0);
  gtk_label_set_ellipsize(GTK_LABEL(name), PANGO_ELLIPSIZE_END);
  gtk_widget_set_hexpand(name, TRUE);
  gtk_widget_add_css_class(box, "whelk-network-row");
  gtk_widget_add_css_class(state, "whelk-network-detail");
  gtk_box_append(GTK_BOX(box), name);
  gtk_box_append(GTK_BOX(box), state);
  gtk_button_set_child(GTK_BUTTON(row), box);
  gtk_widget_add_css_class(row, "whelk-network-item");

  if (device->connected)
    gtk_widget_add_css_class(row, "connected");

  BtTarget *target = g_new0(BtTarget, 1);

  target->self = self;
  target->mac = g_strdup(device->mac);
  target->connected = device->connected;

  g_signal_connect_data(row, "clicked", G_CALLBACK(on_bt_row), target,
                        bt_target_free, 0);
  gtk_box_append(GTK_BOX(self->bt_list), row);
}

static void on_bt_state(const char *text, gpointer user_data) {
  WhelkNetworkPanel *self = user_data;

  if (!self->bt_list)
    return;

  g_auto(GStrv) lines = g_strsplit(text ? text : "", "\n", -1);
  gboolean powered = FALSE;
  gboolean connected_section = FALSE;
  GHashTable *connected = g_hash_table_new_full(g_str_hash, g_str_equal, g_free,
                                                NULL);
  GPtrArray *devices = g_ptr_array_new();

  for (guint i = 0; lines[i]; i++) {
    const char *line = lines[i];

    if (g_str_has_prefix(line, "powered:")) {
      powered = g_strcmp0(line + 8, "yes") == 0;
      continue;
    }

    if (g_strcmp0(line, "--paired--") == 0) {
      connected_section = FALSE;
      continue;
    }

    if (g_strcmp0(line, "--connected--") == 0) {
      connected_section = TRUE;
      continue;
    }

    if (!g_str_has_prefix(line, "Device "))
      continue;

    /* "Device AA:BB:CC:DD:EE:FF Some name" */
    const char *mac = line + 7;
    const char *space = strchr(mac, ' ');

    if (!space)
      continue;

    g_autofree char *address = g_strndup(mac, (gsize)(space - mac));
    const char *name = space + 1;

    if (connected_section) {
      g_hash_table_add(connected, g_strdup(address));
      continue;
    }

    BtDevice *device = g_new0(BtDevice, 1);

    device->mac = g_strdup(address);
    device->name = g_strdup(name);
    g_ptr_array_add(devices, device);
  }

  clear_box(self->bt_list);

  guint shown = 0;

  for (guint i = 0; i < devices->len; i++) {
    BtDevice *device = g_ptr_array_index(devices, i);

    device->connected = g_hash_table_contains(connected, device->mac);
    add_bt_row(self, device);
    shown++;

    g_free(device->mac);
    g_free(device->name);
    g_free(device);
  }

  g_clear_pointer(&devices, g_ptr_array_unref);
  g_hash_table_destroy(connected);

  if (shown == 0) {
    GtkWidget *empty =
        gtk_label_new(powered ? "No paired devices" : "Bluetooth is off");
    gtk_widget_add_css_class(empty, "whelk-panel-empty");
    gtk_box_append(GTK_BOX(self->bt_list), empty);
  }

  if (self->bt_status) {
    g_autofree char *status = NULL;

    if (!powered)
      status = g_strdup("Bluetooth is off");
    else if (shown == 0)
      status = g_strdup("Bluetooth is on");
    else
      status = g_strdup_printf("%u paired device%s", shown,
                               shown == 1 ? "" : "s");

    gtk_label_set_text(GTK_LABEL(self->bt_status), status);
  }

  if (self->bt_switch) {
    self->updating = TRUE;
    gtk_switch_set_active(GTK_SWITCH(self->bt_switch), powered);
    self->updating = FALSE;
  }
}

static void on_bt_switch(GObject *object, GParamSpec *pspec,
                         gpointer user_data) {
  (void)pspec;

  WhelkNetworkPanel *self = user_data;

  if (self->updating)
    return;

  const gboolean on = gtk_switch_get_active(GTK_SWITCH(object));
  g_autofree char *command =
      g_strdup_printf("bluetoothctl power %s 2>&1", on ? "on" : "off");

  whelk_command_run(command, bt_action_done, self);
}

/* =========================================================================
 * REFRESH
 * ========================================================================= */

static gboolean on_refresh_timeout(gpointer user_data) {
  WhelkNetworkPanel *self = user_data;

  self->refresh_timeout = 0;

  const char *page = gtk_stack_get_visible_child_name(GTK_STACK(self->stack));

  if (g_strcmp0(page, "bluetooth") == 0)
    whelk_command_run(bluetooth_command, on_bt_state, self);
  else
    whelk_command_run(wifi_command, on_wifi_state, self);

  return G_SOURCE_REMOVE;
}

static void schedule_refresh(WhelkNetworkPanel *self, guint delay_ms) {
  if (self->refresh_timeout != 0)
    g_source_remove(self->refresh_timeout);

  self->refresh_timeout = g_timeout_add(delay_ms, on_refresh_timeout, self);
}

static void refresh(WhelkNetworkPanel *self) {
  if (!self->stack)
    return;

  const char *page = gtk_stack_get_visible_child_name(GTK_STACK(self->stack));

  gtk_widget_set_visible(self->password_row, FALSE);

  if (g_strcmp0(page, "bluetooth") == 0)
    whelk_command_run(bluetooth_command, on_bt_state, self);
  else
    whelk_command_run(wifi_command, on_wifi_state, self);
}

/* =========================================================================
 * BUILDING
 * ========================================================================= */

static GtkWidget *build_page(WhelkNetworkPanel *self, const char *title,
                             GtkWidget **toggle, GtkWidget **status,
                             GtkWidget **list, const char *page) {
  GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 8);
  GtkWidget *header = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
  GtkWidget *heading = gtk_label_new(title);
  GtkWidget *scroller = gtk_scrolled_window_new();

  gtk_label_set_xalign(GTK_LABEL(heading), 0.0);
  gtk_widget_set_hexpand(heading, TRUE);
  gtk_widget_add_css_class(heading, "whelk-panel-title");

  *toggle = gtk_switch_new();
  gtk_widget_set_valign(*toggle, GTK_ALIGN_CENTER);

  gtk_box_append(GTK_BOX(header), heading);
  gtk_box_append(GTK_BOX(header), *toggle);

  *status = gtk_label_new("");
  gtk_label_set_xalign(GTK_LABEL(*status), 0.0);
  gtk_label_set_wrap(GTK_LABEL(*status), TRUE);
  gtk_widget_add_css_class(*status, "whelk-network-status");

  *list = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);

  gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scroller), *list);
  gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scroller),
                                 GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
  gtk_widget_set_vexpand(scroller, TRUE);

  gtk_box_append(GTK_BOX(box), header);
  gtk_box_append(GTK_BOX(box), *status);

  if (g_strcmp0(page, "wifi") == 0) {
    GtkWidget *actions = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
    GtkWidget *rescan = gtk_button_new_with_label("Rescan");

    gtk_widget_add_css_class(rescan, "whelk-network-button");
    g_signal_connect(rescan, "clicked", G_CALLBACK(on_rescan), self);
    gtk_box_append(GTK_BOX(actions), rescan);

    gtk_box_append(GTK_BOX(box), actions);

    self->password_row = gtk_box_new(GTK_ORIENTATION_VERTICAL, 4);
    self->password_label = gtk_label_new("");
    self->password_entry = gtk_entry_new();

    gtk_label_set_xalign(GTK_LABEL(self->password_label), 0.0);
    gtk_label_set_wrap(GTK_LABEL(self->password_label), TRUE);
    gtk_widget_add_css_class(self->password_label, "whelk-network-hint");
    gtk_entry_set_visibility(GTK_ENTRY(self->password_entry), FALSE);
    gtk_entry_set_placeholder_text(GTK_ENTRY(self->password_entry),
                                   "Network password");

    GtkWidget *connect = gtk_button_new_with_label("Connect");

    gtk_widget_add_css_class(connect, "accent");

    gtk_box_append(GTK_BOX(self->password_row), self->password_label);
    gtk_box_append(GTK_BOX(self->password_row), self->password_entry);
    gtk_box_append(GTK_BOX(self->password_row), connect);
    gtk_widget_set_visible(self->password_row, FALSE);
    gtk_box_append(GTK_BOX(box), self->password_row);

    g_signal_connect(self->password_entry, "activate",
                     G_CALLBACK(on_password_activate), self);
    g_signal_connect(connect, "clicked", G_CALLBACK(on_password_connect),
                     self);
    g_signal_connect(*toggle, "notify::active",
                     G_CALLBACK(on_wifi_switch), self);
  } else {
    g_signal_connect(*toggle, "notify::active", G_CALLBACK(on_bt_switch),
                     self);
  }

  gtk_box_append(GTK_BOX(box), scroller);

  gtk_stack_add_titled(GTK_STACK(self->stack), box, page,
                       g_strcmp0(page, "wifi") == 0 ? "Wi-Fi" : "Bluetooth");

  return box;
}

/* =========================================================================
 * PUBLIC API
 * ========================================================================= */

WhelkNetworkPanel *whelk_network_panel_new(GtkApplication *app,
                                           const WhelkBarConfig *style) {
  WhelkNetworkPanel *self = g_object_new(WHELK_TYPE_NETWORK_PANEL, NULL);

  self->app = app;
  self->style = style ? whelk_bar_config_copy(style)
                      : whelk_bar_config_new("network");

  GtkWidget *card = gtk_box_new(GTK_ORIENTATION_VERTICAL, 10);
  GtkWidget *header = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
  GtkWidget *title = gtk_label_new("Network");
  GtkWidget *close = gtk_button_new_from_icon_name("window-close-symbolic");
  GtkWidget *switcher = NULL;

  gtk_widget_add_css_class(card, "whelk-panel");
  gtk_widget_set_size_request(card, WHELK_NETWORK_WIDTH, -1);
  gtk_widget_set_margin_start(card, 12);
  gtk_widget_set_margin_end(card, 12);
  gtk_widget_set_margin_top(card, 12);
  gtk_widget_set_margin_bottom(card, 12);
  gtk_label_set_xalign(GTK_LABEL(title), 0.0);
  gtk_widget_set_hexpand(title, TRUE);
  gtk_widget_add_css_class(title, "whelk-panel-title");
  gtk_widget_add_css_class(close, "whelk-panel-button");

  self->stack = gtk_stack_new();
  gtk_stack_set_transition_type(GTK_STACK(self->stack),
                                GTK_STACK_TRANSITION_TYPE_CROSSFADE);

  build_page(self, "Wi-Fi", &self->wifi_switch, &self->wifi_status,
             &self->wifi_list, "wifi");
  build_page(self, "Bluetooth", &self->bt_switch, &self->bt_status,
             &self->bt_list, "bluetooth");

  switcher = gtk_stack_switcher_new();
  gtk_stack_switcher_set_stack(GTK_STACK_SWITCHER(switcher),
                               GTK_STACK(self->stack));
  gtk_widget_set_halign(switcher, GTK_ALIGN_START);

  gtk_box_append(GTK_BOX(header), title);
  gtk_box_append(GTK_BOX(header), close);
  gtk_box_append(GTK_BOX(card), header);
  gtk_box_append(GTK_BOX(card), switcher);
  gtk_box_append(GTK_BOX(card), self->stack);

  g_signal_connect_swapped(close, "clicked",
                           G_CALLBACK(whelk_network_panel_hide), self);

  self->surface = whelk_surface_new(app, "network");

  whelk_surface_set_edge(self->surface, GTK_LAYER_SHELL_EDGE_RIGHT);
  whelk_surface_set_layer(self->surface, GTK_LAYER_SHELL_LAYER_OVERLAY);
  whelk_surface_set_exclusive(self->surface, FALSE);
  whelk_surface_set_margins(self->surface, 8, 8, 8, 8);
  whelk_surface_set_namespace(self->surface, "whelk-network");

  g_autofree char *widget_id = whelk_style_widget_id("network");

  whelk_surface_set_widget_id(self->surface, widget_id);
  self->skin = whelk_style_apply(widget_id, self->style, "whelk-panel", NULL);
  whelk_surface_set_child(self->surface, card);
  whelk_surface_present(self->surface);

  GtkWindow *window = whelk_surface_get_window(self->surface);

  if (window)
    gtk_widget_set_visible(GTK_WIDGET(window), FALSE);

  g_signal_connect_swapped(self->stack, "notify::visible-child-name",
                           G_CALLBACK(refresh), self);

  return self;
}

void whelk_network_panel_show(WhelkNetworkPanel *self, const char *page) {
  if (!self)
    return;

  if (page && *page)
    gtk_stack_set_visible_child_name(GTK_STACK(self->stack), page);

  GtkWindow *window = whelk_surface_get_window(self->surface);

  if (window)
    gtk_widget_set_visible(GTK_WIDGET(window), TRUE);

  refresh(self);
}

void whelk_network_panel_hide(WhelkNetworkPanel *self) {
  if (!self)
    return;

  GtkWindow *window = whelk_surface_get_window(self->surface);

  if (window)
    gtk_widget_set_visible(GTK_WIDGET(window), FALSE);

  if (self->refresh_timeout != 0) {
    g_source_remove(self->refresh_timeout);
    self->refresh_timeout = 0;
  }
}

gboolean whelk_network_panel_is_visible(WhelkNetworkPanel *self) {
  if (!self)
    return FALSE;

  GtkWindow *window = whelk_surface_get_window(self->surface);

  return window && gtk_widget_get_visible(GTK_WIDGET(window));
}

gboolean whelk_network_panel_showing(WhelkNetworkPanel *self,
                                     const char *page) {
  if (!self || !whelk_network_panel_is_visible(self))
    return FALSE;

  return g_strcmp0(gtk_stack_get_visible_child_name(GTK_STACK(self->stack)),
                   page) == 0;
}

/* =========================================================================
 * LIFECYCLE
 * ========================================================================= */

static void whelk_network_panel_dispose(GObject *object) {
  WhelkNetworkPanel *self = WHELK_NETWORK_PANEL(object);

  if (self->refresh_timeout != 0) {
    g_source_remove(self->refresh_timeout);
    self->refresh_timeout = 0;
  }

  if (self->skin)
    whelk_style_detach(self->skin, gdk_display_get_default());

  g_clear_object(&self->surface);
  g_clear_pointer(&self->style, whelk_bar_config_free);
  g_free(self->pending_ssid);

  G_OBJECT_CLASS(whelk_network_panel_parent_class)->dispose(object);
}

static void whelk_network_panel_class_init(WhelkNetworkPanelClass *klass) {
  GObjectClass *object_class = G_OBJECT_CLASS(klass);

  object_class->dispose = whelk_network_panel_dispose;
}

static void whelk_network_panel_init(WhelkNetworkPanel *self) { (void)self; }
