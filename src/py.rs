//! The pieces of Python's behavior that the output depends on, reproduced
//! exactly. Paths are raw bytes, as on Unix, so a store whose path is not
//! UTF-8 still opens. Digits are ASCII only, by decision.

use std::ffi::{CStr, CString, OsStr};
use std::os::unix::ffi::OsStrExt;

/// `str.isspace()`: Unicode White_Space plus the four ASCII separators
/// U+001C to U+001F, which Python counts and Rust does not.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

pub fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

pub fn rstrip(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

pub fn plural(n: u64, word: &str) -> String {
    if n == 1 {
        return format!("1 {word}");
    }
    let word = if let Some(stem) = word.strip_suffix('y') {
        format!("{stem}ie")
    } else if word.ends_with(['s', 'h', 'x']) {
        format!("{word}e")
    } else {
        word.to_string()
    };
    format!("{n} {word}s")
}

/// `s.isdigit()`, restricted to ASCII.
pub fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// A digit string as Python's `"%d" % int(s)` prints it.
pub fn int_str(s: &str) -> &str {
    let t = s.trim_start_matches('0');
    if t.is_empty() { &s[s.len() - 1..] } else { t }
}

/// The lines of a text file opened with universal newlines: `\n`, `\r\n`
/// and `\r` each end a line, and nothing else does. Terminators are dropped.
pub fn lines(s: &str) -> Vec<&str> {
    let mut out = vec![];
    let (b, mut start, mut i) = (s.as_bytes(), 0, 0);
    while i < b.len() {
        if b[i] == b'\n' || b[i] == b'\r' {
            out.push(&s[start..i]);
            i += if b[i] == b'\r' && b.get(i + 1) == Some(&b'\n') {
                2
            } else {
                1
            };
            start = i;
        } else {
            i += 1;
        }
    }
    if start < b.len() {
        out.push(&s[start..]);
    }
    out
}

/// `re.fullmatch(r"\d{4}-\d{2}-\d{2}", s)` with ASCII digits.
pub fn date_shape(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

/// `strptime(s, "%Y-%m-%d")` succeeds, for an `s` of the right shape.
pub fn real_date(s: &str) -> bool {
    let n = |r: std::ops::Range<usize>| s[r].parse::<u32>().unwrap_or(0);
    let (y, m, d) = (n(0..4), n(5..7), n(8..10));
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    y >= 1 && (1..=days).contains(&d)
}

unsafe extern "C" {
    fn tzset();
}

/// `datetime.date.today().isoformat()`: the local date.
pub fn today() -> String {
    // SAFETY: time, tzset and localtime_r only write the tm we own.
    let tm = unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        tzset();
        libc::localtime_r(&t, &mut tm);
        tm
    };
    format!(
        "{:04}-{:02}-{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday
    )
}

/// `re.compile(pat, re.I)`. Python's `\Z` (end of text) is `\z` in Rust's
/// dialect.
pub fn python_regex(pat: &str) -> Result<fancy_regex::Regex, fancy_regex::Error> {
    let mut out = String::with_capacity(pat.len());
    let mut it = pat.chars();
    while let Some(c) = it.next() {
        out.push(c);
        if c == '\\' {
            match it.next() {
                Some('Z') => out.push('z'),
                Some(n) => out.push(n),
                None => {}
            }
        }
    }
    fancy_regex::RegexBuilder::new(&out)
        .case_insensitive(true)
        .build()
}

// ---------------------------------------------------------------- paths

/// `posixpath.normpath`.
pub fn normpath(p: &[u8]) -> Vec<u8> {
    if p.is_empty() {
        return b".".to_vec();
    }
    let slashes = match p.iter().take_while(|&&c| c == b'/').count() {
        0 => 0,
        2 => 2,
        _ => 1,
    };
    let mut comps: Vec<&[u8]> = vec![];
    for c in p.split(|&c| c == b'/') {
        match c {
            b"" | b"." => {}
            b".." if slashes > 0 && comps.is_empty() => {}
            b".." if comps.last().is_some_and(|l| *l != b"..") => {
                comps.pop();
            }
            _ => comps.push(c),
        }
    }
    let mut out = vec![b'/'; slashes];
    out.extend(comps.join(&b'/'));
    if out.is_empty() { b".".to_vec() } else { out }
}

/// `posixpath.abspath`, against an explicit working directory.
pub fn abspath(p: &[u8], cwd: &[u8]) -> Vec<u8> {
    if p.starts_with(b"/") || cwd.is_empty() {
        return normpath(p);
    }
    let mut joined = cwd.to_vec();
    if !joined.ends_with(b"/") {
        joined.push(b'/');
    }
    joined.extend_from_slice(p);
    normpath(&joined)
}

/// `posixpath.split`.
pub fn split(p: &[u8]) -> (&[u8], &[u8]) {
    let i = p.iter().rposition(|&c| c == b'/').map_or(0, |i| i + 1);
    let (head, tail) = p.split_at(i);
    if head.iter().all(|&c| c == b'/') {
        return (head, tail);
    }
    let end = head.iter().rposition(|&c| c != b'/').map_or(0, |i| i + 1);
    (&head[..end], tail)
}

/// A home directory from the password database: `None` asks for the
/// current user's, `Some(name)` for a named user's.
type Passwd = dyn Fn(Option<&[u8]>) -> Option<Vec<u8>>;

/// `posixpath.expanduser`, with `$HOME` and the password database passed in.
pub fn expanduser_with(path: &[u8], home: Option<&[u8]>, passwd: &Passwd) -> Vec<u8> {
    if !path.starts_with(b"~") {
        return path.to_vec();
    }
    let i = path[1..]
        .iter()
        .position(|&c| c == b'/')
        .map_or(path.len(), |i| i + 1);
    let userhome = if i == 1 {
        home.map(<[u8]>::to_vec).or_else(|| passwd(None))
    } else {
        passwd(Some(&path[1..i]))
    };
    let Some(userhome) = userhome else {
        return path.to_vec();
    };
    let end = userhome
        .iter()
        .rposition(|&c| c != b'/')
        .map_or(0, |i| i + 1);
    let mut out = userhome[..end].to_vec();
    out.extend_from_slice(&path[i..]);
    if out.is_empty() { b"/".to_vec() } else { out }
}

pub fn expanduser(path: &[u8]) -> Vec<u8> {
    let home = std::env::var_os("HOME");
    expanduser_with(path, home.as_ref().map(|h| h.as_bytes()), &passwd_home)
}

fn passwd_home(name: Option<&[u8]>) -> Option<Vec<u8>> {
    let name = match name {
        Some(n) => Some(CString::new(n).ok()?),
        None => None,
    };
    let mut buf = vec![0 as libc::c_char; 1 << 14];
    // SAFETY: the reentrant lookups write only into pwd and buf, which we
    // own, and pw_dir points into buf when the lookup succeeds.
    unsafe {
        let mut pwd: libc::passwd = std::mem::zeroed();
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        let rc = match &name {
            None => libc::getpwuid_r(
                libc::getuid(),
                &mut pwd,
                buf.as_mut_ptr(),
                buf.len(),
                &mut found,
            ),
            Some(n) => libc::getpwnam_r(
                n.as_ptr(),
                &mut pwd,
                buf.as_mut_ptr(),
                buf.len(),
                &mut found,
            ),
        };
        if rc != 0 || found.is_null() || pwd.pw_dir.is_null() {
            return None;
        }
        Some(CStr::from_ptr(pwd.pw_dir).to_bytes().to_vec())
    }
}

/// `pretty()` from the Python tool, against an explicit cwd and home: the
/// absolute path without resolving symlinks, with the home folded to `~`.
pub fn pretty_with(p: &[u8], cwd: &[u8], home: &[u8]) -> String {
    let p = abspath(p, cwd);
    let folded = match p.strip_prefix(home) {
        Some(rest) if rest.starts_with(b"/") => [b"~", rest].concat(),
        _ => p,
    };
    String::from_utf8_lossy(&folded).into_owned()
}

pub fn pretty(p: &OsStr) -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    pretty_with(p.as_bytes(), cwd.as_os_str().as_bytes(), &expanduser(b"~"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PY_SPACE: [u32; 29] = [
        0x9, 0xa, 0xb, 0xc, 0xd, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x85, 0xa0, 0x1680, 0x2000, 0x2001,
        0x2002, 0x2003, 0x2004, 0x2005, 0x2006, 0x2007, 0x2008, 0x2009, 0x200a, 0x2028, 0x2029,
        0x202f, 0x205f, 0x3000,
    ];

    #[test]
    fn space_matches_python_isspace_over_every_char() {
        let ours: Vec<u32> = (0..=0x10ffff)
            .filter_map(char::from_u32)
            .filter(|&c| is_space(c))
            .map(u32::from)
            .collect();
        assert_eq!(ours, PY_SPACE);
    }

    #[test]
    fn strip_uses_python_whitespace() {
        assert_eq!(strip(" \x1c a b\u{3000}\u{85}"), "a b");
        assert_eq!(strip("x\u{200b} "), "x\u{200b}");
        assert_eq!(rstrip("\x1fa\x1f  "), "\x1fa");
    }

    #[test]
    fn plural_matches_python() {
        assert_eq!(plural(0, "memory"), "0 memories");
        assert_eq!(plural(1, "memory"), "1 memory");
        assert_eq!(plural(2, "summary"), "2 summaries");
        assert_eq!(plural(3, "match"), "3 matches");
        assert_eq!(plural(4, "part"), "4 parts");
        assert_eq!(plural(2, "box"), "2 boxes");
        assert_eq!(plural(2, "bus"), "2 buses");
    }

    #[test]
    fn digits_are_ascii_and_print_like_python_int() {
        assert!(is_digits("0042"));
        assert!(!is_digits(""));
        assert!(!is_digits("\u{0663}"));
        assert!(!is_digits("+1"));
        assert_eq!(int_str("007"), "7");
        assert_eq!(int_str("000"), "0");
        assert_eq!(
            int_str("99999999999999999999999"),
            "99999999999999999999999"
        );
    }

    #[test]
    fn lines_split_on_universal_newlines_only() {
        assert_eq!(lines("a\r\nb\rc\nd"), ["a", "b", "c", "d"]);
        assert_eq!(lines(""), Vec::<&str>::new());
        assert_eq!(lines("a\n"), ["a"]);
        assert_eq!(lines("a\n\n"), ["a", ""]);
        assert_eq!(lines("\r\r\n"), ["", ""]);
        assert_eq!(
            lines("x\x0by\u{85}z\u{2028}w\n"),
            ["x\x0by\u{85}z\u{2028}w"]
        );
        assert_eq!(lines("\u{feff}a\n"), ["\u{feff}a"]);
    }

    #[test]
    fn dates_validate_like_strptime() {
        let real = |s| date_shape(s) && real_date(s);
        for s in [
            "2024-02-29",
            "0001-01-01",
            "9999-12-31",
            "2024-04-30",
            "2000-02-29",
        ] {
            assert!(real(s), "{s}");
        }
        for s in [
            "2023-02-29",
            "0000-01-01",
            "2024-13-01",
            "2024-00-10",
            "2024-04-31",
            "1900-02-29",
        ] {
            assert!(!real(s), "{s}");
        }
        assert!(!date_shape("2024-1-01"));
        assert!(!date_shape("2024-01-01x"));
        assert!(!date_shape("2024\u{2013}01-01"));
    }

    #[test]
    fn regex_end_anchor_translates_but_escaped_backslash_stays() {
        let hit = |pat, s| python_regex(pat).unwrap().is_match(s).unwrap();
        assert!(hit(r"memo\Z", "a memo"));
        assert!(!hit(r"memo\Z", "a memo\n"));
        assert!(hit(r"a\\Z", r"a\Z"));
        assert!(hit(r"a\\\Z", r"a\"));
        assert!(!hit(r"a\\\Z", r"a\Z"));
        assert_eq!(
            python_regex(r"x\").unwrap_err().to_string(),
            "Parsing error at position 1: Backslash without following character"
        );
    }

    #[test]
    fn regex_ignores_case_inside_assertions_and_backrefs() {
        let hit = |pat, s| python_regex(pat).unwrap().is_match(s).unwrap();
        assert!(hit(r"for mem\b", "Rust for MeM (+"));
        assert!(hit(r"opt(?=mem)", "OptMem"));
        assert!(hit(r"(?<!x)mem", "OptMem"));
        assert!(hit(r"(?P<x>o)(?P=x)", "oO"));
        assert!(!hit(r"(?P<x>o)(?P=x)", "oa"));
    }

    #[test]
    fn normpath_matches_python() {
        let cases: [(&str, &str); 17] = [
            ("", "."),
            (".", "."),
            ("/", "/"),
            ("//", "//"),
            ("///", "/"),
            ("////a", "/a"),
            ("//a/b", "//a/b"),
            ("a/../..", ".."),
            ("../a", "../a"),
            ("/../a", "/a"),
            ("a/./b//c/", "a/b/c"),
            ("a/b/../../..", ".."),
            ("/a/b/../../../c", "/c"),
            ("//..", "//"),
            ("x/..", "."),
            ("./", "."),
            ("a//", "a"),
        ];
        for (p, want) in cases {
            assert_eq!(normpath(p.as_bytes()), want.as_bytes(), "{p:?}");
        }
    }

    #[test]
    fn abspath_joins_cwd_then_normalizes() {
        assert_eq!(abspath(b"../x/./y", b"/u/me"), b"/u/x/y");
        assert_eq!(abspath(b"/a//b/", b"/u/me"), b"/a/b");
        assert_eq!(abspath(b"", b"/u/me/"), b"/u/me");
    }

    #[test]
    fn split_matches_python() {
        assert_eq!(split(b"a/b/c"), (&b"a/b"[..], &b"c"[..]));
        assert_eq!(split(b"a/b/"), (&b"a/b"[..], &b""[..]));
        assert_eq!(split(b"a"), (&b""[..], &b"a"[..]));
        assert_eq!(split(b"//"), (&b"//"[..], &b""[..]));
        assert_eq!(split(b"/a"), (&b"/"[..], &b"a"[..]));
        assert_eq!(split(b"//a//b"), (&b"//a"[..], &b"b"[..]));
    }

    #[test]
    fn expanduser_matches_python() {
        let pw = |n: Option<&[u8]>| match n {
            None => Some(b"/home/me".to_vec()),
            Some(b"root") => Some(b"/var/root".to_vec()),
            Some(_) => None,
        };
        let ex = |p: &str, home: Option<&str>| {
            String::from_utf8(expanduser_with(p.as_bytes(), home.map(str::as_bytes), &pw)).unwrap()
        };
        assert_eq!(ex("~", Some("/h/u")), "/h/u");
        assert_eq!(ex("~/", Some("/h/u")), "/h/u/");
        assert_eq!(ex("~//x", Some("/h/u")), "/h/u//x");
        assert_eq!(ex("~/a", Some("/h/u/")), "/h/u/a");
        assert_eq!(ex("x~", Some("/h/u")), "x~");
        assert_eq!(ex("/~", Some("/h/u")), "/~");
        assert_eq!(ex("~nosuchuserxyz/a", Some("/h/u")), "~nosuchuserxyz/a");
        assert_eq!(ex("~root/a", Some("/h/u")), "/var/root/a");
        assert_eq!(ex("~root", Some("/h/u")), "/var/root");
        assert_eq!(ex("~", Some("")), "/");
        assert_eq!(ex("~/a", Some("")), "/a");
        assert_eq!(ex("~/a", Some("/")), "/a");
        assert_eq!(ex("~/a", None), "/home/me/a");
    }

    #[test]
    fn pretty_folds_home_without_resolving() {
        assert_eq!(
            pretty_with(b"/h/u/.optmem/memo", b"/tmp", b"/h/u"),
            "~/.optmem/memo"
        );
        assert_eq!(pretty_with(b"memo", b"/h/u/bin", b"/h/u"), "~/bin/memo");
        assert_eq!(pretty_with(b"../u2/memo", b"/h/u", b"/h/u"), "/h/u2/memo");
        assert_eq!(pretty_with(b"/h/u", b"/", b"/h/u"), "/h/u");
        assert_eq!(pretty_with(b"/h/user/x", b"/", b"/h/u"), "/h/user/x");
        assert_eq!(pretty_with(b"//x/y", b"/", b"/"), "~/x/y");
    }
}
