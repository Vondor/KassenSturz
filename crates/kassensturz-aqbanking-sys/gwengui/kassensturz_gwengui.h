// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The C interface of kassensturz-gwengui, KassenSturz's small C++ library
// around Gwenhywfar's Qt 6 dialogs (Q-035 option A, D-140). It is loaded at
// runtime; the Rust side of this interface is src/gwengui.rs, which must
// change together with this file. Raise KS_GWENGUI_ABI_VERSION with every
// change. This header depends on no Gwenhywfar header: Gwenhywfar's enums
// travel as int, its GWEN_DB_NODE as void *.

#ifndef KASSENSTURZ_GWENGUI_H
#define KASSENSTURZ_GWENGUI_H

#include <stdint.h>

#if defined(KS_GWENGUI_BUILDING) && defined(__GNUC__)
#define KS_GWENGUI_EXPORT __attribute__((visibility("default")))
#else
#define KS_GWENGUI_EXPORT
#endif

#ifdef __cplusplus
extern "C" {
#endif

#define KS_GWENGUI_ABI_VERSION 1

// A callback answered the request.
#define KS_GWENGUI_HANDLED 0
// A callback leaves the request to Gwenhywfar's own dialog.
#define KS_GWENGUI_NOT_HANDLED 1

typedef struct KS_GWENGUI KS_GWENGUI;

// Asked for a PIN or a TAN. method_id is a GWEN_GUI_PASSWORD_METHOD,
// method_params a GWEN_DB_NODE * with the method's data or NULL. The answer
// goes into buffer (max_len bytes including the terminating zero, at least
// min_len bytes long). Returns KS_GWENGUI_HANDLED, KS_GWENGUI_NOT_HANDLED or
// a negative Gwenhywfar error code.
typedef int (*ks_gwengui_get_password_fn)(void *user_data, uint32_t flags, const char *token,
                                          const char *title, const char *text, char *buffer,
                                          int min_len, int max_len, int method_id,
                                          void *method_params, uint32_t gui_id);

// Told how a PIN was received; status is a GWEN_GUI_PASSWORD_STATUS. Same
// results as above.
typedef int (*ks_gwengui_set_password_status_fn)(void *user_data, const char *token,
                                                 const char *pin, int status, uint32_t gui_id);

// Offered a log line; level is a GWEN_LOGGER_LEVEL. Returns nonzero when the
// line is handled and Gwenhywfar should not print it itself.
typedef int (*ks_gwengui_log_hook_fn)(void *user_data, const char *log_domain, int level,
                                      const char *message);

// Told about a certificate check. verdict is the library's own decision (0
// accepted, otherwise the Gwenhywfar error it refuses with); the result is
// the final decision, but a refusal can't be turned into an acceptance.
typedef int (*ks_gwengui_check_cert_fn)(void *user_data, uint32_t status_flags,
                                        const char *status_text, const char *common_name,
                                        const char *fingerprint_sha512, int verdict,
                                        uint32_t gui_id);

typedef struct {
    void *user_data;
    ks_gwengui_get_password_fn get_password;
    ks_gwengui_set_password_status_fn set_password_status;
    ks_gwengui_log_hook_fn log_hook;
    ks_gwengui_check_cert_fn check_cert;
} KS_GWENGUI_CALLBACKS;

// The interface version this library was built with.
KS_GWENGUI_EXPORT int ks_gwengui_abi_version(void);

// Creates the GUI and makes it Gwenhywfar's current GUI. Needs GWEN_Init to
// have run and a QApplication to exist; returns NULL otherwise. callbacks is
// copied and may be NULL.
KS_GWENGUI_EXPORT KS_GWENGUI *ks_gwengui_create(const KS_GWENGUI_CALLBACKS *callbacks);

// Destroys the GUI; if it is the current one, Gwenhywfar has none afterwards.
KS_GWENGUI_EXPORT void ks_gwengui_destroy(KS_GWENGUI *gui);

// The GWEN_GUI_FLAGS_* the GUI runs with, for tests and the log.
KS_GWENGUI_EXPORT uint32_t ks_gwengui_flags(const KS_GWENGUI *gui);

#ifdef __cplusplus
}
#endif

#endif
