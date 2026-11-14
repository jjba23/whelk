#!/usr/bin/env python3
# whelk -- AT-SPI helper for the headless locker integration test.
#
# This is deliberately outside the normal Meson suite: it needs a live
# accessibility bus and a locker process.  It never prints the password.

import os
import sys
import time

import gi
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi

Atspi.init()
desktop = Atspi.get_desktop(0)


def walk(node, out):
    out.append(node)
    for i in range(node.get_child_count()):
        child = node.get_child_at_index(i)
        if child:
            walk(child, out)


TEST_APP_NAME = os.environ.get('WHELK_TEST_APP_NAME', '').lower()


def nodes():
    for i in range(desktop.get_child_count()):
        app = desktop.get_child_at_index(i)
        if not app:
            continue
        name = (app.get_name() or '').lower()
        if TEST_APP_NAME:
            # g_set_application_name() is not always what AT-SPI reports;
            # accept the real name too.  The test runs on a private bus, so
            # there is no unrelated whelk application to match here.
            if name not in (TEST_APP_NAME, 'whelk'):
                continue
        elif name != 'whelk':
            continue
        out = []
        walk(app, out)
        return out
    return []


def find(role, name=None):
    for node in nodes():
        if node.get_role_name() != role:
            continue
        if name is not None and (node.get_name() or '') != name:
            continue
        return node
    return None


def labels():
    return [node.get_name() or '' for node in nodes()
            if node.get_role_name() == 'label']


def lock_frames():
    return [node for node in nodes()
            if node.get_role_name() == 'frame' and
            (node.get_name() or '') == 'whelk lock']


def entry_count():
    return sum(1 for node in nodes() if node.get_role_name() == 'password text')


def wait_for(predicate, timeout=30.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.1)
    return False


def print_state():
    print('lock_frames=%d entries=%d labels=%r' %
          (len(lock_frames()), entry_count(), labels()), flush=True)


def main():
    command = sys.argv[1] if len(sys.argv) > 1 else 'state'

    if command == 'state':
        print_state()
        return 0

    if command == 'wait-lock':
        timeout = float(sys.argv[2]) if len(sys.argv) > 2 else 30.0
        if not wait_for(lambda: lock_frames(), timeout):
            print_state()
            return 1
        print_state()
        return 0

    if command == 'wait-unlocked':
        timeout = float(sys.argv[2]) if len(sys.argv) > 2 else 30.0
        if not wait_for(lambda: not lock_frames(), timeout):
            print_state()
            return 1
        print_state()
        return 0

    if command == 'wait-entry':
        timeout = float(sys.argv[2]) if len(sys.argv) > 2 else 30.0
        if not wait_for(lambda: entry_count() == 1, timeout):
            print_state()
            return 1
        print_state()
        return 0

    if command == 'submit':
        if len(sys.argv) < 3:
            print('submit needs a value', file=sys.stderr)
            return 2
        entry = find('password text')
        button = find('button', 'Unlock')
        if not entry or not button:
            print_state()
            return 1
        if not entry.set_text_contents(sys.argv[2]):
            print('could not set password entry', file=sys.stderr)
            return 1
        if not button.do_action(0):
            print('could not activate Unlock', file=sys.stderr)
            return 1
        print_state()
        return 0

    if command == 'wait-error':
        timeout = float(sys.argv[2]) if len(sys.argv) > 2 else 30.0
        if not wait_for(lambda: any(text in labels() for text in
                                    ('Incorrect password.',
                                     'Authentication is unavailable.',
                                     'Try again.')),
                        timeout):
            print_state()
            return 1
        print_state()
        return 0

    print('unknown command: %s' % command, file=sys.stderr)
    return 2


if __name__ == '__main__':
    sys.exit(main())
