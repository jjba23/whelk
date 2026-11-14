#
# spec file for package whelk
#
# Copyright (c) 2026 SUSE LLC and contributors
#
# All modifications and additions to the file contributed by third parties
# remain the property of their copyright owners, unless otherwise agreed
# upon. The license for this file, and modifications and additions to the
# file, is the same license as for the pristine package itself (unless the
# license for the pristine package is not an Open Source License, in which
# case the license is the MIT License). An "Open Source License" is a
# license that conforms to the Open Source Definition (Version 1.9)
# published by the Open Source Initiative.

# Please submit bugfixes or comments via http://bugs.opensuse.org/
#

Name:           whelk
Version:        0.20.0
Release:        0
Summary:        Wayland shell with bars, a launcher and notifications, in Guile Scheme
License:        MIT
URL:            https://github.com/jjba23/whelk
Source0:        %{name}-%{version}.tar.gz
BuildRequires:  gcc
BuildRequires:  meson
BuildRequires:  ninja
BuildRequires:  pkgconfig
BuildRequires:  pkgconfig(glib-2.0)
BuildRequires:  pkgconfig(gio-unix-2.0)
BuildRequires:  pkgconfig(gtk4)
BuildRequires:  pkgconfig(gtk4-layer-shell-0)
BuildRequires:  pkgconfig(guile-3.0)
BuildRequires:  pkgconfig(pam)
BuildRequires:  pkgconfig(wayland-client)
Requires:       gtk4
Requires:       guile
Requires:       pam
Requires:       hicolor-icon-theme
Requires:       wtype

%description
Whelk is a Wayland shell built with a C core, GTK4 and gtk4-layer-shell.
Its status bars, launcher, notification daemon and network panel are
described in GNU Guile Scheme: a bar is a list of modules, and every
module is a reading plus the mouse actions on it.

Whelk is designed for Wayland compositors implementing the
wlr-layer-shell protocol, including niri, Sway, Hyprland, River and
Wayfire.

%prep
%autosetup

%build
%meson
%meson_build

%install
%meson_install

%check
%meson_test

%files
%license LICENSE
%doc README.org
%{_bindir}/whelk
%{_datadir}/applications/org.jointhefreeworld.whelk.desktop
%{_datadir}/icons/hicolor/*/apps/org.jointhefreeworld.whelk.*
%dir %{_datadir}/guile
%dir %{_datadir}/guile/site
%dir %{_datadir}/guile/site/3.0
%{_datadir}/guile/site/3.0/whelk

%changelog
* Sat Nov 14 2026 Josep Bigorra <jjbigorra@gmail.com> - 0.20.0
- The screen locker asks the compositor for an ext-session-lock-v1 session
  lock; the layer-shell overlay is only the fallback for compositors without
  the protocol.  AT-SPI menus follow live application changes.

* Sat Nov 14 2026 Josep Bigorra <jjbigorra@gmail.com> - 0.19.1
- The bar's battery indicator finds BAT0, BAT1 and any other pack instead of
  only BAT0; buttons use the cursor theme's pointer cursor.

* Sat Nov 14 2026 Josep Bigorra <jjbigorra@gmail.com> - 0.19.0
- AT-SPI menus, Scheme modules, per-monitor bars, golden-image tests and a
  fail-closed PAM screen locker.  Desktop widgets are visible again.

* Thu Nov 12 2026 Josep Bigorra <jjbigorra@gmail.com> - 0.3.3
- The X11 dock hints are exercised on a real X server: struts match the bar.

* Thu Nov 12 2026 Josep Bigorra <jjbigorra@gmail.com> - 0.3.2
- Session detection probes niri and sway when NIRI_SOCKET/SWAYSOCK are absent.

* Thu Nov 12 2026 Josep Bigorra <jjbigorra@gmail.com> - 0.3.1
- The Nix expression runs the Guile theme test too, and ignores build dirs.

* Thu Nov 12 2026 Josep Bigorra <jjbigorra@gmail.com> - 0.3.0
- A session layer: niri, sway and X11 focus, menus and closing, and graceful
  degradation where layer-shell is missing.
- An X11 dock fallback with EWMH struts.

* Thu Nov 12 2026 Josep Bigorra <jjbigorra@gmail.com> - 0.2.0
- A macOS-shaped bar: the shell's own menu and a D-Bus global menu
- Brand-free bar themes
- The launcher searches all of $HOME, pruned and ranked
- Vertical bars reserve the width they paint
- Initial release: bars on any edge, the launcher, the notification daemon
  with its centre and toasts, and the Wi-Fi and Bluetooth panel
