//! The sizes a memory may override in its own `config` file.

use crate::fail::{Fail, die};
use crate::py::{int_str, is_digits, lines, strip};
use crate::store::{LOG_REC, TREE_REC};
use std::ffi::OsStr;
use std::ops::Index;

#[derive(Clone, Copy)]
pub enum Knob {
    WakeLines,
    EntryChars,
    PartChars,
    PartLines,
}

const ALL: [Knob; 4] = [
    Knob::WakeLines,
    Knob::EntryChars,
    Knob::PartChars,
    Knob::PartLines,
];

struct Spec {
    name: &'static str,
    default: u64,
    /// The largest value the knob accepts, if it has one.
    max: Option<u64>,
    what: &'static str,
}

/// In the order every listing prints them, indexed by `Knob`.
const KNOBS: [Spec; 4] = [
    Spec {
        name: "WAKE_LINES",
        default: 96,
        max: None,
        what: "the memory context: how many lines wake prints",
    },
    Spec {
        name: "ENTRY_CHARS",
        default: 280,
        // A memory has to fit both fixed-width records.
        max: Some(if TREE_REC - 8 < LOG_REC - 40 {
            TREE_REC - 8
        } else {
            LOG_REC - 40
        }),
        what: "the longest one memory may be, in bytes",
    },
    Spec {
        name: "PART_CHARS",
        default: 20000,
        max: None,
        what: "output paging: largest part, in bytes",
    },
    Spec {
        name: "PART_LINES",
        default: 500,
        max: None,
        what: "output paging: largest part, in lines",
    },
];

pub const NAMES: &str = "WAKE_LINES, ENTRY_CHARS, PART_CHARS, PART_LINES";

/// The environment variable that sets WAKE_LINES for one process and the
/// commands it starts, leaving the memory's `config` as it is. Named like
/// `MEMORY_DIR`, so the tool's variables share one prefix.
pub const WAKE_ENV: &str = "MEMORY_WAKE_LINES";

impl Knob {
    /// The knob a user-written name means: stripped and upper-cased, as
    /// Python's `k.strip().upper()`.
    pub fn find(name: &str) -> Option<Knob> {
        let name = strip(name).to_uppercase();
        ALL.into_iter().find(|&k| KNOBS[k as usize].name == name)
    }
}

/// The sizes in effect.
pub struct Knobs([u64; 4]);

impl Knobs {
    /// How many lines `wake` prints: `$MEMORY_WAKE_LINES` (as `wake_env`
    /// read it) over the memory's `config`, over the default.
    pub fn wake_lines(&self, env: Option<&str>) -> u64 {
        env.map_or(self[Knob::WakeLines], saturate)
    }
}

impl Index<Knob> for Knobs {
    type Output = u64;
    fn index(&self, k: Knob) -> &u64 {
        &self.0[k as usize]
    }
}

/// The knobs this memory sets for itself, as the digits Python's `int()`
/// prints. A knob it does not set keeps the tool's default, so updating the
/// tool still changes how it behaves.
#[derive(Default)]
pub struct Overrides([Option<String>; 4]);

impl Overrides {
    /// Read the text of a `config` file. `file` is the path as messages show it.
    pub fn parse(text: &str, file: &str) -> Result<Overrides, Fail> {
        let mut out = Overrides::default();
        for (n, line) in lines(text).into_iter().enumerate() {
            let line = strip(line.split('#').next().unwrap_or(""));
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let at = format!("{file} line {}: ", n + 1);
            let Some(knob) = Knob::find(k) else {
                let k = strip(k).to_uppercase();
                return die(format!(
                    "{at}{k} is not a size. Delete the line, or name one of: {NAMES}."
                ));
            };
            out.0[knob as usize] = Some(size(knob, strip(v), &at)?);
        }
        Ok(out)
    }

    /// Set a knob from `memo config NAME=VALUE`; an empty value restores
    /// the default.
    pub fn set(&mut self, k: Knob, v: &str) -> Result<(), Fail> {
        let v = strip(v);
        self.0[k as usize] = if v.is_empty() {
            None
        } else {
            Some(size(k, v, "")?)
        };
        Ok(())
    }

    /// The sizes in effect, each this memory's value or the default.
    pub fn knobs(&self) -> Knobs {
        let value = |k: Knob| {
            self.0[k as usize]
                .as_deref()
                .map_or(KNOBS[k as usize].default, saturate)
        };
        Knobs(ALL.map(value))
    }

    fn shown(&self) -> impl Iterator<Item = (&Spec, String, bool)> {
        KNOBS.iter().zip(&self.0).map(|(spec, over)| {
            (
                spec,
                over.clone().unwrap_or(spec.default.to_string()),
                over.is_some(),
            )
        })
    }

    /// The `config` file: every knob on its own line, commented out unless
    /// this memory overrides it.
    pub fn file_text(&self, me: &str) -> String {
        let mut out = vec![
            "# OptMem sizes for this memory. A commented line means: follow the".to_string(),
            format!("# tool's default. Edit with `{me} config NAME=VALUE`."),
            String::new(),
        ];
        for (spec, v, set) in self.shown() {
            let mark = if set { "" } else { "# " };
            out.push(format!(
                "{mark:<2}{:<12} = {v:<6} # {}",
                spec.name, spec.what
            ));
        }
        out.join("\n") + "\n"
    }

    /// What `memo config` prints.
    pub fn table(&self) -> String {
        let mut out = String::new();
        for (spec, v, set) in self.shown() {
            let note = if set {
                format!(" (default {})", spec.default)
            } else {
                String::new()
            };
            out += &format!("{:<12} {v:<7} {}{note}\n", spec.name, spec.what);
        }
        out
    }
}

/// Validated digits as a count. A value past u64 saturates: every use only
/// compares it with a count that fits u64, so the result is Python's.
fn saturate(v: &str) -> u64 {
    v.parse().unwrap_or(u64::MAX)
}

/// `$MEMORY_WAKE_LINES`, given the raw value: its digits as `int()` prints
/// them, or None when it is unset or blank, which means no override. A bad
/// value is an error, never silently the stored size.
pub fn wake_env(raw: Option<&OsStr>) -> Result<Option<String>, Fail> {
    let Some(raw) = raw else { return Ok(None) };
    let raw = raw.to_string_lossy();
    let v = strip(&raw);
    if v.is_empty() {
        return Ok(None);
    }
    checked(WAKE_ENV, None, v, "").map(Some)
}

/// Validate one knob from `config` or `memo config`.
fn size(k: Knob, v: &str, at: &str) -> Result<String, Fail> {
    let spec = &KNOBS[k as usize];
    checked(spec.name, spec.max, v, at)
}

/// A positive whole number within `max`, as the digits Python's `int()`
/// prints. `name` is what the error calls it.
fn checked(name: &str, max: Option<u64>, v: &str, at: &str) -> Result<String, Fail> {
    if !is_digits(v) || v.bytes().all(|b| b == b'0') {
        return die(format!(
            "{at}{name} must be a positive whole number, not '{v}'."
        ));
    }
    if let Some(max) = max
        && !v.parse::<u64>().is_ok_and(|n| n <= max)
    {
        return die(format!(
            "{at}{name} is at most {max}: a memory has to fit the fixed-width records."
        ));
    }
    Ok(int_str(v).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message<T>(r: Result<T, Fail>) -> String {
        match r {
            Err(Fail::Die(m)) => m,
            _ => panic!("expected a die"),
        }
    }

    #[test]
    fn config_text_parses_like_python() {
        let o = Overrides::parse(
            "wake_lines = 12 # hi\r\nfoo\rentry_chars=0200\n# PART_LINES = 3\n",
            "c",
        )
        .unwrap();
        let k = o.knobs();
        assert_eq!(
            [
                k[Knob::WakeLines],
                k[Knob::EntryChars],
                k[Knob::PartChars],
                k[Knob::PartLines]
            ],
            [12, 200, 20000, 500]
        );
    }

    #[test]
    fn bad_config_lines_name_file_and_line() {
        assert_eq!(
            message(Overrides::parse(
                "wake_lines = 12 # hi\r\nfoo\rPART_CHARS=x\n",
                "/s/config"
            )),
            "/s/config line 3: PART_CHARS must be a positive whole number, not 'x'."
        );
        assert_eq!(
            message(Overrides::parse("ENTRY_CHARS=281\n", "/s/config")),
            "/s/config line 1: ENTRY_CHARS is at most 280: a memory has to fit the fixed-width records."
        );
        assert_eq!(
            message(Overrides::parse(
                "ENTRY_CHARS=99999999999999999999999\n",
                "c"
            )),
            "c line 1: ENTRY_CHARS is at most 280: a memory has to fit the fixed-width records."
        );
        assert_eq!(
            message(Overrides::parse("ß=1\n", "/s/config")),
            "/s/config line 1: SS is not a size. Delete the line, or name one of: WAKE_LINES, ENTRY_CHARS, PART_CHARS, PART_LINES."
        );
        assert_eq!(
            message(Overrides::parse("WAKE_LINES=000", "c")),
            "c line 1: WAKE_LINES must be a positive whole number, not '000'."
        );
        assert_eq!(
            message(Overrides::parse("WAKE_LINES=", "c")),
            "c line 1: WAKE_LINES must be a positive whole number, not ''."
        );
    }

    #[test]
    fn huge_values_keep_their_digits_and_saturate() {
        let o = Overrides::parse("WAKE_LINES=0099999999999999999999\nPART_CHARS=18446744073709551615\nPART_LINES=18446744073709551616\n", "c").unwrap();
        let k = o.knobs();
        assert_eq!(
            [k[Knob::WakeLines], k[Knob::PartChars], k[Knob::PartLines]],
            [u64::MAX; 3]
        );
        assert_eq!(
            o.table(),
            "WAKE_LINES   99999999999999999999 the memory context: how many lines wake prints (default 96)\n\
             ENTRY_CHARS  280     the longest one memory may be, in bytes\n\
             PART_CHARS   18446744073709551615 output paging: largest part, in bytes (default 20000)\n\
             PART_LINES   18446744073709551616 output paging: largest part, in lines (default 500)\n"
        );
        assert!(
            o.file_text("m")
                .contains("\n  WAKE_LINES   = 99999999999999999999 # the memory context")
        );
    }

    #[test]
    fn file_and_table_match_python_layout() {
        let mut o = Overrides::default();
        o.set(Knob::find(" entry_chars").unwrap(), "0200").unwrap();
        o.set(Knob::PartLines, "7").unwrap();
        o.set(Knob::WakeLines, "9").unwrap();
        o.set(Knob::WakeLines, "  ").unwrap();
        assert_eq!(
            o.file_text("/m/memo"),
            "# OptMem sizes for this memory. A commented line means: follow the\n\
             # tool's default. Edit with `/m/memo config NAME=VALUE`.\n\
             \n\
             # WAKE_LINES   = 96     # the memory context: how many lines wake prints\n\
             \x20 ENTRY_CHARS  = 200    # the longest one memory may be, in bytes\n\
             # PART_CHARS   = 20000  # output paging: largest part, in bytes\n\
             \x20 PART_LINES   = 7      # output paging: largest part, in lines\n"
        );
        assert_eq!(
            o.table(),
            "WAKE_LINES   96      the memory context: how many lines wake prints\n\
             ENTRY_CHARS  200     the longest one memory may be, in bytes (default 280)\n\
             PART_CHARS   20000   output paging: largest part, in bytes\n\
             PART_LINES   7       output paging: largest part, in lines (default 500)\n"
        );
    }

    fn env(v: &str) -> Result<Option<String>, Fail> {
        wake_env(Some(OsStr::new(v)))
    }

    #[test]
    fn wake_lines_take_the_environment_then_config_then_default() {
        let stored = Overrides::parse("WAKE_LINES=12\n", "c").unwrap().knobs();
        let default = Overrides::default().knobs();
        let over = env(" 024 ").unwrap();
        assert_eq!(over.as_deref(), Some("24"));
        assert_eq!(stored.wake_lines(over.as_deref()), 24);
        assert_eq!(default.wake_lines(over.as_deref()), 24);
        assert_eq!(stored.wake_lines(None), 12);
        assert_eq!(default.wake_lines(None), 96);
        // Unset and blank both mean no override.
        assert_eq!(wake_env(None).unwrap(), None);
        assert_eq!(env("").unwrap(), None);
        assert_eq!(env(" \t").unwrap(), None);
        // Past u64, as the config file's values do.
        let huge = env("0099999999999999999999").unwrap();
        assert_eq!(huge.as_deref(), Some("99999999999999999999"));
        assert_eq!(stored.wake_lines(huge.as_deref()), u64::MAX);
    }

    #[test]
    fn a_bad_wake_lines_environment_names_the_variable_and_value() {
        for bad in [
            "0", "000", "x", "-3", "+3", "2.5", "1e3", "12 lines", "\u{0661}",
        ] {
            assert_eq!(
                message(env(bad)),
                format!("MEMORY_WAKE_LINES must be a positive whole number, not '{bad}'.")
            );
        }
        let not_utf8 = <OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(b"4\xff");
        assert_eq!(
            message(wake_env(Some(not_utf8))),
            "MEMORY_WAKE_LINES must be a positive whole number, not '4\u{fffd}'."
        );
    }
}
