//! OptMem: a permanent, append-only memory for AI agents. A drop-in port of
//! the Python `memo`: same commands, same store, same bytes out.

mod block;
mod cmd;
mod fail;
mod knobs;
mod py;
mod store;

use fail::{Fail, die, strerror};
use std::ffi::{OsStr, OsString};
use std::io::{IsTerminal, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::exit;

/// The path the user ran, as Python's `__file__` would hold it for a script:
/// argv[0] if it names a path, else the first match on PATH, the way the
/// shell found it. Symlinks are kept, so a `memo` symlink names itself.
fn tool_path(argv0: Option<&OsStr>) -> OsString {
    let name = argv0.unwrap_or_default();
    if name.as_bytes().contains(&b'/') {
        return name.to_owned();
    }
    if !name.is_empty() {
        let path = std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into());
        for dir in path.as_bytes().split(|&c| c == b':') {
            let found = Path::new(OsStr::from_bytes(dir)).join(name);
            let runnable = std::fs::metadata(&found)
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
            if runnable {
                return found.into_os_string();
            }
        }
    }
    std::env::current_exe()
        .map(|p| p.into_os_string())
        .unwrap_or_default()
}

fn run(me: String, argv: &[OsString], out: &mut String) -> Result<(), Fail> {
    let mut args = vec![];
    for (i, a) in argv.iter().enumerate().skip(1) {
        match a.to_str() {
            Some(a) => args.push(a.to_string()),
            None => return die(format!("Argument {i} is not UTF-8 text.")),
        }
    }
    let Some((name, rest)) = args.split_first() else {
        out.push_str(&cmd::usage(&me));
        out.push('\n');
        return Ok(());
    };
    let command: cmd::Command = match name.as_str() {
        // The only command that may run without an existing memory: it is
        // the one that creates it.
        "init" => return cmd::init(me, rest, out),
        "wake" => cmd::wake,
        "note" => cmd::note,
        "nap" => cmd::nap,
        "recall" => cmd::recall,
        "zoom" => cmd::zoom,
        "forget" => cmd::forget,
        "config" => cmd::config,
        "import" => cmd::import,
        _ => return die(format!("No such command: {name}\n\n{}", cmd::usage(&me))),
    };
    command(&store::open(me)?, rest, out)
}

/// What stderr says for a failure, as the Python tool's `die()` and its
/// `except OSError` print it.
fn render(f: Fail) -> String {
    match f {
        Fail::Exit => String::new(),
        Fail::Die(msg) => msg + "\n",
        Fail::Io { path, err } => {
            let path = path.unwrap_or_else(store::memory_dir);
            format!("{}: {}.\n", py::pretty(path.as_os_str()), strerror(&err))
        }
    }
}

fn main() {
    let argv: Vec<OsString> = std::env::args_os().collect();
    let me = py::pretty(&tool_path(argv.first().map(OsString::as_os_str)));
    let mut out = String::new();
    let failed = run(me, &argv, &mut out).err();
    let code = if failed.is_some() { 1 } else { 0 };
    let mut stderr = std::io::stderr();
    let tty = std::io::stdout().is_terminal();
    let mut stdout = std::io::stdout().lock();
    let mut emit = || -> Result<(), Fail> {
        Ok(stdout
            .write_all(out.as_bytes())
            .and_then(|()| stdout.flush())?)
    };
    // Python's stdout is line-buffered on a terminal and block-buffered on
    // a pipe, where it reaches the reader only at exit, after stderr.
    let mut written = if tty { emit() } else { Ok(()) };
    if let Some(f) = failed {
        let _ = stderr.write_all(render(f).as_bytes());
    }
    if !tty {
        written = emit();
    }
    // A reader that closed early (`memo recall x | head`) gets what Python
    // prints once its output outgrows the 8 KB buffer: `<store>: Broken pipe.`
    if let Err(f) = written {
        let _ = stderr.write_all(render(f).as_bytes());
        exit(1);
    }
    exit(code);
}
