//! The sizes a memory may override in its own `config` file.

use crate::fail::{Fail, die};
use crate::py::{int_str, is_digits, lines, strip};
use crate::store::{LOG_REC, TREE_REC};
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

    /// A value past u64 saturates. Every use only compares it with a count
    /// that fits u64, so the result is the same as Python's.
    pub fn knobs(&self) -> Knobs {
        let value = |k: Knob| {
            self.0[k as usize]
                .as_ref()
                .map_or(KNOBS[k as usize].default, |v| v.parse().unwrap_or(u64::MAX))
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

/// Validate one knob, wherever it came from.
fn size(k: Knob, v: &str, at: &str) -> Result<String, Fail> {
    let spec = &KNOBS[k as usize];
    if !is_digits(v) || v.bytes().all(|b| b == b'0') {
        return die(format!(
            "{at}{} must be a positive whole number, not '{v}'.",
            spec.name
        ));
    }
    if let Some(max) = spec.max
        && !v.parse::<u64>().is_ok_and(|n| n <= max)
    {
        return die(format!(
            "{at}{} is at most {max}: a memory has to fit the fixed-width records.",
            spec.name
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
}
