// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// kassensturz-gwengui: Gwenhywfar's Qt 6 GUI with KassenSturz's rules
// (Q-035 option A, D-140). See kassensturz_gwengui.h for the interface and
// src/gwengui.rs for the Rust side.

#include "kassensturz_gwengui.h"

#include <QApplication>
#include <QCoreApplication>
#include <gwen-gui-qt5/qt5_gui.hpp>
#include <gwenhywfar/error.h>
#include <gwenhywfar/gui.h>
#include <gwenhywfar/ssl_cert_descr.h>
#include <gwenhywfar/ssl_cert_flags.h>

namespace {

// Gwenhywfar 5.14.1 builds its Qt 6 GUI from the Qt 5 sources; the class is
// still called QT5_Gui (Q-035).
class KassenSturzGui final : public QT5_Gui
{
public:
    explicit KassenSturzGui(const KS_GWENGUI_CALLBACKS *callbacks)
        : callbacks_(callbacks ? *callbacks : KS_GWENGUI_CALLBACKS{})
    {
        // Threat T-01: Gwenhywfar never gets a password store or database
        // (neither setter is called), and without this flag its password
        // dialog has no "save" option.
        GWEN_Gui_SubFlags(_gui, GWEN_GUI_FLAGS_PERMPASSWORDS);
        // Stated intent (D-140). Gwenhywfar 5.14.1 reads neither flag, so
        // checkCert below enforces the rule itself.
        GWEN_Gui_AddFlags(_gui, GWEN_GUI_FLAGS_ACCEPTVALIDCERTS | GWEN_GUI_FLAGS_REJECTINVALIDCERTS);
        GWEN_Gui_SetName(_gui, "kassensturz-gui");
    }

    GWEN_GUI *cInterface() const { return _gui; }

protected:
    int getPassword(uint32_t flags, const char *token, const char *title, const char *text,
                    char *buffer, int minLen, int maxLen, GWEN_GUI_PASSWORD_METHOD methodId,
                    GWEN_DB_NODE *methodParams, uint32_t guiid) override
    {
        if (callbacks_.get_password) {
            const int rc = callbacks_.get_password(callbacks_.user_data, flags, token, title, text,
                                                   buffer, minLen, maxLen, static_cast<int>(methodId),
                                                   methodParams, guiid);
            if (rc != KS_GWENGUI_NOT_HANDLED) {
                return rc;
            }
        }
        return QT5_Gui::getPassword(flags, token, title, text, buffer, minLen, maxLen, methodId,
                                    methodParams, guiid);
    }

    int setPasswordStatus(const char *token, const char *pin, GWEN_GUI_PASSWORD_STATUS status,
                          uint32_t guiid) override
    {
        if (callbacks_.set_password_status) {
            const int rc = callbacks_.set_password_status(callbacks_.user_data, token, pin,
                                                          static_cast<int>(status), guiid);
            if (rc != KS_GWENGUI_NOT_HANDLED) {
                return rc;
            }
        }
        return QT5_Gui::setPasswordStatus(token, pin, status, guiid);
    }

    // Threat T-12: a certificate the TLS layer found anything wrong with is
    // refused, without the "connect anyway" question of Gwenhywfar's own
    // check. The TLS layer sets the error mark and every flag but OK when
    // something is wrong (syncio_tls.c), so both are checked.
    int checkCert(const GWEN_SSLCERTDESCR *cert, GWEN_SYNCIO *, uint32_t guiid) override
    {
        const uint32_t status = GWEN_SslCertDescr_GetStatusFlags(cert);
        const bool clean = GWEN_SslCertDescr_GetIsError(cert) == 0 && (status & GWEN_SSL_CERT_FLAGS_OK)
                           && (status & ~GWEN_SSL_CERT_FLAGS_OK) == 0;
        int verdict = clean ? 0 : GWEN_ERROR_SSL_SECURITY;
        if (callbacks_.check_cert) {
            const int rc = callbacks_.check_cert(
                callbacks_.user_data, status, GWEN_SslCertDescr_GetStatusText(cert),
                GWEN_SslCertDescr_GetCommonName(cert), GWEN_SslCertDescr_GetFingerPrintSha512(cert),
                verdict, guiid);
            // A callback can refuse, never accept what was refused.
            if (verdict == 0 && rc != 0) {
                verdict = rc < 0 ? rc : GWEN_ERROR_SSL_SECURITY;
            }
        }
        return verdict;
    }

    int logHook(const char *logDomain, GWEN_LOGGER_LEVEL priority, const char *s) override
    {
        if (callbacks_.log_hook) {
            return callbacks_.log_hook(callbacks_.user_data, logDomain, static_cast<int>(priority), s);
        }
        return QT5_Gui::logHook(logDomain, priority, s);
    }

private:
    KS_GWENGUI_CALLBACKS callbacks_;
};

KassenSturzGui *fromHandle(KS_GWENGUI *gui)
{
    return reinterpret_cast<KassenSturzGui *>(gui);
}

} // namespace

extern "C" {

int ks_gwengui_abi_version(void)
{
    return KS_GWENGUI_ABI_VERSION;
}

KS_GWENGUI *ks_gwengui_create(const KS_GWENGUI_CALLBACKS *callbacks)
{
    // Widgets dialogs need a QApplication, not just a QGuiApplication.
    if (qobject_cast<QApplication *>(QCoreApplication::instance()) == nullptr) {
        return nullptr;
    }
    auto *gui = new KassenSturzGui(callbacks);
    GWEN_Gui_SetGui(gui->cInterface());
    return reinterpret_cast<KS_GWENGUI *>(gui);
}

void ks_gwengui_destroy(KS_GWENGUI *handle)
{
    if (handle == nullptr) {
        return;
    }
    KassenSturzGui *gui = fromHandle(handle);
    if (GWEN_Gui_GetGui() == gui->cInterface()) {
        GWEN_Gui_SetGui(nullptr);
    }
    delete gui;
}

uint32_t ks_gwengui_flags(const KS_GWENGUI *handle)
{
    if (handle == nullptr) {
        return 0;
    }
    return GWEN_Gui_GetFlags(fromHandle(const_cast<KS_GWENGUI *>(handle))->cInterface());
}

} // extern "C"
