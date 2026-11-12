// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_COMMAND_H
#define WHELK_COMMAND_H

#include <glib.h>

/* =========================================================================
 * RUNNING PROGRAMS
 * =========================================================================
 * A bar polls programs (wpctl, niri, playerctl) and launches them (a launcher,
 * a mixer, a screenshot tool).  Both go through /bin/sh so a configuration can
 * write a pipeline, and both are asynchronous: a compositor whose bar freezes
 * while `playerctl` waits on a stopped player is not a compositor any more.
 */

/**
 * WhelkCommandCallback:
 * @stdout_text: (nullable): the program's standard output, trailing newline
 *   removed.  NULL when the program could not be started.
 * @user_data: the pointer given to whelk_command_run().
 */
typedef void (*WhelkCommandCallback)(const char *stdout_text,
                                     gpointer user_data);

/**
 * whelk_command_run:
 * @command_line: a shell command line.
 * @callback: (nullable) (scope notified): called on the main loop when the
 *   program has finished.
 * @user_data: passed to @callback; freed by the caller if @callback is NULL.
 *
 * Runs @command_line asynchronously.  Standard error is discarded, because a
 * missing optional tool is a normal answer and not something to paint on the
 * console every two seconds.
 */
void whelk_command_run(const char *command_line, WhelkCommandCallback callback,
                       gpointer user_data);

/**
 * whelk_command_run_input:
 * @command_line: a shell command line.
 * @input: (nullable): text written to the program's standard input, then
 *   closed.  NULL or "" leaves the program with an empty stdin.
 * @callback: (nullable) (scope notified): called on the main loop when the
 *   program has finished.
 * @user_data: passed to @callback.
 *
 * The same as whelk_command_run(), for a program that asks its question on
 * standard input: a secret goes here and never through the command line, where
 * `ps` would publish it for as long as the program runs.
 */
void whelk_command_run_input(const char *command_line, const char *input,
                             WhelkCommandCallback callback, gpointer user_data);

/**
 * WhelkCommandStatusCallback:
 * @stdout_text: (nullable): the program's standard output, trailing newline
 *   removed.  NULL when the program could not be started.
 * @status: the program's exit status, or -1 when it did not exit normally.
 * @user_data: the pointer given to whelk_command_run_input_status().
 */
typedef void (*WhelkCommandStatusCallback)(const char *stdout_text, int status,
                                           gpointer user_data);

/**
 * whelk_command_run_input_status:
 * @command_line: a shell command line.
 * @input: (nullable): text written to the program's standard input.
 * @callback: (nullable) (scope notified): called on the main loop when the
 *   program has finished.
 * @user_data: passed to @callback.
 *
 * whelk_command_run_input(), with the exit status: a program's *answer* is not
 * always a sentence to show, and its words are in whatever language the
 * machine speaks.  "Did it fail?" is the status, and it needs no translating.
 */
void whelk_command_run_input_status(const char *command_line,
                                    const char *input,
                                    WhelkCommandStatusCallback callback,
                                    gpointer user_data);

/**
 * whelk_command_spawn:
 * @command_line: a shell command line.
 *
 * Starts @command_line detached, with its output going nowhere: the click
 * handlers' half of the module.  A failure is logged and otherwise ignored.
 */
void whelk_command_spawn(const char *command_line);

#endif // WHELK_COMMAND_H
