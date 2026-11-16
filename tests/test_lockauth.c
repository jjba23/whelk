// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/lockauth.h"

#include <dlfcn.h>
#include <glib.h>
#include <glib/gstdio.h>
#include <string.h>
#include <unistd.h>
#include <sys/stat.h>

/* The host's library has to be tried before the build's, or a libpam from a
 * private store answers with its own module directory and every password comes
 * back as "authentication service cannot retrieve authentication info". */
static void test_candidates(void) {
  g_auto(GStrv) paths = whelk_lock_pam_library_candidates();

  g_assert_nonnull(paths);

  const guint length = g_strv_length(paths);

  g_assert_cmpuint(length, >=, 2);
  g_assert_cmpstr(paths[0], ==, "/usr/lib64/libpam.so.0");
  g_assert_cmpstr(paths[length - 1], ==, "libpam.so.0");

  /* A store-built whelk names its own libpam in full, because nixpkgs shrinks
   * an rpath nothing links against. */
#ifdef WHELK_PAM_LIBDIR
  g_assert_true(g_strv_contains((const char *const *)paths,
                                WHELK_PAM_LIBDIR "/libpam.so.0"));
#endif

  for (guint i = 0; i < length - 1; i++) {
    g_assert_true(g_path_is_absolute(paths[i]));
    g_assert_true(g_str_has_suffix(paths[i], "libpam.so.0"));

    for (guint j = i + 1; j < length; j++)
      g_assert_cmpstr(paths[i], !=, paths[j]);
  }
}

static int test_conversation(int count, const struct pam_message **messages,
                             struct pam_response **responses, void *data) {
  (void)data;

  if (count <= 0 || !messages || !responses)
    return PAM_CONV_ERR;

  struct pam_response *answers = g_new0(struct pam_response, (gsize)count);

  for (int i = 0; i < count; i++)
    if (messages[i]->msg_style == PAM_PROMPT_ECHO_OFF)
      answers[i].resp = g_strdup("whelk-lockauth-not-the-password");

  *responses = answers;
  return PAM_SUCCESS;
}

/* The bug this pins: a whelk built against a libpam in a private store loaded
 * that copy, whose module directory is the store, and its pam_unix could not
 * run the host's setuid unix_chkpwd - so every password was
 * PAM_AUTHINFO_UNAVAIL and the lock screen said authentication was
 * unavailable.  On a host that keeps PAM in /usr, that copy must be the one
 * loaded, and a name the host's pam_unix cannot find must answer
 * PAM_USER_UNKNOWN (pam_deny answers PAM_AUTH_ERR for every name instead). */
static void test_prefers_the_host_library(void) {
  g_auto(GStrv) paths = whelk_lock_pam_library_candidates();
  const char *host = NULL;

  for (guint i = 0; paths[i]; i++) {
    if (g_str_has_prefix(paths[i], "/usr/") &&
        g_file_test(paths[i], G_FILE_TEST_EXISTS)) {
      host = paths[i];
      break;
    }
  }

  if (!host) {
    g_test_skip("this machine keeps libpam outside /usr");
    return;
  }

  /* A hermetic build (Guix, Nix) links its own C library, and the host's
   * libpam then fails to resolve: the candidate list falls through to the
   * store's copy, which is the fallback it exists for.  Ask the loader whether
   * the host copy is loadable here before claiming that it must win, so the
   * assertion below only speaks about machines where the comparison means
   * something. */
  void *probe = dlopen(host, RTLD_NOW | RTLD_LOCAL);

  if (!probe) {
    g_test_skip("the host's libpam cannot be loaded by a binary built here");
    return;
  }

  dlclose(probe);

  const WhelkPam *pam = whelk_lock_pam_open();

  g_assert_nonnull(pam);
  g_assert_cmpstr(whelk_lock_pam_path(), ==, host);

  if (!pam->start_confdir) {
    g_test_skip("this machine has a libpam older than 1.4");
    return;
  }

  g_autofree char *runtime = g_dir_make_tmp("whelk-lockauth-XXXXXX", NULL);
  g_autofree char *confdir = whelk_lock_pam_confdir(runtime);

  g_assert_nonnull(confdir);

  struct pam_conv conversation = { test_conversation, NULL };
  pam_handle_t *handle = NULL;
  int status =
      pam->start_confdir("whelk-lock", "whelk-lockauth-no-such-user",
                         &conversation, confdir, &handle);

  g_assert_cmpint(status, ==, PAM_SUCCESS);

  status = pam->authenticate(handle, PAM_SILENT);
  g_assert_cmpint(status, ==, PAM_USER_UNKNOWN);

  pam->end(handle, status);

  g_autofree char *file = g_build_filename(confdir, "whelk-lock", NULL);
  g_remove(file);
  g_rmdir(confdir);
  g_autofree char *parent = g_build_filename(runtime, "whelk", NULL);
  g_rmdir(parent);
  g_rmdir(runtime);
}

static void test_confdir_writes_a_minimal_service(void) {
  g_autofree char *runtime = g_dir_make_tmp("whelk-lockauth-XXXXXX", NULL);

  g_assert_nonnull(runtime);

  g_autofree char *dir = whelk_lock_pam_confdir(runtime);
  g_autofree char *expected =
      g_build_filename(runtime, "whelk", "pam.d", NULL);

  g_assert_nonnull(dir);
  g_assert_cmpstr(dir, ==, expected);

  g_autofree char *file = g_build_filename(dir, "whelk-lock", NULL);
  g_autofree char *contents = NULL;

  g_assert_true(g_file_get_contents(file, &contents, NULL, NULL));
  g_assert_nonnull(strstr(contents, "auth required pam_unix.so"));
  g_assert_nonnull(strstr(contents, "account required pam_unix.so"));

  struct stat info;

  g_assert_cmpint(g_stat(file, &info), ==, 0);
  g_assert_cmpuint(info.st_mode & 0777, ==, 0600);
  g_assert_cmpint(g_stat(dir, &info), ==, 0);
  g_assert_cmpuint(info.st_mode & 0777, ==, 0700);

  /* A second lock in the same session rewrites the same file, not another. */
  g_autofree char *again = whelk_lock_pam_confdir(runtime);

  g_assert_cmpstr(again, ==, dir);

  g_remove(file);
  g_rmdir(dir);
  g_autofree char *parent = g_build_filename(runtime, "whelk", NULL);
  g_rmdir(parent);
  g_rmdir(runtime);
}

/* A PAM service another user could write is worse than a locked screen, so the
 * writer refuses anything but an absolute runtime directory and never follows a
 * symlink. */
static void test_confdir_refuses_unsafe_paths(void) {
  g_assert_null(whelk_lock_pam_confdir(NULL));
  g_assert_null(whelk_lock_pam_confdir(""));
  g_assert_null(whelk_lock_pam_confdir("run/user/1000"));

  g_autofree char *runtime = g_dir_make_tmp("whelk-lockauth-XXXXXX", NULL);

  g_assert_nonnull(runtime);

  g_autofree char *dir = g_build_filename(runtime, "whelk", "pam.d", NULL);

  g_assert_cmpint(g_mkdir_with_parents(dir, 0700), ==, 0);

  g_autofree char *elsewhere = g_build_filename(runtime, "elsewhere", NULL);
  g_autofree char *file = g_build_filename(dir, "whelk-lock", NULL);

  g_assert_true(g_file_set_contents(elsewhere, "auth required pam_deny.so\n",
                                    -1, NULL));
  g_assert_cmpint(symlink(elsewhere, file), ==, 0);
  g_assert_null(whelk_lock_pam_confdir(runtime));

  g_remove(file);
  g_remove(elsewhere);
  g_rmdir(dir);
  g_autofree char *parent = g_build_filename(runtime, "whelk", NULL);
  g_rmdir(parent);
  g_rmdir(runtime);
}

/* The fallback the store-built lock screen needs: the host's setuid helper is
 * found by path and refused unless it really is root's setuid file.  A machine
 * without shadow (a build sandbox) simply has none. */
static void test_local_helper_is_trustworthy(void) {
  g_autofree char *helper = whelk_lock_local_helper();

  if (!helper) {
    g_test_skip("this machine has no unix_chkpwd");
    return;
  }

  struct stat info;

  g_assert_cmpint(stat(helper, &info), ==, 0);
  g_assert_true(S_ISREG(info.st_mode));
  g_assert_cmpuint(info.st_uid, ==, 0);
  g_assert_true((info.st_mode & S_ISUID) != 0);
  g_assert_cmpuint(info.st_mode & (S_IWGRP | S_IWOTH), ==, 0);
}

/* A wrong password must be a refusal, never an acceptance, and a password for
 * another account must not be tried as this one.  unix_chkpwd does not record
 * failures, so this cannot lock the test user out. */
static void test_local_verify_refuses_a_wrong_password(void) {
  if (!whelk_lock_local_helper()) {
    g_test_skip("this machine has no unix_chkpwd");
    return;
  }

  const WhelkLocalResult result =
      whelk_lock_local_verify("whelk-lockauth-not-the-password");

  g_assert_cmpint(result, !=, WHELK_LOCAL_OK);
}

/* The bypass policy: an administrator's service file always wins, a missing
 * helper always loses, and strict mode refuses the helper even when it is the
 * only answer left. */
static void test_helper_fallback_policy(void) {
  g_assert_false(whelk_lock_fallback_allowed(TRUE, TRUE, FALSE));
  g_assert_false(whelk_lock_fallback_allowed(TRUE, TRUE, TRUE));
  g_assert_false(whelk_lock_fallback_allowed(TRUE, FALSE, FALSE));
  g_assert_false(whelk_lock_fallback_allowed(TRUE, FALSE, TRUE));

  g_assert_true(whelk_lock_fallback_allowed(FALSE, TRUE, FALSE));
  g_assert_false(whelk_lock_fallback_allowed(FALSE, TRUE, TRUE));
  g_assert_false(whelk_lock_fallback_allowed(FALSE, FALSE, FALSE));
}

static void test_strict_mode_spellings(void) {
  const char *const on[] = {"1", "true", "TRUE", "yes", "On", NULL};
  const char *const off[] = {"", "0", "false", "off", "no", "strictly", NULL};

  for (guint i = 0; on[i]; i++)
    g_assert_true(whelk_lock_strict_value(on[i]));

  for (guint i = 0; off[i]; i++)
    g_assert_false(whelk_lock_strict_value(off[i]));

  g_assert_false(whelk_lock_strict_value(NULL));
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/lockauth/candidates", test_candidates);
  g_test_add_func("/lockauth/host-library", test_prefers_the_host_library);
  g_test_add_func("/lockauth/confdir",
                  test_confdir_writes_a_minimal_service);
  g_test_add_func("/lockauth/confdir-refuses",
                  test_confdir_refuses_unsafe_paths);
  g_test_add_func("/lockauth/local-helper", test_local_helper_is_trustworthy);
  g_test_add_func("/lockauth/local-verify",
                  test_local_verify_refuses_a_wrong_password);
  g_test_add_func("/lockauth/fallback-policy", test_helper_fallback_policy);
  g_test_add_func("/lockauth/strict-spellings", test_strict_mode_spellings);

  return g_test_run();
}
