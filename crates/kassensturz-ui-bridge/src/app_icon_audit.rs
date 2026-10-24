// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests that the app has an icon and that everything that shows it can find
//! it (D-163). Before this the desktop file had no `Icon=` and nothing
//! installed an icon, so Plasma showed its placeholder for the window and the
//! launcher. The tests read the files as text, as `plain_text_audit` does for
//! the QML, because the install itself happens in CMake and in the Flatpak,
//! whose CI job checks the installed result.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use crate::platform::APP_ID;

    /// The sizes `tools/render-icons.sh` draws: 16 px for lists and title
    /// bars, 22 and 24 px for panels, and 128 px and up for stores, which
    /// ask for at least that.
    const SIZES: [u32; 8] = [16, 22, 24, 32, 48, 64, 128, 256];

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn icons() -> PathBuf {
        root().join("data/icons/hicolor")
    }

    fn svg() -> String {
        let path = icons().join(format!("scalable/apps/{APP_ID}.svg"));
        fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
    }

    /// A line of a file, found by its start, so a Windows checkout's CRLF
    /// doesn't matter.
    fn line_starting(text: &str, start: &str) -> Option<String> {
        text.lines()
            .find(|line| line.starts_with(start))
            .map(|line| line[start.len()..].trim().to_owned())
    }

    /// The desktop file names the icon after the app ID, which is also the
    /// desktop file's own name and the name the app sets with
    /// `set_desktop_file_name`, so every way of matching a window to its
    /// launcher ends at the same icon.
    #[test]
    fn the_desktop_file_names_the_icon() {
        let desktop = fs::read_to_string(root().join(format!("data/{APP_ID}.desktop")))
            .expect("the desktop file is named after the app ID");
        assert_eq!(
            line_starting(&desktop, "Icon=").as_deref(),
            Some(APP_ID),
            "the desktop file must say Icon={APP_ID}"
        );
        // X11 windows are matched by their class, which Qt takes from the
        // program's name.
        assert_eq!(
            line_starting(&desktop, "StartupWMClass=").as_deref(),
            Some("kassensturz")
        );
    }

    /// The SVG is the original: square, drawn on a view box, with its license
    /// in a header, and with nothing that fetches or embeds anything else (a
    /// `href` to a web address would load a picture from the network).
    #[test]
    fn the_svg_is_a_square_original() {
        let svg = svg();
        // Written in two pieces so `reuse lint` doesn't read them as this
        // file's own license line.
        assert!(svg.contains(concat!("SPDX-License-", "Identifier: CC-BY-SA-4.0")));
        assert!(svg.contains(concat!(
            "SPDX-FileCopyright",
            "Text: 2026 KassenSturz contributors"
        )));
        let view_box = svg
            .split("viewBox=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("a view box");
        let numbers: Vec<f64> = view_box
            .split_whitespace()
            .map(|n| n.parse().expect("a number"))
            .collect();
        assert_eq!(numbers.len(), 4, "viewBox {view_box}");
        assert_eq!(numbers[2], numbers[3], "the icon must be square");
        assert!(!svg.contains("<image"), "no embedded or linked pictures");
        assert!(!svg.contains("href"), "no link to anything else");
        assert!(!svg.contains("<script"), "no script");
        // A drawing program or a tool that handled the file can add a
        // `<metadata>` block (a provenance manifest of several KB was in the
        // first version). The Flatpak build's AppStream step failed to read
        // such an icon ("icon-file-read-error"), so the SVG has none.
        assert!(!svg.contains("<metadata"), "no metadata block");
        assert!(!svg.contains("xmlns:"), "only the SVG namespace");
    }

    /// Every size is there and really that size (read from the PNG header).
    #[test]
    fn every_png_size_is_drawn() {
        for size in SIZES {
            let path = icons().join(format!("{size}x{size}/apps/{APP_ID}.png"));
            let bytes =
                fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "{}", path.display());
            assert_eq!(&bytes[12..16], b"IHDR", "{}", path.display());
            let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
            let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
            assert_eq!((width, height), (size, size), "{}", path.display());
        }
    }

    /// No size is left over that the script doesn't draw, so a size dropped
    /// from `render-icons.sh` can't linger as a stale file.
    #[test]
    fn no_png_size_is_left_over() {
        let mut found = Vec::new();
        for entry in fs::read_dir(icons()).expect("the icon theme folder") {
            let name = entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned();
            if let Some((width, _)) = name.split_once('x') {
                found.push(width.parse::<u32>().expect("a size folder"));
            }
        }
        found.sort_unstable();
        assert_eq!(found, SIZES);
        let script = fs::read_to_string(root().join("tools/render-icons.sh")).expect("the script");
        let listed = line_starting(&script, "for size in ").expect("the size list");
        let listed: Vec<u32> = listed
            .trim_end_matches("; do")
            .split_whitespace()
            .map(|n| n.parse().expect("a size"))
            .collect();
        assert_eq!(listed, SIZES);
    }

    /// CMake puts the icon folder into the icon theme, and the Flatpak builds
    /// with CMake, so this is the one place that installs it.
    #[test]
    fn cmake_installs_the_icons() {
        let cmake = fs::read_to_string(root().join("CMakeLists.txt")).expect("CMakeLists.txt");
        assert!(
            cmake.lines().any(|line| line.trim()
                == "install(DIRECTORY data/icons/hicolor DESTINATION ${KDE_INSTALL_ICONDIR})"),
            "CMakeLists.txt must install data/icons/hicolor"
        );
    }

    /// The app asks the icon theme for its icon for every window, which is
    /// what an X11 window manager shows.
    #[test]
    fn the_app_gives_its_windows_the_icon() {
        let main = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"))
            .expect("main.rs");
        assert!(main.contains("platform::set_window_icon();"));
    }
}
