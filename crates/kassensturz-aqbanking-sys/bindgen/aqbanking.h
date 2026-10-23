// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The AqBanking headers that `generate.sh` feeds to bindgen (D-060). Only
// the declarations matched by the allowlists in that script end up in
// `src/aqbanking.rs`: what reading balances and statements needs, nothing
// that moves money (D-053, T-14) and no floating-point amounts (D-141).
#include <aqbanking/version.h>
#include <aqbanking/error.h>
#include <aqbanking/banking.h>
#include <aqbanking/banking_online.h>
#include <aqbanking/banking_bankinfo.h>
#include <aqbanking/banking_dialogs.h>
#include <aqbanking/types/value.h>
#include <aqbanking/types/balance.h>
#include <aqbanking/types/account_spec.h>
#include <aqbanking/types/transaction.h>
#include <aqbanking/types/imexporter_context.h>
#include <aqbanking/types/imexporter_accountinfo.h>
#include <aqbanking/types/bankinfo.h>
#include <aqbanking/types/bankinfoservice.h>
#include <aqbanking/types/message.h>
