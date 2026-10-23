// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Keeps the screens off the network (D-143, threat T-08). Qt Quick shows a
//! text that looks like HTML as rich text, and rich text loads pictures from
//! the web. Labels that show text from a bank or a file say
//! `textFormat: Text.PlainText`; this is the second lock for a label that
//! forgot: the engine's network access manager refuses every address that is
//! not a local file or a Qt resource.

use std::pin::Pin;

use cxx_qt_lib::QQmlApplicationEngine;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qqmlapplicationengine.h");
        /// Qt's QML engine of an app.
        type QQmlApplicationEngine = cxx_qt_lib::QQmlApplicationEngine;
    }

    #[namespace = "kassensturz::network_guard"]
    unsafe extern "C++" {
        include!("kassensturz-ui-bridge/cpp/local_only.h");

        #[cxx_name = "installLocalOnly"]
        fn install_local_only(engine: Pin<&mut QQmlApplicationEngine>);

        /// Only the tests ask a manager of their own.
        #[allow(dead_code)]
        #[cxx_name = "isRefused"]
        fn is_refused(url: &QString) -> bool;
    }
}

/// Makes the engine refuse every address that isn't on this computer. Call it
/// before the engine loads its first file.
pub fn install(engine: Pin<&mut QQmlApplicationEngine>) {
    ffi::install_local_only(engine);
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::net::TcpListener;
    use std::sync::Mutex;

    use cxx_qt_lib::QString;

    use super::ffi;

    /// Qt's application is made once, by whichever test asks first, and its
    /// event loop belongs to one thread at a time.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    fn refused(url: &str) -> bool {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        ffi::is_refused(&QString::from(url))
    }

    #[test]
    fn a_picture_from_the_web_is_refused_and_nothing_connects() {
        let server = TcpListener::bind("127.0.0.1:0").expect("a free port on this computer");
        server.set_nonblocking(true).expect("a non-blocking server");
        let port = server.local_addr().expect("the port").port();
        // What `<img src="…">` in a payee name would ask for.
        for url in [
            format!("http://127.0.0.1:{port}/pixel.png"),
            format!("https://127.0.0.1:{port}/pixel.png"),
            format!("HTTP://127.0.0.1:{port}/pixel.png"),
            format!("ftp://127.0.0.1:{port}/pixel.png"),
        ] {
            assert!(refused(&url), "{url} should be refused");
        }
        match server.accept() {
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            other => panic!("something connected: {other:?}"),
        }
    }

    #[test]
    fn local_files_and_resources_are_let_through() {
        let dir = std::env::temp_dir();
        let file = format!("file://{}", dir.display());
        for url in [
            file.as_str(),
            "qrc:/qt/qml/io/github/vondor/kassensturz/qml/Main.qml",
        ] {
            assert!(!refused(url), "{url} should not be refused");
        }
    }
}
