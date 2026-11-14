// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/hostinfo.h"

#include <glib.h>

/* The About window's readings, and the two cases /proc makes awkward: the
 * per-core index line that is not a model, and a memory total with no
 * MemAvailable beside it. */

static void test_cpu_model_skips_the_core_index(void) {
  const char *cpuinfo = "processor : 0\n"
                        "vendor_id : AuthenticAMD\n"
                        "model name : AMD Ryzen 7 7840U w/ Radeon 780M\n"
                        "processor : 1\n"
                        "model name : AMD Ryzen 7 7840U w/ Radeon 780M\n";
  g_autofree char *model = whelk_host_cpu_model_from(cpuinfo);

  g_assert_cmpstr(model, ==, "AMD Ryzen 7 7840U w/ Radeon 780M");
}

static void test_cpu_model_reads_arm_processor(void) {
  const char *cpuinfo = "processor : 0\n"
                        "Processor : ARMv8 Processor rev 1 (v8l)\n";
  g_autofree char *model = whelk_host_cpu_model_from(cpuinfo);

  g_assert_cmpstr(model, ==, "ARMv8 Processor rev 1 (v8l)");
}

static void test_cpu_model_without_a_model(void) {
  g_assert_null(whelk_host_cpu_model_from("processor : 0\nprocessor : 1\n"));
  g_assert_null(whelk_host_cpu_model_from(NULL));
}

static void test_memory_total_and_available(void) {
  const char *meminfo = "MemTotal:       32207892 kB\n"
                        "MemFree:         1234567 kB\n"
                        "MemAvailable:   22000000 kB\n";
  g_autofree char *summary = whelk_host_memory_summary_from(meminfo);

  g_assert_cmpstr(summary, ==, "30.7 GiB total · 21.0 GiB available");
}

static void test_memory_without_available(void) {
  const char *meminfo = "MemTotal:       1048576 kB\n";
  g_autofree char *summary = whelk_host_memory_summary_from(meminfo);

  g_assert_cmpstr(summary, ==, "1.0 GiB");
}

static void test_uptime_leaves_zero_parts_out(void) {
  g_autofree char *minutes = whelk_host_uptime_from("2640.00 0.00");
  g_autofree char *hours = whelk_host_uptime_from("3600.00 0.00");
  g_autofree char *day = whelk_host_uptime_from("90000.00 0.00");
  g_autofree char *one_minute = whelk_host_uptime_from("60.00 0.00");

  g_assert_cmpstr(minutes, ==, "44 minutes");
  g_assert_cmpstr(hours, ==, "1 hour");
  g_assert_cmpstr(day, ==, "1 day, 1 hour");
  g_assert_cmpstr(one_minute, ==, "1 minute");
  g_assert_null(whelk_host_uptime_from(NULL));
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/whelk/hostinfo/cpu_model_skips_the_core_index",
                  test_cpu_model_skips_the_core_index);
  g_test_add_func("/whelk/hostinfo/cpu_model_reads_arm_processor",
                  test_cpu_model_reads_arm_processor);
  g_test_add_func("/whelk/hostinfo/cpu_model_without_a_model",
                  test_cpu_model_without_a_model);
  g_test_add_func("/whelk/hostinfo/memory_total_and_available",
                  test_memory_total_and_available);
  g_test_add_func("/whelk/hostinfo/memory_without_available",
                  test_memory_without_available);
  g_test_add_func("/whelk/hostinfo/uptime_leaves_zero_parts_out",
                  test_uptime_leaves_zero_parts_out);

  return g_test_run();
}
