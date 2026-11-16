// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The lock screen's authentication boundary, without a display or a password.

use std::os::unix::fs::{MetadataExt, PermissionsExt};

use whelk_rs::lockauth;

#[test]
fn candidates_are_absolute_unique_and_host_first() {
    let paths = lockauth::candidates();

    assert!(paths.len() >= 2, "the list always names a fallback");
    assert_eq!(paths[0], "/usr/lib64/libpam.so.0");
    assert_eq!(paths.last().map(String::as_str), Some("libpam.so.0"));

    for (index, path) in paths.iter().enumerate() {
        if path.as_str() == "libpam.so.0" {
            assert_eq!(index, paths.len() - 1, "the soname is the last resort");

            continue;
        }

        assert!(path.starts_with('/'), "{path} is not absolute");
        assert!(path.ends_with("libpam.so.0"), "{path} is not libpam");
        assert!(!paths[index + 1..].contains(path), "{path} repeats");
    }
}

#[test]
fn the_per_user_service_is_private_and_reusable() {
    let runtime = tempfile::tempdir().expect("a temporary runtime directory");
    let directory = lockauth::confdir(runtime.path()).expect("a writable runtime directory");

    assert_eq!(directory, runtime.path().join("whelk").join("pam.d"));

    let mode = std::fs::metadata(&directory)
        .expect("the service directory")
        .permissions()
        .mode()
        & 0o777;

    assert_eq!(mode, 0o700);

    let file = directory.join("whelk-lock");
    let contents = std::fs::read_to_string(&file).expect("the service file");

    assert!(contents.contains("auth required pam_unix.so"));
    assert!(contents.contains("account required pam_unix.so"));

    let mode = std::fs::metadata(&file)
        .expect("the service file")
        .permissions()
        .mode()
        & 0o777;

    assert_eq!(mode, 0o600);

    // A second lock in the same session rewrites the same file, not another.
    assert_eq!(lockauth::confdir(runtime.path()), Some(directory));
}

#[test]
fn an_unsafe_runtime_directory_is_refused() {
    assert_eq!(lockauth::confdir(std::path::Path::new("")), None);
    assert_eq!(
        lockauth::confdir(std::path::Path::new("run/user/1000")),
        None
    );

    let runtime = tempfile::tempdir().expect("a temporary runtime directory");
    let directory = runtime.path().join("whelk").join("pam.d");

    std::fs::create_dir_all(&directory).expect("the service directory");

    let elsewhere = runtime.path().join("elsewhere");

    std::fs::write(&elsewhere, "auth required pam_deny.so\n").expect("the decoy stack");

    std::os::unix::fs::symlink(&elsewhere, directory.join("whelk-lock"))
        .expect("a symlink in the way");

    assert_eq!(lockauth::confdir(runtime.path()), None);
}

#[test]
fn the_local_helper_is_setuid_root_or_absent() {
    let Some(helper) = lockauth::local_helper() else {
        // A build sandbox or a container without shadow simply has none.
        return;
    };

    let metadata = std::fs::metadata(&helper).expect("the helper");

    assert!(metadata.is_file());
    assert_eq!(metadata.uid(), 0);
    assert!(metadata.mode() & 0o4000 != 0, "{helper} is not setuid");
    assert_eq!(
        metadata.mode() & 0o022,
        0,
        "{helper} is group/world writable"
    );
}

#[test]
fn the_session_user_is_the_process_owner() {
    let user = lockauth::session_user().expect("the process's own account");

    assert!(!user.is_empty());
    assert!(!user.contains(' '));
}

#[test]
fn a_host_library_is_never_reported_as_missing_the_host_policy() {
    let Some(path) = lockauth::pam_path() else {
        return;
    };

    let store_built = path.starts_with("/nix/store/") || path.starts_with("/gnu/store/");

    if !store_built {
        assert!(!lockauth::missing_host_policy());
    } else if lockauth::missing_host_policy() {
        assert!(
            [
                "/usr/lib64/libpam.so.0",
                "/lib64/libpam.so.0",
                "/usr/lib/libpam.so.0",
                "/lib/libpam.so.0",
            ]
            .iter()
            .any(|candidate| std::path::Path::new(candidate).is_file()),
            "a store copy only misses a host policy that exists"
        );
    }
}
