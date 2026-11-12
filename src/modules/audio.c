// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include <string.h>

#include "util/format.h"

/* =========================================================================
 * AUDIO
 * =========================================================================
 * wpctl is the PipeWire answer and pactl the PulseAudio one; this reads
 * wpctl's "Volume: 0.42" and falls back to pactl's percentage when the first is
 * not installed, so a machine gets a volume either way.
 */

static const char *audio_command(void) {
  return "wpctl get-volume @DEFAULT_AUDIO_SINK@ 2>/dev/null || "
         "pactl get-sink-volume @DEFAULT_SINK@ 2>/dev/null";
}

static void audio_apply(WhelkModule *module, const char *output) {
  int volume = 0;
  gboolean muted = FALSE;

  if (output) {
    const char *volume_label = strstr(output, "Volume:");

    if (volume_label) {
      const double level =
          g_ascii_strtod(volume_label + strlen("Volume:"), NULL);
      volume = (int)(level * 100.0 + 0.5);
      muted = strstr(output, "[MUTED]") != NULL;
    } else {
      /* pactl: "Volume: front-left: 26214 /  40% / -23.88 dB, ..." */
      const char *percent = strchr(output, '%');

      if (percent) {
        const char *start = percent;

        while (start > output && g_ascii_isdigit(start[-1]))
          start--;

        volume = (int)g_ascii_strtoull(start, NULL, 10);
      }
    }
  }

  if (volume < 0)
    volume = 0;
  if (volume > 150)
    volume = 150;

  g_autofree char *percent = g_strdup_printf("%d", volume);

  GHashTable *values = whelk_values_new();
  whelk_values_set(values, "volume", percent);

  const char *muted_format = whelk_spec_str(module->spec, "muted-format", NULL);
  const char *format = whelk_spec_str(module->spec, "format", "{icon} {volume}%");
  const char *icon = whelk_spec_str(module->spec, "icon", "");

  if (muted) {
    const char *muted_icon = whelk_spec_str(module->spec, "muted-icon", NULL);

    if (muted_format)
      format = muted_format;
    if (muted_icon)
      icon = muted_icon;
  }

  whelk_module_present_format(module, format, icon,
                              whelk_spec_str(module->spec, "tooltip", NULL),
                              values);
}

static void audio_on_command(WhelkModule *module, const char *text,
                             gpointer user_data) {
  (void)user_data;

  audio_apply(module, text);
}

static void audio_update(WhelkModule *module) {
  whelk_module_command(module, audio_command(), audio_on_command, NULL);
}

WhelkModule *whelk_audio_module_new(const WhelkModuleSpec *spec,
                                    const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("audio", spec, bar);

  whelk_module_add_label(module);
  module->update = audio_update;
  whelk_module_install(module, 2);

  return module;
}
