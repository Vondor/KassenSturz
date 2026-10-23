// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The C interface of `kassensturz-gwengui`, KassenSturz's own small C++
//! library around Gwenhywfar's Qt 6 dialogs (Q-035 option A, D-140).
//!
//! Written by hand to match `gwengui/kassensturz_gwengui.h`; the two must
//! change together, and [`KS_GWENGUI_ABI_VERSION`] is raised with every
//! change so a stale installed library is noticed. The library is built by
//! CMake when `gwengui-qt6` is installed, and loaded at runtime with
//! [`GwenGui::new`] from the path the app knows (M4.6).
//!
//! The library makes Gwenhywfar's Qt GUI the current GUI and routes four of
//! its callbacks to KassenSturz, so that KassenSturz's own flow can answer a
//! PIN from the PIN remembered until the app closes or saved in the keychain
//! (D-057, M4.3, M4.5) and still fall back to Gwenhywfar's dialogs for TANs
//! and everything it doesn't handle. Two rules live in the library itself,
//! not in a callback (D-140, threats T-01, T-12): Gwenhywfar never gets a
//! password store and `GWEN_GUI_FLAGS_PERMPASSWORDS` is cleared, so it can't
//! save a PIN; and a certificate is accepted only when the TLS layer found
//! nothing wrong with it, every other one is refused without a "connect
//! anyway" question (Gwenhywfar 5.14.1 defines `GWEN_GUI_FLAGS_ACCEPTVALIDCERTS`
//! and `GWEN_GUI_FLAGS_REJECTINVALIDCERTS` but reads neither, and its own
//! check asks for every certificate).
//!
//! Callback results: [`KS_GWENGUI_HANDLED`] means the callback answered,
//! [`KS_GWENGUI_NOT_HANDLED`] hands the request to Gwenhywfar's own dialog,
//! a negative value is a Gwenhywfar error code returned as is (for example
//! `GWEN_ERROR_USER_ABORTED`).

use std::ffi::OsStr;
use std::os::raw::{c_char, c_int, c_void};

/// The version of the C interface; `ks_gwengui_abi_version` must return it.
pub const KS_GWENGUI_ABI_VERSION: c_int = 1;
/// A callback's result: it answered the request.
pub const KS_GWENGUI_HANDLED: c_int = 0;
/// A callback's result: Gwenhywfar's own dialog takes over.
pub const KS_GWENGUI_NOT_HANDLED: c_int = 1;

/// The library's GUI object; only ever used through a pointer.
#[repr(C)]
pub struct KS_GWENGUI {
    _unused: [u8; 0],
}

/// Asked for a PIN or a TAN. `method_id` is a `GWEN_GUI_PASSWORD_METHOD`;
/// `method_params` a `GWEN_DB_NODE *` with the method's data (the challenge
/// of an optical TAN), or null. The answer is written into `buffer`, which
/// holds `max_len` bytes including the terminating zero; the answer must be
/// at least `min_len` bytes long.
pub type KsGwenGuiGetPasswordFn = Option<
    unsafe extern "C" fn(
        user_data: *mut c_void,
        flags: u32,
        token: *const c_char,
        title: *const c_char,
        text: *const c_char,
        buffer: *mut c_char,
        min_len: c_int,
        max_len: c_int,
        method_id: c_int,
        method_params: *mut c_void,
        gui_id: u32,
    ) -> c_int,
>;

/// Told how a PIN was received; `status` is a `GWEN_GUI_PASSWORD_STATUS`
/// (`GWEN_Gui_PasswordStatus_Bad` when the bank called it wrong, D-140).
pub type KsGwenGuiSetPasswordStatusFn = Option<
    unsafe extern "C" fn(
        user_data: *mut c_void,
        token: *const c_char,
        pin: *const c_char,
        status: c_int,
        gui_id: u32,
    ) -> c_int,
>;

/// Offered a log line; `level` is a `GWEN_LOGGER_LEVEL`. Returns nonzero when
/// the line is handled and Gwenhywfar should not print it itself.
pub type KsGwenGuiLogHookFn = Option<
    unsafe extern "C" fn(
        user_data: *mut c_void,
        log_domain: *const c_char,
        level: c_int,
        message: *const c_char,
    ) -> c_int,
>;

/// Told about a certificate check. `verdict` is the library's own decision:
/// 0 accepted, otherwise the Gwenhywfar error it refuses with. The result is
/// the final decision: a callback can refuse an accepted certificate but
/// never accept a refused one (the library keeps the refusal).
pub type KsGwenGuiCheckCertFn = Option<
    unsafe extern "C" fn(
        user_data: *mut c_void,
        status_flags: u32,
        status_text: *const c_char,
        common_name: *const c_char,
        fingerprint_sha512: *const c_char,
        verdict: c_int,
        gui_id: u32,
    ) -> c_int,
>;

/// What `ks_gwengui_create` is given: KassenSturz's callbacks. Any of them
/// may be `None`, then Gwenhywfar's own behaviour applies.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KsGwenGuiCallbacks {
    /// Passed back to every callback as its first argument.
    pub user_data: *mut c_void,
    /// See [`KsGwenGuiGetPasswordFn`].
    pub get_password: KsGwenGuiGetPasswordFn,
    /// See [`KsGwenGuiSetPasswordStatusFn`].
    pub set_password_status: KsGwenGuiSetPasswordStatusFn,
    /// See [`KsGwenGuiLogHookFn`].
    pub log_hook: KsGwenGuiLogHookFn,
    /// See [`KsGwenGuiCheckCertFn`].
    pub check_cert: KsGwenGuiCheckCertFn,
}

/// The library's functions, loaded from its file.
pub struct GwenGui {
    __library: libloading::Library,
    /// Returns the C interface's version; must equal
    /// [`KS_GWENGUI_ABI_VERSION`].
    pub ks_gwengui_abi_version: unsafe extern "C" fn() -> c_int,
    /// Creates the GUI and makes it Gwenhywfar's current GUI. Needs
    /// `GWEN_Init` to have run and a `QApplication` to exist; returns null
    /// otherwise. `callbacks` is copied and may be null.
    pub ks_gwengui_create:
        unsafe extern "C" fn(callbacks: *const KsGwenGuiCallbacks) -> *mut KS_GWENGUI,
    /// Destroys the GUI; if it is the current one, Gwenhywfar has no GUI
    /// afterwards. Null is ignored.
    pub ks_gwengui_destroy: unsafe extern "C" fn(gui: *mut KS_GWENGUI),
    /// The `GWEN_GUI_FLAGS_*` the GUI runs with, for tests and the log.
    pub ks_gwengui_flags: unsafe extern "C" fn(gui: *const KS_GWENGUI) -> u32,
}

impl GwenGui {
    /// Loads the library from `path` and looks up all its functions.
    ///
    /// # Safety
    ///
    /// As for `libloading::Library::new`: loading runs the library's
    /// initialisation code. The returned value must outlive every GUI
    /// created through it.
    pub unsafe fn new<P: AsRef<OsStr>>(path: P) -> Result<Self, libloading::Error> {
        let library = unsafe { libloading::Library::new(path.as_ref()) }?;
        unsafe { Self::from_library(library) }
    }

    /// Looks up all functions in an already loaded library.
    ///
    /// # Safety
    ///
    /// The library must be `kassensturz-gwengui`; the symbols are taken for
    /// the signatures declared here.
    pub unsafe fn from_library(library: libloading::Library) -> Result<Self, libloading::Error> {
        let ks_gwengui_abi_version =
            unsafe { library.get(b"ks_gwengui_abi_version\0") }.map(|sym| *sym)?;
        let ks_gwengui_create = unsafe { library.get(b"ks_gwengui_create\0") }.map(|sym| *sym)?;
        let ks_gwengui_destroy = unsafe { library.get(b"ks_gwengui_destroy\0") }.map(|sym| *sym)?;
        let ks_gwengui_flags = unsafe { library.get(b"ks_gwengui_flags\0") }.map(|sym| *sym)?;
        Ok(GwenGui {
            __library: library,
            ks_gwengui_abi_version,
            ks_gwengui_create,
            ks_gwengui_destroy,
            ks_gwengui_flags,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_callback_table_has_the_layout_the_c_header_declares() {
        // Five pointer-sized fields, in the header's order; the C++ test
        // checks the same with a static_assert.
        assert_eq!(
            std::mem::size_of::<KsGwenGuiCallbacks>(),
            5 * std::mem::size_of::<*mut c_void>()
        );
        assert_eq!(
            std::mem::align_of::<KsGwenGuiCallbacks>(),
            std::mem::align_of::<*mut c_void>()
        );
    }

    #[test]
    fn results_match_the_c_header() {
        assert_eq!(KS_GWENGUI_ABI_VERSION, 1);
        assert_eq!(KS_GWENGUI_HANDLED, 0);
        assert_eq!(KS_GWENGUI_NOT_HANDLED, 1);
    }
}
