// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The Gwenhywfar headers that `generate.sh` feeds to bindgen (D-060). Only
// the declarations matched by the allowlists in that script end up in
// `src/gwenhywfar.rs`; see there for what is left out and why.
#include <gwenhywfar/gwenhywfar.h>
#include <gwenhywfar/version.h>
#include <gwenhywfar/error.h>
#include <gwenhywfar/logger.h>
#include <gwenhywfar/gui.h>
#include <gwenhywfar/gui_be.h>
#include <gwenhywfar/ssl_cert_descr.h>
#include <gwenhywfar/ssl_cert_flags.h>
#include <gwenhywfar/gwendate.h>
#include <gwenhywfar/gwentime.h>
#include <gwenhywfar/stringlist.h>
#include <gwenhywfar/buffer.h>
#include <gwenhywfar/db.h>
