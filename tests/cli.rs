use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Run `memo` on the memory at `dir`, with `wake_lines` as this process's
/// `MEMORY_WAKE_LINES` (None: unset, whatever the test runner has).
fn run(dir: &Path, wake_lines: Option<&str>, args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_memo"));
    cmd.args(args).env("MEMORY_DIR", dir);
    match wake_lines {
        Some(v) => cmd.env("MEMORY_WAKE_LINES", v),
        None => cmd.env_remove("MEMORY_WAKE_LINES"),
    };
    cmd.output().unwrap()
}

fn memo_at(dir: &Path, wake_lines: Option<&str>, args: &[&str]) -> String {
    let out = run(dir, wake_lines, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn memo(dir: &Path, args: &[&str]) -> String {
    memo_at(dir, None, args)
}

/// A fresh memory in the temp folder, removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("optmem-rs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        memo(&dir, &["init"]);
        Scratch(dir)
    }

    /// `n` memories, every compression done, as a long-lived memory is.
    fn filled(name: &str, n: usize) -> Scratch {
        let s = Scratch::new(name);
        let file = s.0.with_extension("import");
        let lines: String = (0..n).map(|i| format!("2026-01-01 entry {i}\n")).collect();
        std::fs::write(&file, lines).unwrap();
        memo(&s.0, &["import", file.to_str().unwrap()]);
        std::fs::remove_file(&file).unwrap();
        while let Some(id) = next_nap(&memo(&s.0, &["nap"])) {
            memo(&s.0, &["nap", &id, &format!("summary of {id}")]);
        }
        s
    }

    /// Every file of the store and its bytes, to show a command wrote nothing.
    fn snapshot(&self) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = vec![];
        let mut dirs = vec![self.0.clone()];
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    dirs.push(p);
                } else if p.file_name().unwrap() != ".lock" {
                    out.push((p.clone(), std::fs::read(&p).unwrap()));
                }
            }
        }
        out.sort();
        out
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The block a nap prompt asks for, from its `Run: ... nap <id> "..."` line.
fn next_nap(out: &str) -> Option<String> {
    let run = out.lines().rev().find(|l| l.starts_with("Run: "))?;
    let rest = run.split(" nap ").nth(1)?;
    Some(rest.split(' ').next()?.to_string())
}

/// The memory lines of a wake part: everything but its header and footer.
fn body(wake: &str) -> Vec<&str> {
    wake.lines().filter(|l| l.starts_with('#')).collect()
}

#[test]
fn init_note_wake_recall() {
    let s = Scratch::new("cli");
    let dir = &s.0;
    assert_eq!(
        memo(dir, &["note", "the build passes on stable"]),
        "Saved as #0.\n"
    );
    let wake = memo(dir, &["wake"]);
    assert!(wake.contains("the build passes on stable"), "{wake}");
    assert!(wake.ends_with("You are awake.\n"), "{wake}");
    assert!(memo(dir, &["recall", "STABLE"]).ends_with("1 match.\n"));
}

#[test]
fn one_memory_wakes_at_two_sizes_in_two_processes() {
    let s = Scratch::filled("wake-sizes", 300);
    let config = std::fs::read(s.0.join("config")).unwrap();
    let before = s.snapshot();

    let small = memo_at(&s.0, Some("24"), &["wake"]);
    let full = memo(&s.0, &["wake"]);
    assert_eq!(body(&small).len(), 24, "{small}");
    assert_eq!(body(&full).len(), 96, "{full}");
    assert!(small.ends_with("You are awake.\n"), "{small}");
    // Fewer lines reach as far back: coarser summaries of the same history,
    // up to the same newest memory.
    let size = |w: &str| {
        let id = w.split(' ').next().unwrap().trim_start_matches('#');
        let (lo, hi) = id.split_once('-').unwrap();
        hi.parse::<u64>().unwrap() - lo.parse::<u64>().unwrap()
    };
    assert!(size(body(&small)[0]) > size(body(&full)[0]), "{small}");
    assert_eq!(body(&small).last(), body(&full).last());

    // The stored setting is left alone, and so is the rest of the store.
    assert_eq!(std::fs::read(s.0.join("config")).unwrap(), config);
    assert_eq!(s.snapshot(), before);

    // A larger size than the memory's own works the same way.
    assert_eq!(body(&memo_at(&s.0, Some("128"), &["wake"])).len(), 128);
}

#[test]
fn unset_and_blank_wake_lines_change_nothing() {
    let s = Scratch::filled("wake-unset", 300);
    let unset = memo(&s.0, &["wake"]);
    for blank in ["", "  "] {
        assert_eq!(memo_at(&s.0, Some(blank), &["wake"]), unset);
        assert_eq!(
            memo_at(&s.0, Some(blank), &["config"]),
            memo(&s.0, &["config"])
        );
    }
    // The variable outranks the stored size; unset, the stored size rules.
    memo(&s.0, &["config", "WAKE_LINES=40"]);
    assert_eq!(body(&memo(&s.0, &["wake"])).len(), 40);
    assert_eq!(body(&memo_at(&s.0, Some("24"), &["wake"])).len(), 24);
    memo(&s.0, &["config", "WAKE_LINES="]);
    assert_eq!(memo(&s.0, &["wake"]), unset);
}

#[test]
fn every_part_of_one_wake_has_the_same_size() {
    let s = Scratch::filled("wake-parts", 300);
    let whole = memo_at(&s.0, Some("24"), &["wake"]);
    memo(&s.0, &["config", "PART_LINES=10"]);

    let first = memo_at(&s.0, Some("24"), &["wake"]);
    // The line that tells the reader how to go on, as `<tool> wake <k> <T>`.
    let goes_on = |w: &str, next: &str| {
        let last = w.lines().last().unwrap();
        last.starts_with("Not awake yet. Run: ") && last.ends_with(&format!(" wake {next}"))
    };
    assert!(
        first.starts_with("Your memory, part 1 of 3, oldest first (300 memories).\n"),
        "{first}"
    );
    assert!(goes_on(&first, "2 300"), "{first}");
    // A note lands between parts; T keeps the rest of the read as of 300.
    memo(&s.0, &["note", "a note from another session"]);
    let mut lines = body(&first);
    let second = memo_at(&s.0, Some("24"), &["wake", "2", "300"]);
    assert!(goes_on(&second, "3 300"), "{second}");
    let third = memo_at(&s.0, Some("24"), &["wake", "3", "300"]);
    assert!(third.contains("You are awake.\n"), "{third}");
    lines.extend(body(&second));
    lines.extend(body(&third));
    assert_eq!(lines, body(&whole));

    // Unset, the same paging splits the stored 96 lines into ten parts.
    let unset = memo(&s.0, &["wake", "1", "300"]);
    assert!(unset.starts_with("Your memory, part 1 of 10,"), "{unset}");
}

#[test]
fn a_bad_wake_lines_is_an_error_naming_it() {
    let s = Scratch::filled("wake-bad", 20);
    for bad in ["0", "x", "-3", "2.5"] {
        let out = run(&s.0, Some(bad), &["wake"]);
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(out.stdout, b"");
        assert_eq!(
            String::from_utf8(out.stderr).unwrap(),
            format!("MEMORY_WAKE_LINES must be a positive whole number, not '{bad}'.\n")
        );
    }
    // Only wake reads it: other commands still run, and config says why
    // wake will not.
    assert_eq!(
        memo_at(&s.0, Some("x"), &["note", "still works"]),
        "Saved as #20.\n"
    );
    let config = memo_at(&s.0, Some("x"), &["config"]);
    assert!(
        config.ends_with(
            "MEMORY_WAKE_LINES must be a positive whole number, not 'x'. \
             wake refuses to run until it is fixed.\n"
        ),
        "{config}"
    );
    assert!(
        memo_at(&s.0, Some("12"), &["config"])
            .ends_with("MEMORY_WAKE_LINES=12 is set here: wake uses it in place of WAKE_LINES.\n")
    );
}

#[test]
fn a_small_wake_asks_only_for_compressions_already_owed() {
    let s = Scratch::filled("wake-owed", 300);
    // Notes nobody compressed yet: the default wake still reads...
    for i in 0..12 {
        memo(&s.0, &["note", &format!("not napped {i}")]);
    }
    let full = memo(&s.0, &["wake"]);
    assert!(full.contains("You are awake.\n"), "{full}");
    let owed = memo(&s.0, &["nap"]);

    // ...but a 24-line one needs a block among them, so it refuses in the
    // normal way, asks for the same next compression, and writes nothing.
    let before = s.snapshot();
    let out = run(&s.0, Some("24"), &["wake"]);
    assert_eq!(out.status.code(), Some(1));
    let small = String::from_utf8(out.stdout).unwrap();
    assert!(
        small.starts_with("Cannot wake: the memory context needs #"),
        "{small}"
    );
    assert!(small.ends_with(&owed), "{small}");
    assert_eq!(s.snapshot(), before);

    // Once they are paid, it wakes.
    while let Some(id) = next_nap(&memo(&s.0, &["nap"])) {
        memo(&s.0, &["nap", &id, &format!("summary of {id}")]);
    }
    let small = memo_at(&s.0, Some("24"), &["wake"]);
    assert_eq!(body(&small).len(), 24);
    assert!(small.ends_with("You are awake.\n"), "{small}");
}
