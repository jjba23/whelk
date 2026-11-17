// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Authenticating the session user, ported from `util/lockauth.c` and the
//! authentication half of `locker.c`.
//!
//! PAM is not linked.  The lock screen loads the running system's `libpam` at
//! run time, because the library that answers owns the module directory and
//! knows the setuid helper those modules exec.  The declarations below are the
//! whole ABI boundary; they are reviewed against `security/pam_appl.h`.
//!
//! A store-built whelk (Nix, Guix) links its own C library, so the host's
//! `libpam` cannot be loaded into it -- that library needs the host's
//! `libaudit` and `libcrypt`, which are not on the store's search path.  The
//! store's own `libpam` loads, but its `pam_unix` execs a `unix_chkpwd` from
//! the same store, which is not setuid, so every password comes back as
//! `PAM_AUTHINFO_UNAVAIL`: the lock screen's "Authentication is unavailable".
//!
//! When the refusing stack is whelk's own `pam_unix` service -- never an
//! administrator's `whelk-lock` or `login` policy -- the host's setuid
//! `unix_chkpwd` is asked directly.  That is the same credential check
//! `pam_unix` makes, through the one helper every distribution installs for
//! exactly this purpose, so the fallback adds no dependency and no setuid
//! binary of its own.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// security/pam_appl.h
// ---------------------------------------------------------------------------

const PAM_SUCCESS: c_int = 0;
const PAM_AUTH_ERR: c_int = 7;
const PAM_AUTHINFO_UNAVAIL: c_int = 9;
const PAM_USER_UNKNOWN: c_int = 10;
const PAM_MAXTRIES: c_int = 11;
const PAM_NEW_AUTHTOK_REQD: c_int = 12;
const PAM_CONV_ERR: c_int = 19;

const PAM_SILENT: c_int = 0x8000;
const PAM_DISALLOW_NULL_AUTHTOK: c_int = 0x0001;

const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_ERROR_MSG: c_int = 3;
const PAM_TEXT_INFO: c_int = 4;

#[repr(C)]
struct PamMessage {
    msg_style: c_int,
    msg: *const c_char,
}

#[repr(C)]
struct PamResponse {
    resp: *mut c_char,
    resp_retcode: c_int,
}

type ConversationFn = unsafe extern "C" fn(
    count: c_int,
    messages: *const *const PamMessage,
    responses: *mut *mut PamResponse,
    data: *mut c_void,
) -> c_int;

#[repr(C)]
struct PamConv {
    conv: Option<ConversationFn>,
    appdata_ptr: *mut c_void,
}

type PamStart = unsafe extern "C" fn(
    service: *const c_char,
    user: *const c_char,
    conversation: *const PamConv,
    handle: *mut *mut c_void,
) -> c_int;

type PamStartConfdir = unsafe extern "C" fn(
    service: *const c_char,
    user: *const c_char,
    conversation: *const PamConv,
    confdir: *const c_char,
    handle: *mut *mut c_void,
) -> c_int;

type PamEnd = unsafe extern "C" fn(handle: *mut c_void, status: c_int) -> c_int;
type PamAuthenticate = unsafe extern "C" fn(handle: *mut c_void, flags: c_int) -> c_int;
type PamStrError = unsafe extern "C" fn(handle: *mut c_void, status: c_int) -> *const c_char;

// ---------------------------------------------------------------------------
// Results and diagnostics
// ---------------------------------------------------------------------------

/// What the lock screen is allowed to conclude from one attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthOutcome {
    /// The password was accepted.
    Ok,
    /// The password was wrong (or the account cannot try again now).
    Denied,
    /// The password could not be checked at all; the screen stays locked.
    Unavailable,
}

/// Which stack answers on this machine.  Reported by `--check-lock`, and the
/// reason the local helper fallback is allowed or refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stack {
    /// An administrator's `/etc/pam.d/<name>` file; authoritative.
    Admin(&'static str),
    /// whelk's own `pam_unix` service in the user's runtime directory.
    Private(PathBuf),
    /// No PAM service and no library: only the host's helper can answer.
    LocalFallback,
}

// ---------------------------------------------------------------------------
// libpam at run time
// ---------------------------------------------------------------------------

struct Pam {
    handle: *mut c_void,
    path: String,
    start: PamStart,
    start_confdir: Option<PamStartConfdir>,
    end: PamEnd,
    authenticate: PamAuthenticate,
    acct_mgmt: PamAuthenticate,
    strerror: PamStrError,
}

// The handle and the function pointers are immutable after `open`, and every
// call creates its own PAM handle, so sharing the resolved library is safe.
unsafe impl Send for Pam {}
unsafe impl Sync for Pam {}

impl Drop for Pam {
    fn drop(&mut self) {
        // The process-wide instance is never dropped; this keeps the field
        // meaningful for any future per-call use instead of leaking silently.
        if !self.handle.is_null() {
            unsafe { libc::dlclose(self.handle) };
        }
    }
}

impl Pam {
    fn open(path: &str) -> Option<Self> {
        // A host candidate gets the dependency shim; a store candidate is
        // reached through the build's own rpath and loads as it always has.
        let handle = if path.starts_with("/usr/") || path.starts_with("/lib") {
            open_host_object(Path::new(path), 0, false)?
        } else {
            let library = CString::new(path).ok()?;
            let handle =
                unsafe { libc::dlopen(library.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };

            if handle.is_null() {
                return None;
            }

            handle
        };

        macro_rules! resolve {
            ($symbol:literal) => {{
                let name = concat!($symbol, "\0");
                let symbol = unsafe { libc::dlsym(handle, name.as_ptr() as *const c_char) };

                if symbol.is_null() {
                    unsafe { libc::dlclose(handle) };
                    return None;
                }

                unsafe { std::mem::transmute(symbol) }
            }};
        }

        let start: PamStart = resolve!("pam_start");
        let end: PamEnd = resolve!("pam_end");
        let authenticate: PamAuthenticate = resolve!("pam_authenticate");
        let acct_mgmt: PamAuthenticate = resolve!("pam_acct_mgmt");
        let strerror: PamStrError = resolve!("pam_strerror");

        // Optional: only a libpam older than 1.4 lacks it, and only a machine
        // with no session stack to borrow asks for it.
        let optional = b"pam_start_confdir\0";
        let confdir = unsafe { libc::dlsym(handle, optional.as_ptr() as *const c_char) };
        let start_confdir = if confdir.is_null() {
            None
        } else {
            Some(unsafe { std::mem::transmute::<*mut c_void, PamStartConfdir>(confdir) })
        };

        Some(Self {
            handle,
            path: path.to_owned(),
            start,
            start_confdir,
            end,
            authenticate,
            acct_mgmt,
            strerror,
        })
    }
}

// ---------------------------------------------------------------------------
// The host library's own dependencies
// ---------------------------------------------------------------------------

const HOST_LIBRARY_DIRS: [&str; 4] = ["/lib64", "/usr/lib64", "/lib", "/usr/lib"];

fn is_glibc_soname(soname: &str) -> bool {
    matches!(
        soname,
        "libc.so.6"
            | "libm.so.6"
            | "libmvec.so.1"
            | "libpthread.so.0"
            | "libdl.so.2"
            | "librt.so.1"
            | "libresolv.so.2"
            | "libutil.so.1"
            | "libanl.so.1"
    ) || soname.starts_with("ld-linux")
        || soname.starts_with("libnss_")
}

/// Loads one soname from a distribution directory.
///
/// A missing dependency of the object itself is resolved first, recursively,
/// because `libaudit` may name `libcap-ng` in the same way `libpam` names
/// `libaudit`.  A glibc soname is never loaded from the host: the process
/// already has one, and mixing two would be worse than a locked screen.
fn load_host_soname(soname: &str, depth: u32) -> bool {
    if soname.is_empty() || soname.contains('/') || depth > 4 {
        return false;
    }

    if is_glibc_soname(soname) {
        return true;
    }

    // Already loaded (by this process or an earlier candidate)?  Then the
    // soname is satisfied and no host copy is needed.
    if let Ok(name) = CString::new(soname) {
        let loaded = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_NOLOAD) };

        if !loaded.is_null() {
            unsafe { libc::dlclose(loaded) };

            return true;
        }
    }

    for directory in HOST_LIBRARY_DIRS {
        let path = Path::new(directory).join(soname);

        if path.is_file() && open_host_object(&path, depth + 1, true).is_some() {
            return true;
        }
    }

    // Debian multiarch: /usr/lib/x86_64-linux-gnu and friends.
    for root in ["/usr/lib", "/lib"] {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };

        for entry in entries.flatten() {
            let name = entry.file_name();

            if !name.to_string_lossy().ends_with("-linux-gnu") {
                continue;
            }

            let path = entry.path().join(soname);

            if path.is_file() && open_host_object(&path, depth + 1, true).is_some() {
                return true;
            }
        }
    }

    false
}

/// `dlopen`, but when the loader names a missing host object, load that object
/// from a distribution directory and retry.  The retry count is bounded: an
/// object graph deeper than this is not one whelk can reason about.
fn open_host_object(path: &Path, depth: u32, global: bool) -> Option<*mut c_void> {
    let Ok(path) = CString::new(path.as_os_str().as_encoded_bytes()) else {
        return None;
    };
    let flags = libc::RTLD_NOW
        | if global {
            libc::RTLD_GLOBAL
        } else {
            libc::RTLD_LOCAL
        };
    let mut handle = unsafe { libc::dlopen(path.as_ptr(), flags) };
    let mut attempt = 0;

    while handle.is_null() && attempt < 8 {
        attempt += 1;

        let error = unsafe { libc::dlerror() };

        if error.is_null() {
            break;
        }

        let message = unsafe { CStr::from_ptr(error) }.to_string_lossy();
        let Some((soname, _)) = message.split_once(": cannot open shared object file") else {
            break;
        };

        if !load_host_soname(soname, depth + 1) {
            break;
        }

        handle = unsafe { libc::dlopen(path.as_ptr(), flags) };
    }

    if handle.is_null() { None } else { Some(handle) }
}

/// The library paths to try, most host-specific first.
///
/// `/usr/lib64` and `/lib64` are Fedora, openSUSE and their relatives; a
/// Debian-style multiarch directory carries an architecture triple, so those
/// are discovered rather than listed.  The path the build linked against comes
/// next (a Nix or Guix build may have shrunk an rpath nothing links against),
/// and the loader's own search is the last resort.
pub fn candidates() -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();

    let fixed = [
        "/usr/lib64/libpam.so.0",
        "/lib64/libpam.so.0",
        "/usr/lib/libpam.so.0",
        "/lib/libpam.so.0",
    ];

    for path in fixed {
        push_unique(&mut paths, path.to_owned());
    }

    for root in ["/usr/lib", "/lib"] {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };

        for entry in entries.flatten() {
            let name = entry.file_name();

            if !name.to_string_lossy().ends_with("-linux-gnu") {
                continue;
            }

            let path = entry.path().join("libpam.so.0");

            if path.is_file() {
                push_unique(&mut paths, path.to_string_lossy().into_owned());
            }
        }
    }

    if let Some(libdir) = option_env!("WHELK_PAM_LIBDIR")
        && !libdir.is_empty()
    {
        push_unique(&mut paths, format!("{libdir}/libpam.so.0"));
    }

    push_unique(&mut paths, "libpam.so.0".to_owned());

    paths
}

fn push_unique(paths: &mut Vec<String>, path: String) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

fn pam() -> Option<&'static Pam> {
    static PAM: OnceLock<Option<Pam>> = OnceLock::new();

    PAM.get_or_init(|| {
        for candidate in candidates() {
            if candidate != "libpam.so.0" && !Path::new(&candidate).is_file() {
                continue;
            }

            if let Some(pam) = Pam::open(&candidate) {
                log(&format!("authentication library: {}", pam.path));

                return Some(pam);
            }
        }

        log("no PAM library could be loaded on this machine");

        None
    })
    .as_ref()
}

/// The library whelk actually loaded, for the log line and `--check-lock`.
pub fn pam_path() -> Option<String> {
    pam().map(|pam| pam.path.clone())
}

/// TRUE when the helper fallback may run: no administrator's service file
/// (`whelk-lock` or `login`) owns the answer, a trustworthy helper exists, and
/// strict mode is off.
///
/// This is the whole bypass policy in one place, so a reader does not have to
/// reconstruct it from the call sites.
pub fn fallback_allowed(admin_policy: bool, helper_available: bool, strict: bool) -> bool {
    !admin_policy && helper_available && !strict
}

/// The documented spellings of `WHELK_LOCK_STRICT`, exposed for the test that
/// pins them.
pub fn strict_from_value(value: Option<&str>) -> bool {
    match value.map(str::trim) {
        Some(value) if !value.is_empty() => {
            value == "1"
                || value.eq_ignore_ascii_case("true")
                || value.eq_ignore_ascii_case("yes")
                || value.eq_ignore_ascii_case("on")
        }
        _ => false,
    }
}

/// TRUE when `WHELK_LOCK_STRICT` asks for a fail-closed lock screen: PAM or
/// nothing, never the host's helper.
pub fn strict_mode() -> bool {
    let value = std::env::var("WHELK_LOCK_STRICT").ok();

    strict_from_value(value.as_deref())
}

// ---------------------------------------------------------------------------
// The conversation
// ---------------------------------------------------------------------------

struct Conversation {
    password: *const c_char,
    username: *const c_char,
    supplied: bool,
}

fn free_answers(answers: *mut PamResponse, count: c_int) {
    if answers.is_null() {
        return;
    }

    // SAFETY: PAM allocated `count` consecutive `PamResponse` values for this
    // conversation.  A negative count is treated as empty, and the allocation
    // is still released below.
    let responses = unsafe { std::slice::from_raw_parts_mut(answers, count.max(0) as usize) };

    for response in responses {
        if !response.resp.is_null() {
            // SAFETY: PAM's response is a NUL-terminated allocation; its
            // length is valid for the volatile wipe before `free`.
            unsafe {
                wipe_bytes(std::slice::from_raw_parts_mut(
                    response.resp as *mut u8,
                    libc::strlen(response.resp),
                ));
                libc::free(response.resp as *mut c_void);
            }

            response.resp = std::ptr::null_mut();
        }
    }

    // SAFETY: `answers` came from `calloc` and has not been freed.
    unsafe { libc::free(answers as *mut c_void) };
}

unsafe extern "C" fn conversation(
    count: c_int,
    messages: *const *const PamMessage,
    responses: *mut *mut PamResponse,
    data: *mut c_void,
) -> c_int {
    if count <= 0 || count > 16 || messages.is_null() || responses.is_null() || data.is_null() {
        return PAM_CONV_ERR;
    }

    // SAFETY: the caller checked that `data`, `messages` and `responses` are
    // non-null and that `count` is between 1 and 16, so both arrays are valid
    // for that many entries.  The conversation state outlives the PAM call.
    let state = unsafe { &mut *(data as *mut Conversation) };
    let messages = unsafe { std::slice::from_raw_parts(messages, count as usize) };
    // SAFETY: `calloc` returns either null (handled below) or `count` zeroed
    // `PamResponse` values, which is exactly the slice created here.
    let answers = unsafe { libc::calloc(count as usize, std::mem::size_of::<PamResponse>()) }
        as *mut PamResponse;

    if answers.is_null() {
        return PAM_CONV_ERR;
    }

    // SAFETY: the allocation above is this many entries and is not aliased
    // while the callback fills it.
    let answers = unsafe { std::slice::from_raw_parts_mut(answers, count as usize) };

    for (message, answer) in messages.iter().zip(answers.iter_mut()) {
        let message = *message;

        if message.is_null() {
            free_answers(answers.as_mut_ptr(), count);
            return PAM_CONV_ERR;
        }

        match unsafe { (*message).msg_style } {
            PAM_PROMPT_ECHO_OFF => {
                if state.supplied {
                    free_answers(answers.as_mut_ptr(), count);
                    return PAM_CONV_ERR;
                }

                answer.resp = unsafe { libc::strdup(state.password) };
                state.supplied = true;

                if answer.resp.is_null() {
                    free_answers(answers.as_mut_ptr(), count);
                    return PAM_CONV_ERR;
                }
            }
            PAM_PROMPT_ECHO_ON => {
                answer.resp = unsafe { libc::strdup(state.username) };

                if answer.resp.is_null() {
                    free_answers(answers.as_mut_ptr(), count);
                    return PAM_CONV_ERR;
                }
            }
            PAM_ERROR_MSG | PAM_TEXT_INFO => {}
            _ => {
                free_answers(answers.as_mut_ptr(), count);
                return PAM_CONV_ERR;
            }
        }
    }

    unsafe { *responses = answers.as_mut_ptr() };

    PAM_SUCCESS
}

// ---------------------------------------------------------------------------
// The services
// ---------------------------------------------------------------------------

const SERVICE_FILE: &str = "auth required pam_unix.so\naccount required pam_unix.so\n";

fn service_exists(name: &str) -> bool {
    ["/etc/pam.d", "/usr/lib/pam.d", "/usr/etc/pam.d"]
        .iter()
        .any(|directory| Path::new(directory).join(name).is_file())
}

/// Writes a minimal per-user PAM service to `<runtime_dir>/whelk/pam.d` and
/// returns that directory.
///
/// The service is only a fallback for a machine with no session stack to
/// borrow -- this host has the `common-*` fragments and no `login`, so PAM's
/// compiled-in `other` stack would deny every password.  `pam_unix` covers
/// local accounts through the host's `unix_chkpwd`.
///
/// `runtime_dir` must be absolute (`$XDG_RUNTIME_DIR`, normally).  A relative
/// or missing path returns `None` instead of falling back to `/tmp`: a PAM
/// service another user could write is worse than a locked screen.  A symlink
/// in the way is refused too.
pub fn confdir(runtime_dir: &Path) -> Option<PathBuf> {
    if !runtime_dir.is_absolute() {
        return None;
    }

    let directory = runtime_dir.join("whelk").join("pam.d");

    fs::create_dir_all(&directory).ok()?;
    // The directory may predate this call, so its mode is set rather than
    // assumed: PAM refuses a configuration others can write.
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).ok()?;

    let file = directory.join("whelk-lock");

    if let Ok(metadata) = fs::symlink_metadata(&file)
        && metadata.file_type().is_symlink()
    {
        return None;
    }

    let mut handle = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&file)
        .ok()?;

    handle
        .set_permissions(fs::Permissions::from_mode(0o600))
        .ok()?;
    handle.write_all(SERVICE_FILE.as_bytes()).ok()?;

    Some(directory)
}

fn runtime_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

/// Which stack would answer, without authenticating.
pub fn stack_plan() -> Stack {
    if service_exists("whelk-lock") {
        Stack::Admin("whelk-lock")
    } else if service_exists("login") {
        Stack::Admin("login")
    } else if let Some(directory) = runtime_dir().and_then(|path| confdir(&path)) {
        Stack::Private(directory)
    } else {
        Stack::LocalFallback
    }
}

// ---------------------------------------------------------------------------
// Authentication
// ---------------------------------------------------------------------------

/// Verifies the password for the session user.  `password` is wiped before the
/// function returns, whatever the outcome.
pub fn authenticate(password: &mut [u8]) -> AuthOutcome {
    let stack = stack_plan();
    // An administrator's service file owns the answer; read this once, so both
    // decisions below speak about the same policy.
    let admin_policy = matches!(stack, Stack::Admin(_));
    let strict = strict_mode();

    let Some(pam) = pam() else {
        log("no PAM library; the helper may answer only without an administrator policy");

        return local_fallback(password, admin_policy, strict);
    };

    let Ok(secret) = CString::new(&password[..]) else {
        log("the password contains a NUL byte; refusing to send it to PAM");

        return AuthOutcome::Unavailable;
    };

    let Some(Ok(user)) = session_user().map(CString::new) else {
        return AuthOutcome::Unavailable;
    };

    let mut conversation_state = Conversation {
        password: secret.as_ptr(),
        username: user.as_ptr(),
        supplied: false,
    };
    let conversation = PamConv {
        conv: Some(conversation),
        appdata_ptr: &mut conversation_state as *mut Conversation as *mut c_void,
    };

    let mut handle: *mut c_void = std::ptr::null_mut();
    let mut private_stack = false;

    let status = match stack {
        Stack::Admin(service) => {
            log(&format!("authenticating with PAM service {service}"));

            match CString::new(service) {
                Ok(name) => unsafe {
                    (pam.start)(name.as_ptr(), user.as_ptr(), &conversation, &mut handle)
                },
                Err(_) => PAM_CONV_ERR,
            }
        }
        Stack::Private(directory) => {
            let Some(start_confdir) = pam.start_confdir else {
                log(
                    "this libpam predates pam_start_confdir; the helper may answer only \
                     without an administrator policy",
                );
                wipe_cstring(&secret);

                return local_fallback(password, admin_policy, strict);
            };

            let Ok(directory) = CString::new(directory.as_os_str().as_encoded_bytes()) else {
                wipe_cstring(&secret);

                return AuthOutcome::Unavailable;
            };

            private_stack = true;
            log(&format!(
                "authenticating with the per-user PAM stack {}",
                directory.to_string_lossy()
            ));

            let service = CString::new("whelk-lock").expect("a literal service name");

            unsafe {
                start_confdir(
                    service.as_ptr(),
                    user.as_ptr(),
                    &conversation,
                    directory.as_ptr(),
                    &mut handle,
                )
            }
        }
        Stack::LocalFallback => {
            log("no PAM service file and no per-user PAM directory");

            wipe_cstring(&secret);

            return local_fallback(password, admin_policy, strict);
        }
    };

    if status != PAM_SUCCESS {
        log(&format!("PAM could not start (status {status})"));

        if !handle.is_null() {
            unsafe { (pam.end)(handle, status) };
        }

        wipe_cstring(&secret);

        return local_fallback(password, admin_policy, strict);
    }

    let mut status = unsafe { (pam.authenticate)(handle, PAM_SILENT | PAM_DISALLOW_NULL_AUTHTOK) };

    if status == PAM_SUCCESS {
        status = unsafe { (pam.acct_mgmt)(handle, PAM_SILENT) };
    }

    if status != PAM_SUCCESS {
        let message = unsafe {
            let text = (pam.strerror)(handle, status);

            if text.is_null() {
                "unknown PAM status".to_owned()
            } else {
                CStr::from_ptr(text).to_string_lossy().into_owned()
            }
        };

        log(&format!(
            "PAM refused authentication (status {status}): {message}"
        ));
    }

    unsafe { (pam.end)(handle, status) };
    wipe_cstring(&secret);

    let outcome = match status {
        PAM_SUCCESS => AuthOutcome::Ok,
        PAM_AUTH_ERR | PAM_USER_UNKNOWN | PAM_MAXTRIES | PAM_NEW_AUTHTOK_REQD => {
            AuthOutcome::Denied
        }
        // The symptom this module exists for: the loaded library cannot reach
        // the credential store, so no password could have been checked.
        PAM_AUTHINFO_UNAVAIL => AuthOutcome::Unavailable,
        _ => AuthOutcome::Unavailable,
    };

    // Only whelk's own pam_unix stack falls through: an administrator's service
    // file that refuses is authoritative, and pam_faillock or a locked account
    // must not be talked around.
    if outcome == AuthOutcome::Unavailable && private_stack {
        log("the loaded libpam cannot reach the credential store; asking the host's unix_chkpwd");

        return local_fallback(password, admin_policy, strict);
    }

    outcome
}

/// The one place the helper may be reached.  An administrator's service file
/// owns the answer: PAM's refusal is final, even when the library could not
/// read that file.  `WHELK_LOCK_STRICT` makes even the remaining no-policy case
/// fail closed.
fn local_fallback(password: &[u8], admin_policy: bool, strict: bool) -> AuthOutcome {
    if !fallback_allowed(admin_policy, local_helper().is_some(), strict) {
        if admin_policy {
            log("an administrator's PAM service is authoritative; not asking unix_chkpwd");
        } else if strict {
            log("strict lock mode; not asking unix_chkpwd");
        } else {
            log("no trustworthy unix_chkpwd; the screen stays locked");
        }

        return AuthOutcome::Unavailable;
    }

    local_verify(password)
}

// ---------------------------------------------------------------------------
// The host's setuid helper
// ---------------------------------------------------------------------------

const LOCAL_HELPERS: [&str; 3] = [
    "/usr/sbin/unix_chkpwd",
    "/sbin/unix_chkpwd",
    "/usr/libexec/unix_chkpwd",
];

/// The host's setuid password helper, if this machine has a trustworthy one.
///
/// Only a regular file owned by root, with the setuid bit set and no group or
/// world write permission, is accepted: a helper another user could replace
/// would be a password thief, not an authenticator.
pub fn local_helper() -> Option<String> {
    for path in LOCAL_HELPERS {
        let Ok(metadata) = fs::metadata(path) else {
            continue;
        };

        use std::os::unix::fs::MetadataExt;

        if !metadata.is_file() || metadata.uid() != 0 {
            continue;
        }

        if metadata.mode() & 0o6000 != 0o4000 {
            continue;
        }

        return Some(path.to_owned());
    }

    None
}

fn local_verify(password: &[u8]) -> AuthOutcome {
    let Some(helper) = local_helper() else {
        log("this machine has no setuid unix_chkpwd; the screen stays locked");

        return AuthOutcome::Unavailable;
    };

    let Some(Ok(user)) = session_user().map(CString::new) else {
        return AuthOutcome::Unavailable;
    };

    let mut child = match Command::new(&helper)
        .arg(user.to_string_lossy().as_ref())
        .arg("nonull")
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            log(&format!("could not run {helper}: {error}"));

            return AuthOutcome::Unavailable;
        }
    };

    let delivered = match child.stdin.take() {
        Some(mut stdin) => {
            let written = stdin
                .write_all(password)
                .and_then(|()| stdin.write_all(b"\n"))
                .is_ok();

            // Closing the pipe is what ends the helper's read loop.
            drop(stdin);
            written
        }
        None => false,
    };

    let status = match child.wait() {
        Ok(status) => status,
        Err(error) => {
            log(&format!("{helper} could not be reaped: {error}"));

            return AuthOutcome::Unavailable;
        }
    };

    if !delivered {
        return AuthOutcome::Unavailable;
    }

    match status.code() {
        Some(PAM_SUCCESS) => {
            log("password accepted by the host's unix_chkpwd");

            AuthOutcome::Ok
        }
        Some(PAM_AUTH_ERR | PAM_USER_UNKNOWN | PAM_MAXTRIES | PAM_NEW_AUTHTOK_REQD) => {
            log("the host's unix_chkpwd refused the password");

            AuthOutcome::Denied
        }
        code => {
            log(&format!(
                "the host's unix_chkpwd could not answer (status {})",
                code.map_or_else(|| "signal".to_owned(), |value| value.to_string())
            ));

            AuthOutcome::Unavailable
        }
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Ends a password's lifetime in memory.  Volatile so the compiler cannot
/// decide the writes are dead.
pub fn wipe(bytes: &mut [u8]) {
    wipe_bytes(bytes);
}

fn wipe_bytes(bytes: &mut [u8]) {
    for byte in bytes.iter_mut() {
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
}

fn wipe_cstring(value: &CString) {
    unsafe {
        libc::explicit_bzero(value.as_ptr() as *mut c_void, value.as_bytes().len());
    }
}

/// The user the shell belongs to -- never a name typed at the lock screen.
pub fn session_user() -> Option<String> {
    unsafe {
        let entry = libc::getpwuid(libc::geteuid());

        if entry.is_null() || (*entry).pw_name.is_null() {
            return None;
        }

        Some(
            CStr::from_ptr((*entry).pw_name)
                .to_string_lossy()
                .into_owned(),
        )
    }
}

fn log(message: &str) {
    eprintln!("whelk-lock: {message}");
}

#[cfg(test)]
mod tests {
    use super::{PAM_AUTHINFO_UNAVAIL, candidates, fallback_allowed, strict_from_value};

    #[test]
    fn candidates_are_ordered_unique_and_end_with_the_soname() {
        let paths = candidates();

        assert!(paths.len() >= 2);
        assert_eq!(paths[0], "/usr/lib64/libpam.so.0");
        assert_eq!(paths.last().map(String::as_str), Some("libpam.so.0"));

        for (index, path) in paths.iter().enumerate() {
            if index + 1 < paths.len() {
                assert!(path.starts_with('/'), "{path} is not absolute");
                assert!(path.ends_with("libpam.so.0"));
            }

            assert!(!paths[index + 1..].contains(path), "{path} repeats");
        }
    }

    #[test]
    fn the_status_that_started_the_fallback_is_still_the_one_documented() {
        assert_eq!(PAM_AUTHINFO_UNAVAIL, 9);
    }

    #[test]
    fn the_helper_fallback_never_overrides_an_administrator() {
        assert!(!fallback_allowed(true, true, false));
        assert!(!fallback_allowed(false, false, false));
        assert!(!fallback_allowed(false, true, true));
        assert!(fallback_allowed(false, true, false));
    }

    #[test]
    fn strict_mode_accepts_only_the_documented_spellings() {
        for value in ["1", "true", "TRUE", "yes", "on", " 1 "] {
            assert!(strict_from_value(Some(value)), "{value}");
        }

        for value in ["", "0", "false", "no", "off", "2", "treu"] {
            assert!(!strict_from_value(Some(value)), "{value}");
        }

        assert!(!strict_from_value(None));
    }
}
