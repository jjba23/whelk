// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/command.h"

#include <gio/gio.h>

#include "logger.h"
#include "util/format.h"

/* =========================================================================
 * ASYNCHRONOUS POLLING
 * ========================================================================= */

typedef struct {
  WhelkCommandCallback callback;
  WhelkCommandStatusCallback status_callback;
  gpointer user_data;
} RunContext;

static void run_context_free(RunContext *ctx) { g_free(ctx); }

static void on_communicate(GObject *source, GAsyncResult *result,
                           gpointer user_data) {
  RunContext *ctx = user_data;
  GSubprocess *process = G_SUBPROCESS(source);
  g_autofree char *stdout_text = NULL;
  g_autoptr(GError) error = NULL;

  if (!g_subprocess_communicate_utf8_finish(process, result, &stdout_text,
                                            NULL, &error)) {
    LOG_C_DEBUG("Command", "reading the program's output failed: %s",
                error->message);
    stdout_text = NULL;
  }

  char *plain = g_strdup(stdout_text ? stdout_text : "");
  const int status = g_subprocess_get_if_exited(process)
                         ? g_subprocess_get_exit_status(process)
                         : -1;

  whelk_str_strip_newline(plain);

  if (ctx->status_callback)
    ctx->status_callback(plain, status, ctx->user_data);
  else if (ctx->callback)
    ctx->callback(plain, ctx->user_data);

  g_free(plain);
  run_context_free(ctx);
}

static void run_impl(const char *command_line, const char *input,
                     WhelkCommandCallback callback,
                     WhelkCommandStatusCallback status_callback,
                     gpointer user_data) {
  if (!command_line || *command_line == '\0') {
    if (callback)
      callback(NULL, user_data);
    return;
  }

  g_autoptr(GError) error = NULL;
  const char *argv[] = {"sh", "-c", command_line, NULL};
  const gboolean has_input = input && *input;
  const GSubprocessFlags flags =
      G_SUBPROCESS_FLAGS_STDOUT_PIPE | G_SUBPROCESS_FLAGS_STDERR_SILENCE |
      (has_input ? G_SUBPROCESS_FLAGS_STDIN_PIPE : 0);

  GSubprocess *process = g_subprocess_newv(argv, flags, &error);

  if (!process) {
    LOG_C_DEBUG("Command", "could not start '%s': %s", command_line,
                error->message);
    if (callback)
      callback(NULL, user_data);
    return;
  }

  RunContext *ctx = g_new0(RunContext, 1);
  ctx->callback = callback;
  ctx->status_callback = status_callback;
  ctx->user_data = user_data;

  g_subprocess_communicate_utf8_async(process, has_input ? input : NULL, NULL,
                                      on_communicate, ctx);
  g_object_unref(process);
}

void whelk_command_run(const char *command_line, WhelkCommandCallback callback,
                       gpointer user_data) {
  run_impl(command_line, NULL, callback, NULL, user_data);
}

void whelk_command_run_input(const char *command_line, const char *input,
                             WhelkCommandCallback callback, gpointer user_data) {
  run_impl(command_line, input, callback, NULL, user_data);
}

void whelk_command_run_input_status(const char *command_line,
                                    const char *input,
                                    WhelkCommandStatusCallback callback,
                                    gpointer user_data) {
  run_impl(command_line, input, NULL, callback, user_data);
}

/* =========================================================================
 * FIRE AND FORGET
 * ========================================================================= */

void whelk_command_spawn(const char *command_line) {
  if (!command_line || *command_line == '\0')
    return;

  g_autoptr(GError) error = NULL;
  const char *argv[] = {"sh", "-c", command_line, NULL};

  if (!g_spawn_async(NULL, (char **)argv, NULL,
                     G_SPAWN_SEARCH_PATH | G_SPAWN_STDOUT_TO_DEV_NULL |
                         G_SPAWN_STDERR_TO_DEV_NULL,
                     NULL, NULL, NULL, &error)) {
    LOG_C_WARN("Command", "could not launch '%s': %s", command_line,
               error->message);
  }
}
