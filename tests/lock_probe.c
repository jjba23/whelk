// whelk -- a probe that asks the compositor for ext-session-lock-v1.
//
// The integration harness uses this to distinguish "the session is still
// locked" from "the locker process is gone": a successful lock request means
// no lock is currently held.  It is a test helper, never installed.

#include <gtk/gtk.h>
#include <gtk4-session-lock.h>

int main(void) {
  gtk_init();

  if (!gtk_session_lock_is_supported())
    return 2;

  GtkApplication *app =
      gtk_application_new("org.jointhefreeworld.whelk.lock-probe",
                          G_APPLICATION_NON_UNIQUE);
  g_application_register(G_APPLICATION(app), NULL, NULL);

  GtkSessionLockInstance *lock = gtk_session_lock_instance_new();
  const gboolean acquired = gtk_session_lock_instance_lock(lock);

  if (acquired)
    gtk_session_lock_instance_unlock(lock);

  g_object_unref(lock);
  g_object_unref(app);

  return acquired ? 0 : 1;
}
