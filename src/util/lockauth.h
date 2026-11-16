// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_LOCKAUTH_H
#define WHELK_LOCKAUTH_H

#include <glib.h>
#include <security/pam_appl.h>

G_BEGIN_DECLS

/* =========================================================================
 * PAM AT RUN TIME
 * =========================================================================
 * The lock screen does not link a PAM library.  PAM is only portable when the
 * library that answers is the one the running system installed: it owns the
 * module directory (`/usr/lib64/security', `/usr/lib/x86_64-linux-gnu/security'
 * and their relatives) and it knows the helper those modules exec.  A libpam
 * from a package manager's private store (Nix, Guix) has its own module
 * directory, and on a distribution that keeps PAM elsewhere its `pam_unix'
 * cannot run the host's setuid `unix_chkpwd' at all: every password comes back
 * as PAM_AUTHINFO_UNAVAIL, which is what the lock screen reported as
 * "Authentication is unavailable".
 *
 * So whelk asks for the host's library first and the linked soname last.  On a
 * NixOS or Guix System, where the host's PAM lives in the store, the last
 * candidate resolves through the build's rpath to that store, which is the
 * correct library there.
 */

/* Returns a newly allocated, NULL-terminated list of library paths to try, in
 * order: the host's libpam, then the bare soname the build resolves.  Free it
 * with g_strfreev().  Never NULL. */
char **whelk_lock_pam_library_candidates(void);

/* The entry points whelk calls.  They all come from one library, so a handle
 * never crosses libraries. */
typedef struct {
  void *handle;
  int (*start)(const char *service, const char *user,
               const struct pam_conv *conversation, pam_handle_t **handle_out);
  int (*start_confdir)(const char *service, const char *user,
                       const struct pam_conv *conversation,
                       const char *confdir, pam_handle_t **handle_out);
  int (*end)(pam_handle_t *handle, int status);
  int (*authenticate)(pam_handle_t *handle, int flags);
  int (*acct_mgmt)(pam_handle_t *handle, int flags);
  const char *(*strerror)(pam_handle_t *handle, int status);
} WhelkPam;

/* Loads the first candidate that provides the entry points above (the confdir
 * start is optional: it arrived in Linux-PAM 1.4) and returns it, or NULL when
 * the machine has no PAM library at all.  Idempotent; the pointer is
 * process-wide and stays valid for the life of the process. */
const WhelkPam *whelk_lock_pam_open(void);

/* The path whelk_lock_pam_open() loaded, or NULL.  For the log line and the
 * test that pins the host-first order. */
const char *whelk_lock_pam_path(void);

/* Writes a minimal per-user PAM service to
 * `<runtime_dir>/whelk/pam.d/whelk-lock` and returns that directory, or NULL.
 *
 * It exists for a machine with no session stack to borrow: this host, for
 * example, has `/etc/pam.d/common-auth` but no `login`, no `xdm` and no
 * `other`, so PAM's own fallback denies every password.  The service is
 * `pam_unix' for both the auth and the account check, which covers local
 * accounts through the host's `unix_chkpwd'.  It is only a fallback: a
 * `whelk-lock` or `login` service file is always preferred.
 *
 * `runtime_dir` must be an absolute path - normally `$XDG_RUNTIME_DIR`.  A
 * relative path or a missing one returns NULL instead of falling back to
 * `/tmp': a PAM service another user could write is worse than a locked screen.
 * A symlink in the way is refused too. */
char *whelk_lock_pam_confdir(const char *runtime_dir);

/* =========================================================================
 * THE HOST'S PASSWORD HELPER
 * =========================================================================
 * A store-built whelk (Nix, Guix) links its own C library, so it cannot load
 * the host's libpam: that library needs the host's libaudit and libcrypt,
 * which are not in the store's search path.  The store's libpam loads, but its
 * pam_unix cannot exec the host's setuid `unix_chkpwd', so every password
 * returns PAM_AUTHINFO_UNAVAIL -- the lock screen's "Authentication is
 * unavailable".
 *
 * The host does install one helper that answers the local password question
 * from any process: `unix_chkpwd', setuid root and owned by shadow.  It is
 * what the host's own pam_unix execs, so asking it directly uses the same
 * credential check without loading a foreign libpam.  It is only consulted
 * after PAM has failed in a way that means "this library cannot reach the
 * store", and only when the stack that refused is whelk's own pam_unix
 * service: an administrator's `whelk-lock' or `login' policy is never
 * bypassed.
 */

typedef enum {
  WHELK_LOCAL_OK,
  WHELK_LOCAL_DENIED,
  WHELK_LOCAL_UNAVAILABLE,
} WhelkLocalResult;

/* The host's setuid password helper, or NULL when this machine has none
 * (a build sandbox, a container without shadow).  The path is only returned
 * when the file is a regular file owned by root with the setuid bit set and
 * no group or world write permission.  Free with g_free(). */
char *whelk_lock_local_helper(void);

/* Verifies `password' for the session user through that helper.  The password
 * is written to the helper's standard input and wiped from whelk's copy
 * immediately afterwards; it is never placed on a command line.  Returns
 * WHELK_LOCAL_UNAVAILABLE when no trustworthy helper exists, so a lock screen
 * that cannot verify stays locked. */
WhelkLocalResult whelk_lock_local_verify(const char *password);

/* TRUE when the library whelk loaded is a package manager's copy (a Nix or
 * Guix store path) *and* the host has its own libpam.  That combination means
 * the library cannot see the host's configuration or module directory, so
 * every answer it gives says more about the build than about the password:
 * `pam_start("login")' reads the store's `etc/pam.d', which has no such
 * service.  The locker treats an "unavailable" answer from that combination
 * as the build's fault and verifies through the host's helper instead. */
gboolean whelk_lock_pam_missing_host_policy(void);

G_END_DECLS

#endif // WHELK_LOCKAUTH_H
