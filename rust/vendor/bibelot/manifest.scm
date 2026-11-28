;; bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT

(use-modules (guix packages)
             (gnu)
             (gnu packages)
             (gnu packages glib)
             (gnu packages gtk)
             (gnu packages guile)
             (gnu packages pkg-config)
             (gnu packages rust))

(packages->manifest
 (list guile-3.0
       (list rust "out")
       (list rust "cargo")
       (list rust "tools")
       (list rust "rust-src")
       pkg-config
       wayland
       gtk
       gtk4-layer-shell
       xz
       adwaita-icon-theme
       coreutils
       procps
       util-linux))
