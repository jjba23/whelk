// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/lockauth.h"

#include <dlfcn.h>
#include <fcntl.h>
#include <glib/gstdio.h>
#include <string.h>
#include <unistd.h>

/* The host's fixed locations, most specific first: /usr/lib64 is Fedora,
 * openSUSE and their relatives.  The Debian multiarch directories carry an
 * architecture triple, so they are discovered instead of listed. */
static const char *const host_library_paths[] = {
    "/usr/lib64/libpam.so.0",
    "/lib64/libpam.so.0",
    "/usr/lib/libpam.so.0",
    "/lib/libpam.so.0",
    NULL,
};

static void add_path(GPtrArray *paths, const char *path) {
  for (guint i = 0; i < paths->len; i++)
    if (g_strcmp0(g_ptr_array_index(paths, i), path) == 0)
      return;

  g_ptr_array_add(paths, g_strdup(path));
}

static void add_multiarch_paths(GPtrArray *paths) {
  static const char *const roots[] = {"/usr/lib", "/lib", NULL};

  for (guint r = 0; roots[r]; r++) {
    GDir *dir = g_dir_open(roots[r], 0, NULL);

    if (!dir)
      continue;

    const char *name;

    while ((name = g_dir_read_name(dir))) {
      if (!g_str_has_suffix(name, "-linux-gnu"))
        continue;

      g_autofree char *path =
          g_build_filename(roots[r], name, "libpam.so.0", NULL);

      if (g_file_test(path, G_FILE_TEST_IS_REGULAR))
        add_path(paths, path);
    }

    g_dir_close(dir);
  }
}

char **whelk_lock_pam_library_candidates(void) {
  GPtrArray *paths = g_ptr_array_new_with_free_func(g_free);

  for (guint i = 0; host_library_paths[i]; i++)
    add_path(paths, host_library_paths[i]);

  add_multiarch_paths(paths);

  /* The build's rpath decides this one: a store path on NixOS and Guix System,
   * the loader's own search everywhere else. */
  g_ptr_array_add(paths, g_strdup("libpam.so.0"));

  g_ptr_array_add(paths, NULL);

  return (char **)g_ptr_array_free(paths, FALSE);
}

char *whelk_lock_pam_confdir(const char *runtime_dir) {
  if (!runtime_dir || runtime_dir[0] != '/')
    return NULL;

  g_autofree char *dir = g_build_filename(runtime_dir, "whelk", "pam.d", NULL);

  if (g_mkdir_with_parents(dir, 0700) != 0)
    return NULL;

  /* The directory may predate this call, so its mode is set rather than
   * assumed: PAM refuses a configuration others can write. */
  if (g_chmod(dir, 0700) != 0)
    return NULL;

  g_autofree char *file = g_build_filename(dir, "whelk-lock", NULL);

  static const char stack[] = "auth required pam_unix.so\n"
                              "account required pam_unix.so\n";

  int fd = g_open(file, O_WRONLY | O_CREAT | O_TRUNC | O_NOFOLLOW | O_CLOEXEC,
                  0600);

  if (fd < 0)
    return NULL;

  if (fchmod(fd, 0600) != 0) {
    close(fd);
    return NULL;
  }

  gsize written = 0;

  while (written < sizeof(stack) - 1) {
    ssize_t count = write(fd, stack + written, sizeof(stack) - 1 - written);

    if (count <= 0) {
      close(fd);
      return NULL;
    }

    written += (gsize)count;
  }

  if (close(fd) != 0)
    return NULL;

  return g_steal_pointer(&dir);
}

static WhelkPam pam_api;
static gsize pam_api_ready = 0;
static char *pam_api_path = NULL;

static void pam_api_reset(WhelkPam *api) {
  if (api->handle)
    dlclose(api->handle);

  memset(api, 0, sizeof *api);
}

static gboolean pam_api_resolve(WhelkPam *api, const char *path) {
  void *handle = dlopen(path, RTLD_NOW | RTLD_LOCAL);

  if (!handle)
    return FALSE;

  memset(api, 0, sizeof *api);
  api->handle = handle;

  /* The POSIX way to take a function pointer out of dlsym's `void *'; a direct
   * cast is what -Wpedantic objects to. */
#define PAM_RESOLVE(field, symbol)                                             \
  do {                                                                         \
    *(void **)(&api->field) = dlsym(handle, symbol);                            \
    if (!api->field) {                                                         \
      pam_api_reset(api);                                                      \
      return FALSE;                                                            \
    }                                                                          \
  } while (0)

  PAM_RESOLVE(start, "pam_start");
  PAM_RESOLVE(end, "pam_end");
  PAM_RESOLVE(authenticate, "pam_authenticate");
  PAM_RESOLVE(acct_mgmt, "pam_acct_mgmt");
  PAM_RESOLVE(strerror, "pam_strerror");

#undef PAM_RESOLVE

  /* Optional: only a libpam older than 1.4 lacks it, and only a machine with no
   * session stack asks for it. */
  *(void **)(&api->start_confdir) = dlsym(handle, "pam_start_confdir");

  return TRUE;
}

const WhelkPam *whelk_lock_pam_open(void) {
  if (g_once_init_enter(&pam_api_ready)) {
    g_auto(GStrv) candidates = whelk_lock_pam_library_candidates();

    for (guint i = 0; candidates[i]; i++) {
      if (!pam_api_resolve(&pam_api, candidates[i]))
        continue;

      pam_api_path = g_strdup(candidates[i]);
      break;
    }

    g_once_init_leave(&pam_api_ready, 1);
  }

  return pam_api.handle ? &pam_api : NULL;
}

const char *whelk_lock_pam_path(void) { return pam_api_path; }
