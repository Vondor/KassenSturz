// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Loads kassensturz-gwengui the way KassenSturz does (dlopen, the four C
// functions by name) and drives Gwenhywfar's GUI entry points through it.
// Runs under ctest on Qt's offscreen platform. Nothing here is a real PIN or
// certificate (D-055).

#include "kassensturz_gwengui.h"

#include <QApplication>
#include <dlfcn.h>
#include <gwenhywfar/error.h>
#include <gwenhywfar/gui.h>
#include <gwenhywfar/gwenhywfar.h>
#include <gwenhywfar/logger.h>
#include <gwenhywfar/ssl_cert_descr.h>
#include <gwenhywfar/ssl_cert_flags.h>

#include <cstdio>
#include <cstring>
#include <string>

static_assert(sizeof(KS_GWENGUI_CALLBACKS) == 5 * sizeof(void *),
              "src/gwengui.rs mirrors this layout");

namespace {

struct Record {
    std::string passwordToken;
    void *methodParams = nullptr;
    std::string statusToken;
    int status = 12345;
    std::string logDomain;
    int logLevel = -1;
    std::string logMessage;
    int certVerdict = 12345;
    uint32_t certFlags = 0;
    std::string certName;
    int certAnswer = 0;
    int certCalls = 0;
};

int getPassword(void *user, uint32_t, const char *token, const char *, const char *, char *buffer,
                int, int maxLen, int, void *methodParams, uint32_t)
{
    auto *record = static_cast<Record *>(user);
    record->passwordToken = token ? token : "";
    record->methodParams = methodParams;
    if (record->passwordToken == "ask-gwenhywfar") {
        return KS_GWENGUI_NOT_HANDLED;
    }
    std::snprintf(buffer, static_cast<size_t>(maxLen), "%s", "123456");
    return KS_GWENGUI_HANDLED;
}

int setPasswordStatus(void *user, const char *token, const char *, int status, uint32_t)
{
    auto *record = static_cast<Record *>(user);
    record->statusToken = token ? token : "";
    record->status = status;
    return KS_GWENGUI_HANDLED;
}

int logHook(void *user, const char *domain, int level, const char *message)
{
    auto *record = static_cast<Record *>(user);
    record->logDomain = domain ? domain : "";
    record->logLevel = level;
    record->logMessage = message ? message : "";
    return 1;
}

int checkCert(void *user, uint32_t flags, const char *, const char *name, const char *, int verdict,
              uint32_t)
{
    auto *record = static_cast<Record *>(user);
    record->certCalls++;
    record->certFlags = flags;
    record->certName = name ? name : "";
    record->certVerdict = verdict;
    return record->certAnswer;
}

int failures = 0;

void check(bool ok, const char *what)
{
    if (!ok) {
        std::fprintf(stderr, "FAILED: %s\n", what);
        failures++;
    }
}

GWEN_SSLCERTDESCR *certificate(uint32_t flags, int isError)
{
    GWEN_SSLCERTDESCR *cert = GWEN_SslCertDescr_new();
    GWEN_SslCertDescr_SetStatusFlags(cert, flags);
    GWEN_SslCertDescr_SetIsError(cert, isError);
    GWEN_SslCertDescr_SetCommonName(cert, "bank.example");
    GWEN_SslCertDescr_SetFingerPrintSha512(cert, "00");
    return cert;
}

} // namespace

int main(int argc, char **argv)
{
    if (argc != 2) {
        std::fprintf(stderr, "usage: %s <path to libkassensturz-gwengui.so>\n", argv[0]);
        return 2;
    }

    void *library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (library == nullptr) {
        std::fprintf(stderr, "dlopen failed: %s\n", dlerror());
        return 1;
    }
    auto abiVersion = reinterpret_cast<int (*)(void)>(dlsym(library, "ks_gwengui_abi_version"));
    auto create = reinterpret_cast<KS_GWENGUI *(*)(const KS_GWENGUI_CALLBACKS *)>(
        dlsym(library, "ks_gwengui_create"));
    auto destroy = reinterpret_cast<void (*)(KS_GWENGUI *)>(dlsym(library, "ks_gwengui_destroy"));
    auto flags = reinterpret_cast<uint32_t (*)(const KS_GWENGUI *)>(dlsym(library, "ks_gwengui_flags"));
    check(abiVersion && create && destroy && flags, "all four functions are exported");
    if (failures) {
        return 1;
    }
    check(abiVersion() == KS_GWENGUI_ABI_VERSION, "the interface version matches the header");

    // Without a QApplication there is no GUI.
    check(GWEN_Init() == 0, "GWEN_Init");
    check(create(nullptr) == nullptr, "no GUI without a QApplication");

    QApplication app(argc, argv);
    Record record;
    KS_GWENGUI_CALLBACKS callbacks{&record, getPassword, setPasswordStatus, logHook, checkCert};
    KS_GWENGUI *gui = create(&callbacks);
    check(gui != nullptr, "the GUI is created");
    if (gui == nullptr) {
        return 1;
    }
    check(GWEN_Gui_GetGui() != nullptr, "the GUI is Gwenhywfar's current GUI");

    // D-140, threat T-01: no saved passwords; the certificate flags are set.
    const uint32_t guiFlags = flags(gui);
    check((guiFlags & GWEN_GUI_FLAGS_PERMPASSWORDS) == 0, "GWEN_GUI_FLAGS_PERMPASSWORDS is cleared");
    check((guiFlags & GWEN_GUI_FLAGS_ACCEPTVALIDCERTS) != 0, "GWEN_GUI_FLAGS_ACCEPTVALIDCERTS is set");
    check((guiFlags & GWEN_GUI_FLAGS_REJECTINVALIDCERTS) != 0, "GWEN_GUI_FLAGS_REJECTINVALIDCERTS is set");
    check(GWEN_Gui_GetPasswdStore(GWEN_Gui_GetGui()) == nullptr, "Gwenhywfar has no password store");

    // A PIN request reaches the callback, with the method's parameters, and
    // its answer reaches the caller.
    char buffer[32] = {};
    GWEN_DB_NODE *params = GWEN_DB_Group_new("params");
    int rc = GWEN_Gui_GetPassword(0, "bank/login", "Title", "Text", buffer, 4, sizeof buffer,
                                  GWEN_Gui_PasswordMethod_Text, params, 0);
    check(rc == 0, "GWEN_Gui_GetPassword returns the callback's result");
    check(std::strcmp(buffer, "123456") == 0, "the callback's answer is in the buffer");
    check(record.passwordToken == "bank/login", "the callback sees the token");
    check(record.methodParams == params, "the callback sees the method's parameters");
    GWEN_DB_Group_free(params);

    // The bank called the PIN wrong (D-140, threat T-05).
    rc = GWEN_Gui_SetPasswordStatus("bank/login", "123456", GWEN_Gui_PasswordStatus_Bad, 0);
    check(rc == 0, "GWEN_Gui_SetPasswordStatus returns the callback's result");
    check(record.statusToken == "bank/login", "the status callback sees the token");
    check(record.status == GWEN_Gui_PasswordStatus_Bad, "the status callback sees the status");

    // Log lines reach the callback (D-140: only KassenSturz's log, no file).
    rc = GWEN_Gui_LogHook("aqbanking", GWEN_LoggerLevel_Warning, "something to note");
    check(rc == 1, "the log hook's result is passed on");
    check(record.logDomain == "aqbanking" && record.logLevel == GWEN_LoggerLevel_Warning
              && record.logMessage == "something to note",
          "the log hook sees domain, level and message");

    // Certificates (D-140, threat T-12): clean ones are accepted without a
    // question, everything else is refused, and the callback is told.
    GWEN_SSLCERTDESCR *clean = certificate(GWEN_SSL_CERT_FLAGS_OK, 0);
    check(GWEN_Gui_CheckCert(clean, nullptr, 0) == 0, "a clean certificate is accepted");
    check(record.certCalls == 1 && record.certVerdict == 0 && record.certName == "bank.example"
              && record.certFlags == GWEN_SSL_CERT_FLAGS_OK,
          "the callback sees the accepted certificate");

    GWEN_SSLCERTDESCR *expired = certificate(GWEN_SSL_CERT_FLAGS_EXPIRED, 1);
    check(GWEN_Gui_CheckCert(expired, nullptr, 0) == GWEN_ERROR_SSL_SECURITY,
          "a certificate with an error is refused");
    check(record.certCalls == 2 && record.certVerdict == GWEN_ERROR_SSL_SECURITY,
          "the callback sees the refusal");

    GWEN_SSLCERTDESCR *oddFlag = certificate(GWEN_SSL_CERT_FLAGS_OK | GWEN_SSL_CERT_FLAGS_BAD_HOSTNAME, 0);
    check(GWEN_Gui_CheckCert(oddFlag, nullptr, 0) == GWEN_ERROR_SSL_SECURITY,
          "a certificate with any flag but OK is refused");

    GWEN_SSLCERTDESCR *noFlags = certificate(0, 0);
    check(GWEN_Gui_CheckCert(noFlags, nullptr, 0) == GWEN_ERROR_SSL_SECURITY,
          "a certificate without the OK flag is refused");

    // The callback can refuse a clean certificate, but not accept a refused one.
    record.certAnswer = GWEN_ERROR_USER_ABORTED;
    check(GWEN_Gui_CheckCert(clean, nullptr, 0) == GWEN_ERROR_USER_ABORTED,
          "the callback can refuse a clean certificate");
    record.certAnswer = 0;
    check(GWEN_Gui_CheckCert(expired, nullptr, 0) == GWEN_ERROR_SSL_SECURITY,
          "the callback cannot accept a refused certificate");
    record.certAnswer = 7;
    check(GWEN_Gui_CheckCert(clean, nullptr, 0) == GWEN_ERROR_SSL_SECURITY,
          "a positive answer from the callback refuses too");

    GWEN_SslCertDescr_free(noFlags);
    GWEN_SslCertDescr_free(oddFlag);
    GWEN_SslCertDescr_free(expired);
    GWEN_SslCertDescr_free(clean);

    destroy(gui);
    check(GWEN_Gui_GetGui() == nullptr, "after destroy Gwenhywfar has no GUI");
    destroy(nullptr);
    check(flags(nullptr) == 0, "flags of no GUI are 0");

    GWEN_Fini();
    dlclose(library);

    if (failures) {
        std::fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    std::printf("kassensturz-gwengui: all checks passed\n");
    return 0;
}
