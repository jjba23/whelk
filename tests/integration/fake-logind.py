#!/usr/bin/env python3
# whelk -- fake logind sender for the private-bus integration test.
#
# It owns org.freedesktop.login1 on the test bus and emits the same Manager
# and Session signals whelk subscribes to.  It is not a substitute for a real
# logind test; it makes the D-Bus matching and lock transition repeatable.

import os
import sys

import gi
gi.require_version('Gio', '2.0')
from gi.repository import Gio, GLib

NAME = 'org.freedesktop.login1'
MANAGER_PATH = '/org/freedesktop/login1'
SESSION_PATH = '/org/freedesktop/login1/session/test'
TRIGGER = os.environ.get('WHELK_FAKE_LOGIND_TRIGGER', '/tmp/whelk-fake-logind')

conn = Gio.bus_get_sync(Gio.BusType.SYSTEM, None)

MANAGER_XML = '''
<node>
  <interface name="org.freedesktop.login1.Manager">
    <method name="GetSessionByPID">
      <arg type="u" direction="in"/>
      <arg type="o" direction="out"/>
    </method>
  </interface>
</node>
'''


def on_manager_call(connection, sender, path, interface, method, parameters,
                    invocation):
    if method == 'GetSessionByPID':
        invocation.return_value(GLib.Variant('(o)', (SESSION_PATH,)))
    else:
        invocation.return_error_literal(
            Gio.DBusError.UNKNOWN_METHOD, 'unknown method %s' % method)


node = Gio.DBusNodeInfo.new_for_xml(MANAGER_XML)
conn.register_object(MANAGER_PATH, node.interfaces[0], on_manager_call,
                     None, None)

reply = conn.call_sync(
    'org.freedesktop.DBus', '/org/freedesktop/DBus',
    'org.freedesktop.DBus', 'RequestName',
    GLib.Variant('(su)', (NAME, 4)),
    GLib.VariantType.new('(u)'), Gio.DBusCallFlags.NONE, -1, None)
print('request_name_reply=%s' % reply.unpack()[0], flush=True)


def emit(kind):
    if kind == 'session-lock':
        conn.emit_signal(None, SESSION_PATH,
                         'org.freedesktop.login1.Session', 'Lock', None)
    elif kind == 'prepare-sleep':
        conn.emit_signal(None, MANAGER_PATH,
                         'org.freedesktop.login1.Manager',
                         'PrepareForSleep', GLib.Variant('(b)', (True,)))
    elif kind == 'session-inactive':
        conn.emit_signal(
            None, SESSION_PATH, 'org.freedesktop.login1.Session',
            'PropertiesChanged',
            GLib.Variant('(sa{sv}as)',
                         ('org.freedesktop.login1.Session',
                          {'Active': GLib.Variant('b', False)}, [])))
    elif kind == 'idle-hint':
        conn.emit_signal(
            None, SESSION_PATH, 'org.freedesktop.login1.Session',
            'PropertiesChanged',
            GLib.Variant('(sa{sv}as)',
                         ('org.freedesktop.login1.Session',
                          {'IdleHint': GLib.Variant('b', True)}, [])))
    else:
        print('unknown kind: %s' % kind, flush=True)
        return

    print('emitted %s' % kind, flush=True)


def check():
    if os.path.exists(TRIGGER):
        try:
            with open(TRIGGER, 'r', encoding='utf-8') as stream:
                kind = stream.read().strip()
            os.unlink(TRIGGER)
            emit(kind)
        except OSError as error:
            print('trigger error: %r' % error, flush=True)
    return True


GLib.timeout_add(100, check)
print('ready session_path=%s' % SESSION_PATH, flush=True)
GLib.MainLoop().run()
