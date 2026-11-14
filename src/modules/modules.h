// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_MODULES_H
#define WHELK_MODULES_H

#include "module.h"

/* =========================================================================
 * THE BUILT-IN MODULES
 * =========================================================================
 * One factory per kind, all with the same signature so module.c's table has
 * nothing to translate.  The baseline set is the one the heks-linux Waybar
 * configuration uses, plus the workspace switcher and the tray that make the
 * bar a shell rather than a readout.
 */

WhelkModule *whelk_audio_module_new(const WhelkModuleSpec *spec,
                                    const WhelkBarConfig *bar);
WhelkModule *whelk_backlight_module_new(const WhelkModuleSpec *spec,
                                        const WhelkBarConfig *bar);
WhelkModule *whelk_battery_module_new(const WhelkModuleSpec *spec,
                                      const WhelkBarConfig *bar);
WhelkModule *whelk_bluetooth_module_new(const WhelkModuleSpec *spec,
                                        const WhelkBarConfig *bar);
WhelkModule *whelk_clock_module_new(const WhelkModuleSpec *spec,
                                    const WhelkBarConfig *bar);
WhelkModule *whelk_clipboard_module_new(const WhelkModuleSpec *spec,
                                        const WhelkBarConfig *bar);
WhelkModule *whelk_cpu_module_new(const WhelkModuleSpec *spec,
                                  const WhelkBarConfig *bar);
WhelkModule *whelk_custom_module_new(const WhelkModuleSpec *spec,
                                     const WhelkBarConfig *bar);
WhelkModule *whelk_scheme_module_new(const WhelkModuleSpec *spec,
                                     const WhelkBarConfig *bar);
WhelkModule *whelk_dnd_module_new(const WhelkModuleSpec *spec,
                                  const WhelkBarConfig *bar);
WhelkModule *whelk_keyboard_module_new(const WhelkModuleSpec *spec,
                                       const WhelkBarConfig *bar);
WhelkModule *whelk_language_module_new(const WhelkModuleSpec *spec,
                                       const WhelkBarConfig *bar);
WhelkModule *whelk_memory_module_new(const WhelkModuleSpec *spec,
                                     const WhelkBarConfig *bar);
WhelkModule *whelk_menu_module_new(const WhelkModuleSpec *spec,
                                   const WhelkBarConfig *bar);
WhelkModule *whelk_notifications_module_new(const WhelkModuleSpec *spec,
                                            const WhelkBarConfig *bar);
WhelkModule *whelk_power_profile_module_new(const WhelkModuleSpec *spec,
                                            const WhelkBarConfig *bar);
WhelkModule *whelk_start_module_new(const WhelkModuleSpec *spec,
                                    const WhelkBarConfig *bar);
WhelkModule *whelk_tray_module_new(const WhelkModuleSpec *spec,
                                   const WhelkBarConfig *bar);
WhelkModule *whelk_wifi_module_new(const WhelkModuleSpec *spec,
                                   const WhelkBarConfig *bar);
WhelkModule *whelk_window_module_new(const WhelkModuleSpec *spec,
                                     const WhelkBarConfig *bar);
WhelkModule *whelk_workspaces_module_new(const WhelkModuleSpec *spec,
                                         const WhelkBarConfig *bar);

#endif // WHELK_MODULES_H
