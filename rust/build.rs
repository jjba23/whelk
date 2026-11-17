// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

fn main() {
    // Guile is a C library with no stable Rust crate that tracks this project's
    // ABI.  pkg-config supplies the include and linker flags; the declarations
    // in src/guile/sys.rs are the small, reviewed boundary we maintain.
    pkg_config::Config::new()
        .atleast_version("3.0")
        .probe("guile-3.0")
        .expect("whelk needs GNU Guile 3.0 development files (guile-3.0.pc)");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=WHELK_SCHEME_DIR");

    // The lock screen loads PAM at run time.  The path the build linked against
    // is a candidate after the host's own library: a Nix or Guix build can have
    // an rpath nothing links against shrunk away, and then only the bare soname
    // is left to find a store copy.  Optional, because a machine may have no PAM
    // development files and the lock screen must still build.
    if let Ok(pam) = pkg_config::Config::new()
        .atleast_version("1.4")
        .probe("pam")
        && let Some(libdir) = pam.link_paths.first()
    {
        println!("cargo:rustc-env=WHELK_PAM_LIBDIR={}", libdir.display());
    }
}
