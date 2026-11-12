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
Version:        0.1.0
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
BuildRequires:  pkgconfig(wayland-client)
Requires:       gtk4
Requires:       guile
Requires:       hicolor-icon-theme

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
%dir %{_datadir}/guile
%dir %{_datadir}/guile/site
%dir %{_datadir}/guile/site/3.0
%{_datadir}/guile/site/3.0/whelk

%changelog
* Thu Nov 12 2026 Josep Bigorra <jjbigorra@gmail.com> - 0.1.0
- Initial release: bars on any edge, the launcher, the notification daemon
  with its centre and toasts, and the Wi-Fi and Bluetooth panel
