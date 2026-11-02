// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const SMALL: Limits = Limits {
    max_file_bytes: 10,
    max_depth: 3,
    max_entries: 2,
    max_text_bytes: 4,
};

#[test]
fn each_limit_allows_up_to_and_including_its_value() {
    assert_eq!(SMALL.check_file_size(10), Ok(()));
    assert_eq!(
        SMALL.check_file_size(11),
        Err(ImportError::FileTooLarge { max_bytes: 10 })
    );
    assert_eq!(SMALL.check_depth(3, None), Ok(()));
    assert_eq!(
        SMALL.check_depth(4, Some(9)),
        Err(ImportError::TooDeeplyNested {
            max: 3,
            line: Some(9)
        })
    );
    assert_eq!(SMALL.check_entries(2), Ok(()));
    assert_eq!(
        SMALL.check_entries(3),
        Err(ImportError::TooManyEntries { max: 2 })
    );
    assert_eq!(SMALL.check_text("abcd", None), Ok(()));
    assert_eq!(
        SMALL.check_text("abcde", Some(2)),
        Err(ImportError::TextTooLong {
            max_bytes: 4,
            line: Some(2)
        })
    );
}

#[test]
fn text_is_measured_in_bytes() {
    // Two letters, four bytes in UTF-8.
    assert_eq!(SMALL.check_text("ßß", None), Ok(()));
    assert!(SMALL.check_text("ßßß", None).is_err());
}

#[test]
fn the_defaults_leave_room_for_any_household() {
    let limits = Limits::default();
    assert_eq!(limits.max_file_bytes, 128 * 1024 * 1024);
    assert_eq!(limits.max_depth, 64);
    assert_eq!(limits.max_entries, 1_000_000);
    assert_eq!(limits.max_text_bytes, 16 * 1024);
}

/// Reads `path` on another thread, so a read that waits for a writer fails
/// the test instead of hanging it (M6-B93).
#[cfg(unix)]
fn read_within_time(path: std::path::PathBuf) -> Result<Vec<u8>, ImportError> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(read_file(&path, &Limits::default()));
    });
    receiver
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the read came back instead of waiting")
}

#[cfg(unix)]
#[test]
fn a_named_pipe_or_device_is_refused_without_waiting() {
    let folder = std::env::temp_dir().join(format!("ks-limits-b93-{}", std::process::id()));
    std::fs::create_dir_all(&folder).expect("a folder");
    let pipe = folder.join("pipe.xml");
    let made = std::process::Command::new("mkfifo")
        .arg(&pipe)
        .status()
        .expect("mkfifo runs");
    assert!(made.success());
    let expected = Err(ImportError::Io(ErrorKind::InvalidInput));
    assert_eq!(read_within_time(pipe.clone()), expected);
    // A link to it, and a device that never ends.
    let link = folder.join("link.xml");
    std::os::unix::fs::symlink(&pipe, &link).expect("a link");
    assert_eq!(read_within_time(link), expected);
    assert_eq!(read_within_time("/dev/zero".into()), expected);
    // A folder is no file either; a regular file still reads.
    assert!(read_within_time(folder.clone()).is_err());
    let plain = folder.join("plain.xml");
    std::fs::write(&plain, b"abc").expect("a file");
    assert_eq!(read_within_time(plain), Ok(b"abc".to_vec()));
    std::fs::remove_dir_all(&folder).expect("cleanup");
}
