// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/lockauth.h"

#include <dlfcn.h>
#include <fcntl.h>
#include <glib/gstdio.h>
#include <pthread.h>
#include <signal.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
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

#ifdef WHELK_PAM_LIBDIR
  /* The library whelk was built against, named in full: a Nix or Guix build
   * may have its rpath shrunk to the libraries it links, and this one is only
   * ever opened by name, so the bare soname below can find nothing. */
  add_path(paths, WHELK_PAM_LIBDIR "/libpam.so.0");
#endif

  /* The loader's own search: a distribution's ld cache, or the build's rpath
   * when it survived. */
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

/* =========================================================================
 * THE HOST LIBRARY'S OWN DEPENDENCIES
 * =========================================================================
 * A distribution's libpam names libaudit and libeconf.  A store-built whelk
 * links a store C library, so its loader does not search /lib64 and dlopen of
 * the host's libpam fails before PAM is ever asked.  Loading the missing
 * objects by absolute host path with RTLD_GLOBAL is enough: the host library
 * then loads, and with it the host's module directory and pam.d.
 *
 * Only fixed distribution directories are searched, never LD_LIBRARY_PATH or
 * a path from the environment, and a glibc soname is never loaded from the
 * host: the process already has one, and mixing two would be worse than a
 * locked screen. */

static const char *const host_library_dirs[] = {
    "/lib64",
    "/usr/lib64",
    "/lib",
    "/usr/lib",
    NULL,
};

static gboolean is_glibc_soname(const char *soname) {
  return g_str_equal(soname, "libc.so.6") ||
         g_str_equal(soname, "libm.so.6") ||
         g_str_equal(soname, "libmvec.so.1") ||
         g_str_equal(soname, "libpthread.so.0") ||
         g_str_equal(soname, "libdl.so.2") ||
         g_str_equal(soname, "librt.so.1") ||
         g_str_equal(soname, "libresolv.so.2") ||
         g_str_equal(soname, "libutil.so.1") ||
         g_str_equal(soname, "libanl.so.1") ||
         g_str_has_prefix(soname, "ld-linux") ||
         g_str_has_prefix(soname, "libnss_");
}

static void *dlopen_host_object(const char *path, guint depth, int flags);

/* Loads one soname from a distribution directory.  A missing dependency of the
 * object itself is resolved first, recursively, because libaudit may name
 * libcap-ng in the same way libpam names libaudit. */
static gboolean load_host_soname(const char *soname, guint depth) {
  if (!soname || !*soname || strchr(soname, '/') || depth > 4)
    return FALSE;

  if (is_glibc_soname(soname))
    return TRUE;

  /* Already loaded (by this process or by an earlier candidate)?  Then the
   * soname is satisfied and no host copy is needed. */
  void *loaded = dlopen(soname, RTLD_NOW | RTLD_NOLOAD);

  if (loaded) {
    dlclose(loaded);
    return TRUE;
  }

  for (guint i = 0; host_library_dirs[i]; i++) {
    g_autofree char *path = g_build_filename(host_library_dirs[i], soname, NULL);

    if (!g_file_test(path, G_FILE_TEST_IS_REGULAR))
      continue;

    if (dlopen_host_object(path, depth, RTLD_NOW | RTLD_GLOBAL))
      return TRUE;
  }

  /* Debian multiarch: /usr/lib/x86_64-linux-gnu and friends. */
  for (guint r = 0; r < 2; r++) {
    const char *root = r == 0 ? "/usr/lib" : "/lib";
    GDir *dir = g_dir_open(root, 0, NULL);

    if (!dir)
      continue;

    const char *name;

    while ((name = g_dir_read_name(dir))) {
      if (!g_str_has_suffix(name, "-linux-gnu"))
        continue;

      g_autofree char *path = g_build_filename(root, name, soname, NULL);

      if (g_file_test(path, G_FILE_TEST_IS_REGULAR) &&
          dlopen_host_object(path, depth, RTLD_NOW | RTLD_GLOBAL)) {
        g_dir_close(dir);
        return TRUE;
      }
    }

    g_dir_close(dir);
  }

  return FALSE;
}

/* dlopen, but when the loader names a missing host object, load that object
 * from a distribution directory and retry.  The retry count is bounded: an
 * object graph deeper than this is not one whelk can reason about. */
static void *dlopen_host_object(const char *path, guint depth, int flags) {
  void *handle = dlopen(path, flags);

  for (guint attempt = 0; !handle && attempt < 8; attempt++) {
    const char *error = dlerror();

    if (!error)
      break;

    const char *marker = strstr(error, ": cannot open shared object file");

    if (!marker)
      break;

    g_autofree char *missing = g_strndup(error, (gsize)(marker - error));

    if (!load_host_soname(missing, depth + 1))
      break;

    handle = dlopen(path, flags);
  }

  return handle;
}

static gboolean pam_api_resolve(WhelkPam *api, const char *path) {
  /* A host candidate gets the dependency shim; a store candidate is reached
   * through the build's own rpath and loads as it always has. */
  const gboolean host = g_str_has_prefix(path, "/usr/") ||
                        g_str_has_prefix(path, "/lib");
  void *handle = host ? dlopen_host_object(path, 0, RTLD_NOW | RTLD_LOCAL)
                      : dlopen(path, RTLD_NOW | RTLD_LOCAL);

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

gboolean whelk_lock_fallback_allowed(gboolean admin_policy,
                                     gboolean helper_available,
                                     gboolean strict) {
  return !admin_policy && helper_available && !strict;
}

gboolean whelk_lock_strict_value(const char *value) {
  if (!value)
    return FALSE;

  g_autofree char *trimmed = g_strdup(value);

  g_strstrip(trimmed);

  return g_str_equal(trimmed, "1") ||
         g_ascii_strcasecmp(trimmed, "true") == 0 ||
         g_ascii_strcasecmp(trimmed, "yes") == 0 ||
         g_ascii_strcasecmp(trimmed, "on") == 0;
}

gboolean whelk_lock_strict_mode(void) {
  return whelk_lock_strict_value(g_getenv("WHELK_LOCK_STRICT"));
}

/* =========================================================================
 * THE HOST'S PASSWORD HELPER
 * =========================================================================
 * The helper is the host's own setuid `unix_chkpwd'; asking it is the same
 * check pam_unix makes.  It is only reached when no administrator service file
 * owns the answer, and never in strict mode.  See the header. */

static const char *const local_helper_paths[] = {
    "/usr/sbin/unix_chkpwd",
    "/sbin/unix_chkpwd",
    "/usr/libexec/unix_chkpwd",
    NULL,
};

char *whelk_lock_local_helper(void) {
  for (guint i = 0; local_helper_paths[i]; i++) {
    struct stat info;

    if (stat(local_helper_paths[i], &info) != 0)
      continue;

    if (!S_ISREG(info.st_mode) || info.st_uid != 0)
      continue;

    /* setuid root, and nobody else may replace the file.  A helper another
     * user could write is a password thief, not an authenticator. */
    if ((info.st_mode & (S_ISUID | S_IWGRP | S_IWOTH)) != S_ISUID)
      continue;

    return g_strdup(local_helper_paths[i]);
  }

  return NULL;
}

/* Writing to a pipe whose reader has exited raises SIGPIPE, and SIGPIPE would
 * end the shell.  The authentication worker masks it around the one write and
 * drains it afterwards, so the process-wide disposition is left alone. */
static gboolean write_password(int fd, const char *password) {
  sigset_t pipe_set, old_set, pending;
  gboolean failed = FALSE;

  sigemptyset(&pipe_set);
  sigaddset(&pipe_set, SIGPIPE);
  pthread_sigmask(SIG_BLOCK, &pipe_set, &old_set);

  const char *cursor = password;

  while (*cursor) {
    ssize_t count = write(fd, cursor, strlen(cursor));

    if (count <= 0) {
      failed = TRUE;
      break;
    }

    cursor += count;
  }

  /* unix_chkpwd reads one line; the newline flushes its read loop. */
  if (!failed && write(fd, "\n", 1) != 1)
    failed = TRUE;

  if (failed) {
    sigpending(&pending);

    if (sigismember(&pending, SIGPIPE)) {
      struct timespec zero = {0, 0};
      sigtimedwait(&pipe_set, NULL, &zero);
    }
  }

  pthread_sigmask(SIG_SETMASK, &old_set, NULL);

  return !failed;
}

WhelkLocalResult whelk_lock_local_verify(const char *password) {
  g_autofree char *helper = whelk_lock_local_helper();

  if (!helper || !password)
    return WHELK_LOCAL_UNAVAILABLE;

  const char *user = g_get_user_name();

  if (!user || !*user)
    return WHELK_LOCAL_UNAVAILABLE;

  int input[2];

  if (pipe2(input, O_CLOEXEC) != 0)
    return WHELK_LOCAL_UNAVAILABLE;

  const pid_t pid = fork();

  if (pid < 0) {
    close(input[0]);
    close(input[1]);
    return WHELK_LOCAL_UNAVAILABLE;
  }

  if (pid == 0) {
    /* The child only needs the password on stdin; its diagnostics are not
     * useful to a lock screen and must not reach the session's terminal. */
    if (dup2(input[0], STDIN_FILENO) < 0)
      _exit(127);

    int quiet = open("/dev/null", O_WRONLY | O_CLOEXEC);

    if (quiet >= 0) {
      dup2(quiet, STDOUT_FILENO);
      dup2(quiet, STDERR_FILENO);
    }

    char *argv[] = {helper, (char *)user, "nonull", NULL};
    char *envp[] = {"PATH=/usr/sbin:/usr/bin:/sbin:/bin", NULL};

    execve(helper, argv, envp);
    _exit(127);
  }

  close(input[0]);

  const gboolean delivered = write_password(input[1], password);

  close(input[1]);

  int status = 0;
  const pid_t reaped = waitpid(pid, &status, 0);

  if (!delivered || reaped != pid || !WIFEXITED(status))
    return WHELK_LOCAL_UNAVAILABLE;

  switch (WEXITSTATUS(status)) {
  case PAM_SUCCESS:
    return WHELK_LOCAL_OK;
  case PAM_AUTH_ERR:
  case PAM_USER_UNKNOWN:
  case PAM_MAXTRIES:
  case PAM_NEW_AUTHTOK_REQD:
    return WHELK_LOCAL_DENIED;
  default:
    return WHELK_LOCAL_UNAVAILABLE;
  }
}
