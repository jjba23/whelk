// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/command.h"

#include <glib.h>

/* A program's question, answered on its standard input.  The Wi-Fi panel's key
 * travels this way so that `ps` never shows it. */

typedef struct {
  GMainLoop *loop;
  char *output;
} Capture;

static void on_done(const char *text, gpointer user_data) {
  Capture *capture = user_data;

  capture->output = g_strdup(text);
  g_main_loop_quit(capture->loop);
}

static char *run_and_wait(const char *command, const char *input) {
  Capture capture = {g_main_loop_new(NULL, FALSE), NULL};

  whelk_command_run_input(command, input, on_done, &capture);
  g_main_loop_run(capture.loop);

  g_main_loop_unref(capture.loop);
  return capture.output;
}

static void test_input_reaches_the_program(void) {
  g_autofree char *out = run_and_wait("tr a-z A-Z", "correct horse\n");

  /* tr copies stdin to stdout: the answer arrived, and the newline was made
   * into the line a prompt expects. */
  g_assert_cmpstr(out, ==, "CORRECT HORSE");
}

static void test_empty_input_closes_stdin(void) {
  /* Without a key the program still gets an end of file, not a hang. */
  g_autofree char *out = run_and_wait("wc -c", NULL);

  g_assert_cmpstr(out, ==, "0");
}

typedef struct {
  GMainLoop *loop;
  char *output;
  int status;
} StatusCapture;

static void on_done_status(const char *text, int status, gpointer user_data) {
  StatusCapture *capture = user_data;

  capture->output = g_strdup(text);
  capture->status = status;
  g_main_loop_quit(capture->loop);
}

static void test_status_reports_failure(void) {
  StatusCapture capture = {g_main_loop_new(NULL, FALSE), NULL, 0};

  whelk_command_run_input_status("cat >/dev/null; exit 3", "ignored",
                                 on_done_status, &capture);
  g_main_loop_run(capture.loop);

  /* The status is the answer a translated message cannot give: this is what
   * tells the Wi-Fi panel that saving the key failed. */
  g_assert_cmpint(capture.status, ==, 3);
  g_assert_cmpstr(capture.output, ==, "");

  g_free(capture.output);
  g_main_loop_unref(capture.loop);
}

static void test_status_reports_success(void) {
  StatusCapture capture = {g_main_loop_new(NULL, FALSE), NULL, -1};

  whelk_command_run_input_status("cat", "ok\n", on_done_status, &capture);
  g_main_loop_run(capture.loop);

  g_assert_cmpint(capture.status, ==, 0);
  g_assert_cmpstr(capture.output, ==, "ok");

  g_free(capture.output);
  g_main_loop_unref(capture.loop);
}

static void test_plain_run_still_works(void) {
  g_autofree char *out = run_and_wait("printf 'still here'", NULL);

  g_assert_cmpstr(out, ==, "still here");
}

int main(int argc, char *argv[]) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/command/input", test_input_reaches_the_program);
  g_test_add_func("/command/empty-input", test_empty_input_closes_stdin);
  g_test_add_func("/command/plain", test_plain_run_still_works);
  g_test_add_func("/command/status-failure", test_status_reports_failure);
  g_test_add_func("/command/status-success", test_status_reports_success);

  return g_test_run();
}
