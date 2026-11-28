// bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT

fn main() {
    // Guile is a C library with no stable Rust crate that tracks this project's
    // ABI.  pkg-config supplies the include and linker flags; the declarations
    // in src/guile/sys.rs are the small, reviewed boundary we maintain.
    pkg_config::Config::new()
        .atleast_version("3.0")
        .probe("guile-3.0")
        .expect("bibelot needs GNU Guile 3.0 development files (guile-3.0.pc)");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=BIBELOT_SCHEME_DIR");
}
