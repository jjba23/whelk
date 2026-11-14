# whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
#
# SPDX-License-Identifier: MIT
# Copyright © Josep Bigorra <jjbigorra@gmail.com>

# ------------------------------------------------------------------------------
# Project Initialization & Build Options
# ------------------------------------------------------------------------------
project('whelk', 'c',
  version: '0.20.1',
  meson_version: '>= 0.62.0',
  default_options: [
    'c_std=gnu17',            # Target standard C17 with GNU extensions
    'warning_level=3',        # -Wall, -Wextra, -Wpedantic
    'werror=true',            # Treat compiler warnings as fatal errors
    'b_lto=true',             # Link-Time Optimization
    'b_pie=true',             # Position-Independent Executable
    'buildtype=release',
    'strip=true'
  ]
)

gnome = import('gnome')
cc = meson.get_compiler('c')

# ------------------------------------------------------------------------------
# Strict Compiler Warning Flags
# ------------------------------------------------------------------------------
desired_warning_flags = [
  '-Wpedantic',
  '-Wshadow',
  '-Wstrict-prototypes',
  '-Wmissing-prototypes',
  '-Wredundant-decls',
  '-Wimplicit-fallthrough',
  '-Wvla',
  '-Wundef',
  '-Wformat=2',
  '-Wformat-security',
  '-Werror=implicit-function-declaration',
  '-Werror=incompatible-pointer-types',
  '-Werror=int-conversion',
  '-Wno-overlength-strings',
]

add_project_arguments(
  cc.get_supported_arguments(desired_warning_flags),
  language: 'c'
)

# Expose GNU extensions in standard system C headers (strdup, asprintf, ...)
add_project_arguments('-D_GNU_SOURCE', language: 'c')

# ------------------------------------------------------------------------------
# Third-Party Library Dependencies
# ------------------------------------------------------------------------------
gtk_dep = dependency('gtk4', include_type: 'system')
guile_dep = dependency('guile-3.0', include_type: 'system')
# 1.2 added GtkSessionLockInstance, the GTK4 binding for ext-session-lock-v1
# used by the hardened locker.  The fallback overlay still uses the same
# library's layer-shell half.
layer_shell_dep = dependency('gtk4-layer-shell-0', version: '>= 1.2.0',
  include_type: 'system')
wayland_client_dep = dependency('wayland-client', include_type: 'system')
# g_desktop_app_info_new_from_filename() lives in gio-unix; the launcher reads
# the machine's .desktop entries with it.
gio_unix_dep = dependency('gio-unix-2.0', include_type: 'system')
# PAM: the screen locker asks the session's authentication service, never a
# password file of its own.  Optional: without it the locker fails closed.
pam_dep = dependency('pam', required: false)

# libm: the workspace bar and the metric readouts use round()/fabs().
libm_dep = cc.find_library('m', required: false)

# Xlib: on an X11 desktop a bar reserves its edge the EWMH way, with a dock
# window type and struts, because there is no layer-shell to ask.  Optional: a
# Wayland-only build simply has no X11 fallback compiled into it.
x11_dep = dependency('x11', include_type: 'system', required: false)

whelk_deps = [
  gtk_dep,
  guile_dep,
  layer_shell_dep,
  wayland_client_dep,
  gio_unix_dep,
  libm_dep,
]

if x11_dep.found()
  whelk_deps += x11_dep
  add_project_arguments('-DHAVE_X11', language: 'c')
endif

if pam_dep.found()
  whelk_deps += pam_dep
  add_project_arguments('-DHAVE_PAM', language: 'c')
endif

# ------------------------------------------------------------------------------
# GLib Resource Compilation
# ------------------------------------------------------------------------------
# The structural stylesheet is embedded in the binary. The per-bar skin is
# generated from the Guile configuration at runtime (see src/bar.c).
whelk_resources = gnome.compile_resources(
  'whelk-resources',
  'resources/whelk.gresource.xml',
  source_dir: 'resources',
  c_name: 'whelk',
)

# ------------------------------------------------------------------------------
# Installation Paths & C Preprocessor Macros
# ------------------------------------------------------------------------------
datadir = get_option('prefix') / get_option('datadir')
scheme_install_path = datadir / 'guile/site/3.0'

add_project_arguments(
  '-DWHELK_SCHEME_DIR="' + scheme_install_path + '"',
  '-DDEFAULT_GSK_RENDERER="gl"',
  '-DPACKAGE_VERSION="' + meson.project_version() + '"',
  language: 'c'
)

# The Scheme modules that implement the configuration DSL.
install_subdir('scheme/whelk', install_dir: scheme_install_path)

# ------------------------------------------------------------------------------
# Desktop Integration
# ------------------------------------------------------------------------------
whelk_app_id = 'org.jointhefreeworld.whelk'

whelk_desktop_file = files('data' / whelk_app_id + '.desktop')
install_data(
  whelk_desktop_file,
  install_dir: get_option('datadir') / 'applications',
)

# The application icon, a whelk shell drawn for it in the Tela idiom: a
# hicolor tree so the desktop entry and the shell's own windows resolve it by
# the application id, and a scalable source beside the rendered sizes.
install_subdir(
  'data/icons',
  install_dir: get_option('datadir') / 'icons',
  strip_directory: true,
)

# ------------------------------------------------------------------------------
# Source Code Definitions & Executable Build
# ------------------------------------------------------------------------------
inc = include_directories('src')

whelk_sources = files(
  'src/about.c',
  'src/bar.c',
  'src/clipboard.c',
  'src/config.c',
  'src/guile_bridge.c',
  'src/launcher.c',
  'src/locker.c',
  'src/module.c',
  'src/network_panel.c',
  'src/notification_center.c',
  'src/notifications.c',
  'src/osd.c',
  'src/preferences.c',
  'src/shell.c',
  'src/style.c',
  'src/surface.c',
  'src/modules/audio.c',
  'src/modules/backlight.c',
  'src/modules/battery.c',
  'src/modules/bluetooth.c',
  'src/modules/clock.c',
  'src/modules/clipboard.c',
  'src/modules/cpu.c',
  'src/modules/custom.c',
  'src/modules/dnd.c',
  'src/modules/keyboard.c',
  'src/modules/language.c',
  'src/modules/memory.c',
  'src/modules/menu.c',
  'src/modules/notifications.c',
  'src/modules/power_profile.c',
  'src/modules/scheme.c',
  'src/modules/start.c',
  'src/modules/tray.c',
  'src/modules/wifi.c',
  'src/modules/window.c',
  'src/modules/workspaces.c',
  'src/util/applets.c',
  'src/util/atspimenus.c',
  'src/util/barmonitors.c',
  'src/util/calc.c',
  'src/util/chord.c',
  'src/util/click.c',
  'src/util/command.c',
  'src/util/dbusmenu.c',
  'src/util/filesearch.c',
  'src/util/format.c',
  'src/util/fuzzy.c',
  'src/util/json.c',
  'src/util/level.c',
  'src/util/launchergeometry.c',
  'src/util/lockpolicy.c',
  'src/util/menuaction.c',
  'src/util/menudict.c',
  'src/util/menusearch.c',
  'src/util/menutree.c',
  'src/util/menuprobe.c',
  'src/util/gtkmenu.c',
  'src/util/session.c',
  'src/util/shortcutmaps.c',
  'src/util/desktop.c',
  'src/util/nmcli.c',
  'src/util/sysfs.c',
  'src/util/trayicon.c',
  'src/widgets.c',
)

whelk_main_source = files('src/main.c')

whelk_exe = executable('whelk',
  whelk_sources,
  whelk_main_source,
  whelk_resources,
  include_directories: inc,
  dependencies: whelk_deps,
  gnu_symbol_visibility: 'hidden',
  install: true,
)

# The golden-image renderer is the shell without its entry point: it builds a
# real bar, presents it on Broadway, snapshots it and compares the pixels.
bar_preview = executable('bar_preview',
  'tests/bar_preview.c',
  whelk_sources,
  whelk_resources,
  include_directories: inc,
  dependencies: whelk_deps,
  install: false,
)

# ------------------------------------------------------------------------------
# Test Suite Configuration
# ------------------------------------------------------------------------------
# The pure halves of the shell are tested without a display: the JSON reader
# that parses niri's IPC answers, the option tables the Guile bridge fills, and
# the format substitution every module renders through.
test_env = environment()
test_env.set('GSETTINGS_BACKEND', 'memory')
# The C tests ask the bridge for the Scheme modules, so they have to find them.
test_env.set('GUILE_LOAD_PATH',
             join_paths(meson.project_source_root(), 'scheme'))
test_env.set('WHELK_CSS_PATH',
             join_paths(meson.project_source_root(), 'resources', 'css',
                        'whelk.css'))
test_env.set('WHELK_TEST_DIR',
             join_paths(meson.project_source_root(), 'tests'))

unit_tests = [
  {
    'id': 'json',
    'sources': ['src/util/json.c'],
  },
  {
    'id': 'chord',
    'sources': ['src/util/chord.c'],
  },
  {
    'id': 'shortcutmaps',
    'sources': [
      'src/util/shortcutmaps.c',
      'src/util/menutree.c',
      'src/util/menudict.c',
      'src/util/chord.c',
      'src/util/json.c',
    ],
  },
  {
    'id': 'menuprobe',
    'sources': [
      'src/util/menuprobe.c',
      'src/util/session.c',
      'src/util/json.c',
    ],
  },
  {
    'id': 'config',
    'sources': ['src/config.c', 'src/util/format.c', 'src/util/sysfs.c'],
  },
  {
    'id': 'barmonitors',
    'sources': ['src/config.c', 'src/util/barmonitors.c'],
  },
  {
    'id': 'preferences',
    'sources': [
      'src/preferences.c',
      'src/guile_bridge.c',
      'src/config.c',
      'src/util/format.c',
      'src/util/menudict.c',
      'src/util/chord.c',
      'src/util/menutree.c',
    ],
  },
  {
    'id': 'widgetconfig',
    'sources': [
      'src/guile_bridge.c',
      'src/config.c',
      'src/util/format.c',
      'src/util/menudict.c',
      'src/util/chord.c',
      'src/util/menutree.c',
    ],
  },
  {
    'id': 'schemewidget',
    'sources': [
      'src/guile_bridge.c',
      'src/style.c',
      'src/config.c',
      'src/util/format.c',
      'src/util/menudict.c',
      'src/util/chord.c',
      'src/util/menutree.c',
    ],
  },
  {
    'id': 'launcher',
    'sources': [
      'src/util/fuzzy.c',
      'src/util/calc.c',
      'src/util/launchergeometry.c',
    ],
  },
  {
    'id': 'filesearch',
    'sources': ['src/util/filesearch.c'],
  },
  # The dictionary itself is Scheme, so these two ask the bridge for the real
  # one: they fail if the module is missing, malformed or no longer matches what
  # a compositor reports.
  {
    'id': 'menudict',
    'sources': [
      'src/util/menudict.c',
      'src/util/chord.c',
      'src/guile_bridge.c',
      'src/config.c',
      'src/util/format.c',
    ],
  },
  {
    'id': 'menutree',
    'sources': [
      'src/util/menutree.c',
      'src/util/menudict.c',
      'src/util/chord.c',
      'src/guile_bridge.c',
      'src/config.c',
      'src/util/format.c',
    ],
  },
  {
    'id': 'orbit',
    'sources': [
      'src/util/menutree.c',
      'src/util/menudict.c',
      'src/util/chord.c',
      'src/guile_bridge.c',
      'src/config.c',
      'src/util/format.c',
    ],
  },
  {
    'id': 'gtkmenu',
    'sources': [
      'src/util/gtkmenu.c',
      'src/util/menutree.c',
      'src/util/menudict.c',
      'src/util/chord.c',
      'src/guile_bridge.c',
      'src/config.c',
      'src/util/format.c',
    ],
  },
  {
    'id': 'session',
    'sources': ['src/util/session.c', 'src/util/json.c'],
  },
  {
    'id': 'desktop',
    'sources': ['src/util/desktop.c'],
  },
  {
    'id': 'applets',
    'sources': ['src/util/applets.c'],
  },
  {
    'id': 'command',
    'sources': ['src/util/command.c', 'src/util/format.c'],
  },
  {
    'id': 'nmcli',
    'sources': ['src/util/nmcli.c'],
  },
  {
    'id': 'clipboard',
    'sources': [
      'src/clipboard.c',
      'src/util/command.c',
      'src/util/format.c',
    ],
  },
  {
    'id': 'menusearch',
    'sources': [
      'src/util/menusearch.c',
      'src/util/menuaction.c',
      'src/util/menutree.c',
      'src/util/menudict.c',
      'src/util/chord.c',
      'src/guile_bridge.c',
      'src/config.c',
      'src/util/format.c',
      'src/util/command.c',
    ],
  },
  {
    'id': 'lockpolicy',
    'sources': ['src/util/lockpolicy.c'],
  },
  {
    'id': 'atspimenus',
    'sources': [
      'src/util/atspimenus.c',
      'src/util/menutree.c',
      'src/util/chord.c',
    ],
  },
  {
    'id': 'dbusmenu',
    'sources': [
      'src/util/dbusmenu.c',
      'src/util/menutree.c',
      'src/util/menudict.c',
      'src/util/chord.c',
      'src/guile_bridge.c',
      'src/config.c',
      'src/util/format.c',
    ],
  },
  {
    'id': 'level',
    'sources': ['src/util/level.c'],
  },
  {
    'id': 'trayicon',
    'sources': ['src/util/trayicon.c'],
  },
]

foreach t : unit_tests
  exe = executable(
    'test_' + t['id'],
    'tests/test_' + t['id'] + '.c',
    t['sources'],
    include_directories: inc,
    dependencies: whelk_deps,
  )
  test(t['id'] + '_unit_tests', exe, env: test_env)
endforeach

# Drawing is the half the pure tests cannot see.  The renderer runs under
# Broadway (a display server with no monitor), saves the bar and diffs it with
# the committed reference; a colour, padding or font regression is a failure.
broadwayd = find_program('gtk4-broadwayd', required: false)

if broadwayd.found()
  test(
    'golden_bar_image',
    find_program('tests/golden-bar.sh'),
    args: [
      meson.current_build_dir(),
      meson.project_source_root(),
      join_paths(meson.current_build_dir(), 'bar-golden.png'),
      join_paths(meson.project_source_root(), 'tests', 'golden', 'bar.png'),
    ],
    env: test_env,
    timeout: 60,
  )
endif

# The configuration engine is Scheme, and so is its share of the tests: a
# settings file is read without a display or a running shell.
guile = find_program('guile', required: false)

if guile.found()
  test(
    'themes_unit_tests',
    guile,
    args: [
      '--no-auto-compile',
      '-L', join_paths(meson.project_source_root(), 'scheme'),
      join_paths(meson.project_source_root(), 'tests', 'test_themes.scm'),
    ],
    env: test_env,
  )

  # The menu dictionary is data, and data can be wrong in ways C cannot see:
  # an id claimed twice, an entry without its chord.
  test(
    'menudict_unit_tests',
    guile,
    args: [
      '--no-auto-compile',
      '-L', join_paths(meson.project_source_root(), 'scheme'),
      join_paths(meson.project_source_root(), 'tests', 'test_menudict.scm'),
    ],
    env: test_env,
  )

  # The built-in bar and the shipped example have to stay the same bar.
  test(
    'schememodule_unit_tests',
    guile,
    args: [
      '--no-auto-compile',
      '-L', join_paths(meson.project_source_root(), 'scheme'),
      join_paths(meson.project_source_root(), 'tests', 'test_schememodule.scm'),
    ],
    env: test_env,
  )

  test(
    'defaults_unit_tests',
    guile,
    args: [
      '--no-auto-compile',
      '-L', join_paths(meson.project_source_root(), 'scheme'),
      join_paths(meson.project_source_root(), 'tests', 'test_defaults.scm'),
      join_paths(meson.project_source_root(), 'settings', 'settings.scm'),
    ],
    env: test_env,
  )

  test(
    'barmonitors_unit_tests',
    guile,
    args: [
      '--no-auto-compile',
      '-L', join_paths(meson.project_source_root(), 'scheme'),
      join_paths(meson.project_source_root(), 'tests', 'test_barmonitors.scm'),
    ],
    env: test_env,
  )

  test(
    'prefs_unit_tests',
    guile,
    args: [
      '--no-auto-compile',
      '-L', join_paths(meson.project_source_root(), 'scheme'),
      join_paths(meson.project_source_root(), 'tests', 'test_prefs.scm'),
    ],
    env: test_env,
  )

  # Desktop widgets are configuration too: the standard set, its placement
  # and the bridge view a floating surface is built from.
  test(
    'widgets_unit_tests',
    guile,
    args: [
      '--no-auto-compile',
      '-L', join_paths(meson.project_source_root(), 'scheme'),
      join_paths(meson.project_source_root(), 'tests', 'test_widgets.scm'),
    ],
    env: test_env,
  )
endif
