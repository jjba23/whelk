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

static void test_plain_run_still_works(void) {
  g_autofree char *out = run_and_wait("printf 'still here'", NULL);

  g_assert_cmpstr(out, ==, "still here");
}

int main(int argc, char *argv[]) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/command/input", test_input_reaches_the_program);
  g_test_add_func("/command/empty-input", test_empty_input_closes_stdin);
  g_test_add_func("/command/plain", test_plain_run_still_works);

  return g_test_run();
}
