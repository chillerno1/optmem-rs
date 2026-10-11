//! The one error type. Every command returns `Result<(), Fail>` and `main`
//! renders it once.

use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum Fail {
    /// The Python tool's `die()`: the message on stderr, exit 1.
    Die(String),
    /// An OSError caught by the Python tool's `main`. `path` is set exactly
    /// where Python's `e.filename` would be: open, stat, mkdir.
    Io {
        path: Option<PathBuf>,
        err: io::Error,
    },
    /// The reason is already on stdout (`Cannot wake`); exit 1.
    Exit,
}

impl From<io::Error> for Fail {
    fn from(err: io::Error) -> Self {
        Fail::Io { path: None, err }
    }
}

pub fn die<T>(msg: impl Into<String>) -> Result<T, Fail> {
    Err(Fail::Die(msg.into()))
}

/// Tag an OS error with the path Python would report.
pub fn at(path: &Path) -> impl FnOnce(io::Error) -> Fail + '_ {
    move |err| Fail::Io {
        path: Some(path.to_path_buf()),
        err,
    }
}

/// The OS message alone, as Python's `e.strerror` gives it, without the
/// ` (os error N)` that Rust appends.
pub fn strerror(err: &io::Error) -> String {
    match err.raw_os_error() {
        // SAFETY: strerror returns a valid C string for any errno.
        Some(code) => unsafe { std::ffi::CStr::from_ptr(libc::strerror(code)) }
            .to_string_lossy()
            .into_owned(),
        None => err.to_string(),
    }
}
