//! The nine commands. Each prints into `out`, which `main` writes at exit.

use crate::block::{Block, cover, paginate};
use crate::fail::{Fail, at, die};
use crate::knobs::{Knob, NAMES, Overrides};
use crate::py::{
    date_shape, int_str, is_digits, plural, pretty, python_regex, real_date, rstrip, strip, today,
};
use crate::store::{Memo, append, makedirs, memory_dir, open_read, overrides};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Blocks up to this many memories compress from the raw log.
const RAW_MAX: u128 = 16;

pub type Command = fn(&Memo, &[String], &mut String) -> Result<(), Fail>;

fn say(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
}

pub fn usage(me: &str) -> String {
    format!(
        "OptMem: a permanent, append-only memory for AI agents.

  {me} init             create this memory; print the setup block.
  {me} wake [part [T]]  read your memory. Run first, every session.
  {me} note \"...\"       record one memory: one short line.
  {me} nap [id \"...\"]   do the pending compressions.
  {me} recall <regex>   search every memory ever recorded.
  {me} zoom <lo>-<hi>   open a tree node: its two halves.
  {me} forget <lo>-<hi> drop a bad summary; nap rebuilds it.
  {me} config [NAME=N]  show this memory's sizes, or change one.
  {me} import <file>    bulk-load dated memories (bootstrap only).

The memories live in ~/.optmem/memory, or in $MEMORY_DIR if set.
See github.com/VictorTaelin/OptMem."
    )
}

fn template(memo: &str, data: &str, chars: u64) -> String {
    format!(
        "## Memory

Your memory is OptMem:
- The tool is `{memo}`
- Your memories are in `{data}`

OptMem outlives every session, compaction, model and vendor change.
Without it you do not know who you are, or what was decided and tried.

### At startup: activating OptMem (mandatory)

Run `{memo} wake` before any other tool call, in every session, and
then do exactly what it prints, to the end of its output.

### While working: register memories (mandatory)

Call `{memo} note \"<1 line, max {chars} bytes>\"` whenever you learn
something new, or something worth keeping happens. That covers a task
worth real effort, a fact or insight the user teaches you, anything you
learn about their life (even indirectly), any event of lasting effect.

Do not register redundant memories.

If `{memo} note` asks a compression: do it before your next action.

Never edit or delete anything under `{data}`: the tool manages it.

### When you need an old memory: search, or navigate

`{memo} recall <regex>` searches every memory, word for word.

Your memories also form a binary tree: #0-1, #2-3 ... exist as one-line
summaries, pairs of those as #0-3, and so on -- every `#a-b` line wake
prints is one node of it. `{memo} zoom <a-b>` opens a node into its
two halves, down to the raw memories.

### If you're a subagent: skip everything above

Parallel sessions on this machine are all you, and may all write memories.
A subagent is not: it must never run `memo`, because it cannot judge what
is already known, and its notes would arrive duplicated and incorrectly.
When you spawn one, write: `You are a subagent. Don't run memo.`
"
    )
}

/// One memory as typed: one line, within ENTRY_CHARS bytes.
fn check<'a>(m: &Memo, text: &'a str) -> Result<&'a str, Fail> {
    let text = strip(text);
    let max = m.knobs[Knob::EntryChars];
    if text.is_empty() {
        return die("Empty. A memory is one line of text.");
    }
    if text.contains(['\n', '\r']) {
        let n = text.matches('\n').count() + 1;
        return die(format!(
            "{n} lines. A memory is one line: merge them, or note them separately."
        ));
    }
    if text.len() as u64 > max {
        return die(format!(
            "Too long: {} bytes, limit {max}. Accented characters cost 2 bytes. Compress it further.",
            text.len()
        ));
    }
    Ok(text)
}

fn blank(m: &Memo, b: Block) -> Fail {
    Fail::Die(format!(
        "The summary of #{b} is blank. Run: {} forget {b}",
        m.me
    ))
}

fn nap_prompt(m: &Memo, b: Block, left: i64) -> Result<String, Fail> {
    let body: Vec<String> = if b.size() <= RAW_MAX {
        m.log_block(b)?.iter().map(|e| format!("  {e}")).collect()
    } else {
        let mut halves = vec![];
        for h in b.halves() {
            // A half is listed only after it settled, so a missing one is a
            // blank record: a corrupt write.
            let s = m.tree_get(h)?.ok_or_else(|| blank(m, h))?;
            halves.push(format!("  #{h} {s}"));
        }
        halves
    };
    let tail = match left {
        0 => String::new(),
        1 => "\n1 compression remains after this one.".into(),
        n => format!("\n{n} compressions remain after this one."),
    };
    Ok(format!(
        "Compress memories #{b} into one line of at most {} bytes.\n\
         Keep what has lasting effect, drop what does not. Invent nothing.\n\n\
         {}\n{tail}\nRun: {} nap {b} \"<your line>\"",
        m.knobs[Knob::EntryChars],
        body.join("\n"),
        m.me
    ))
}

fn next_nap(m: &Memo, t: u64) -> Result<Option<String>, Fail> {
    let Some(b) = m.first_pending(t)? else {
        return Ok(None);
    };
    // Signed: a parallel nap can settle the rest between the two reads.
    let left = m.pending_count(t)? as i64 - 1;
    nap_prompt(m, b, left).map(Some)
}

pub fn init(me: String, args: &[String], out: &mut String) -> Result<(), Fail> {
    if !args.is_empty() {
        return die(format!("usage: {me} init"));
    }
    let dir = memory_dir();
    let fresh = !dir.is_dir();
    makedirs(&dir.join("TREE"))?;
    append(&dir.join("LOG.txt"))?;
    if !dir.join("config").exists() {
        write_config(&dir, &Overrides::default(), &me)?;
    }
    let m = Memo {
        knobs: overrides(&dir)?.knobs(),
        dir,
        me,
    };
    let d = pretty(m.dir.as_os_str());
    if fresh {
        say(
            out,
            &format!("Created {d}: this machine's memory, one identity, forever."),
        );
    } else {
        say(
            out,
            &format!("Found {d}: {}.", plural(m.log_len()?, "memory")),
        );
    }
    say(
        out,
        &format!("Sizes live in {d}/config; the defaults are fine."),
    );
    say(out, "");
    say(
        out,
        "Paste this at the top of your agent's AGENTS.md (or CLAUDE.md), done:",
    );
    say(out, "");
    say(out, rstrip(&template(&m.me, &d, m.knobs[Knob::EntryChars])));
    Ok(())
}

fn write_config(dir: &Path, over: &Overrides, me: &str) -> Result<(), Fail> {
    let p = dir.join("config");
    File::create(&p)
        .map_err(at(&p))?
        .write_all(over.file_text(me).as_bytes())?;
    Ok(())
}

pub fn wake(m: &Memo, args: &[String], out: &mut String) -> Result<(), Fail> {
    let now = m.log_len()?;
    if args.len() > 2 || !args.iter().all(|a| is_digits(a)) {
        return die(format!("usage: {} wake [part [T]]", m.me));
    }
    let k = args.first().map_or("1", |a| int_str(a));
    let t = match args.get(1).map(|a| int_str(a)) {
        None => now,
        Some(t) => match t.parse::<u64>() {
            Ok(t) if t <= now => t,
            _ => {
                return die(format!(
                    "T={t}, but the log holds {}. Run: {} wake",
                    plural(now, "memory"),
                    m.me
                ));
            }
        },
    };
    // A part is rendered as of T, so a note landing between two parts
    // cannot shift a boundary and drop a line.
    if t == 0 {
        say(
            out,
            &format!(
                "No memories yet. Record the first with: {} note \"<one line>\"",
                m.me
            ),
        );
        say(out, "You are awake.");
        return Ok(());
    }
    let mut lines = vec![];
    for b in cover(t, m.knobs[Knob::WakeLines]) {
        if let Some(i) = b.single() {
            lines.push(m.log_get(i)?.to_string());
            continue;
        }
        let mut s = m.tree_get(b)?;
        if s.is_none() {
            if let Some(nap) = next_nap(m, t)? {
                // The only reason to refuse: this document cannot be written
                // without that summary.
                say(
                    out,
                    &format!(
                        "Cannot wake: the memory context needs #{b}, which is not compressed yet.\n\
                     Do the {} below, then run {} wake again.\n",
                        plural(m.pending_count(t)?, "compression"),
                        m.me
                    ),
                );
                say(out, &nap);
                return Err(Fail::Exit);
            }
            s = m.tree_get(b)?; // a parallel session may have paid it
        }
        // Nothing is pending, so the record exists but is blank.
        let s = s.ok_or_else(|| blank(m, b))?;
        lines.push(format!("#{b} {s}"));
    }
    let parts = paginate(lines, m.knobs[Knob::PartLines], m.knobs[Knob::PartChars]);
    let n = parts.len() as u64;
    let Some(k) = k.parse::<u64>().ok().filter(|k| (1..=n).contains(k)) else {
        return die(format!(
            "No part {k}: the memory has {}. Run: {} wake",
            plural(n, "part"),
            m.me
        ));
    };
    if n > 1 {
        say(
            out,
            &format!(
                "Your memory, part {k} of {n}, oldest first ({}).",
                plural(t, "memory")
            ),
        );
    }
    say(out, &parts[k as usize - 1].join("\n"));
    if k < n {
        // The only instruction that survives every harness's truncation, so
        // it says both that the read is unfinished and how to continue it.
        say(
            out,
            &format!("Not awake yet. Run: {} wake {} {t}", m.me, k + 1),
        );
    } else {
        say(out, "You are awake.");
        if let Some(nap) = next_nap(m, t)? {
            say(out, &format!("\n{nap}"));
        }
    }
    Ok(())
}

pub fn note(m: &Memo, args: &[String], out: &mut String) -> Result<(), Fail> {
    let [text] = args else {
        return die(format!(
            "usage: {} note \"<one line, at most {} bytes>\"",
            m.me,
            m.knobs[Knob::EntryChars]
        ));
    };
    let text = check(m, text)?;
    let i = m.log_append(&[(today(), text.to_string())])?;
    say(out, &format!("Saved as #{i}."));
    if let Some(nap) = next_nap(m, i + 1)? {
        say(out, &format!("\n{nap}"));
    }
    Ok(())
}

pub fn nap(m: &Memo, args: &[String], out: &mut String) -> Result<(), Fail> {
    let t = m.log_len()?;
    if !args.is_empty() {
        let [id, text] = args else {
            return die(format!("usage: {} nap <lo>-<hi> \"<one line>\"", m.me));
        };
        let b = Block::parse(id)?;
        let Some(next) = m.first_pending(t)? else {
            say(out, "Nothing left to compress.");
            return Ok(());
        };
        if b != next {
            if m.tree_get(b)?.is_none() {
                return die(format!(
                    "Wrong block: {id}. Blocks are built in order; the next is {next}. Run: {} nap",
                    m.me
                ));
            }
            say(out, &format!("{b} is already settled."));
        } else if m.tree_put(b, check(m, text)?)? {
            say(out, &format!("{b} saved."));
        } else {
            say(out, &format!("{b} was settled or forgotten meanwhile."));
        }
    }
    match next_nap(m, t)? {
        None => say(out, "Nothing left to compress."),
        Some(nap) if args.is_empty() => say(out, &nap),
        Some(nap) => say(out, &format!("\n{nap}")),
    }
    Ok(())
}

pub fn config(m: &Memo, args: &[String], out: &mut String) -> Result<(), Fail> {
    let mut over = overrides(&m.dir)?;
    for a in args {
        let set = a
            .split_once('=')
            .and_then(|(k, v)| Some((Knob::find(k)?, v)));
        let Some((k, v)) = set else {
            return die(format!(
                "usage: {} config [NAME=VALUE ...]   # NAME one of {NAMES}",
                m.me
            ));
        };
        over.set(k, v)?;
    }
    if !args.is_empty() {
        write_config(&m.dir, &over, &m.me)?;
    }
    out.push_str(&over.table());
    Ok(())
}

pub fn forget(m: &Memo, args: &[String], out: &mut String) -> Result<(), Fail> {
    let [id] = args else {
        return die(format!("usage: {} forget <lo>-<hi>", m.me));
    };
    let gone = m.tree_drop(Block::parse(id)?)?;
    let Some(first) = gone.first() else {
        return die(format!("No summary at {id}."));
    };
    say(
        out,
        &format!(
            "Forgot {}, from {first} up. Run: {} nap",
            plural(gone.len() as u64, "summary"),
            m.me
        ),
    );
    Ok(())
}

pub fn recall(m: &Memo, args: &[String], out: &mut String) -> Result<(), Fail> {
    let [pat] = args else {
        return die(format!("usage: {} recall <regex>", m.me));
    };
    let bad = |e: fancy_regex::Error| Fail::Die(format!("bad regex: {e}"));
    let pat = python_regex(pat).map_err(bad)?;
    // One pass, keeping only the newest matches that fit the cap wake
    // respects: a vague regex matches the whole log.
    let (mut hits, mut kept, mut size) = (0u64, VecDeque::new(), 0u64);
    let cap = m.knobs[Knob::PartChars];
    m.log_scan(|e| {
        let line = e.to_string();
        if !pat.is_match(&line).map_err(bad)? {
            return Ok(());
        }
        hits += 1;
        size += line.len() as u64 + 1;
        kept.push_back(line);
        while size > cap {
            size -= kept.pop_front().map_or(0, |l| l.len() as u64 + 1);
        }
        Ok(())
    })?;
    if hits == 0 {
        say(out, "No match.");
        return Ok(());
    }
    let shown = kept.len();
    say(out, &Vec::from(kept).join("\n"));
    if (shown as u64) < hits {
        say(
            out,
            &format!(
                "Newest {shown} of {}. Narrow the regex.",
                plural(hits, "match")
            ),
        );
    } else {
        say(out, &format!("{}.", plural(hits, "match")));
    }
    Ok(())
}

pub fn zoom(m: &Memo, args: &[String], out: &mut String) -> Result<(), Fail> {
    let [id] = args else {
        return die(format!(
            "usage: {} zoom <lo>-<hi>   # a block id, as wake prints them",
            m.me
        ));
    };
    let b = Block::parse(id)?;
    let t = m.log_len()?;
    if b.lo() >= u128::from(t) {
        return die(format!(
            "#{id} is beyond the memory: it holds {}. Run: {} wake",
            plural(t, "memory"),
            m.me
        ));
    }
    for h in b.halves() {
        if h.lo() >= u128::from(t) {
            continue; // the future: no memories there yet
        }
        match h.single() {
            Some(i) => say(out, &m.log_get(i)?.to_string()),
            None => {
                let s = m
                    .tree_get(h)?
                    .unwrap_or_else(|| "not compressed yet".into());
                say(out, &format!("#{h} {s}"));
            }
        }
    }
    Ok(())
}

pub fn import(m: &Memo, args: &[String], out: &mut String) -> Result<(), Fail> {
    let [file] = args else {
        return die(format!(
            "usage: {} import <file>   # lines of 'YYYY-MM-DD <text>'",
            m.me
        ));
    };
    let p = Path::new(file);
    let mut bytes = vec![];
    open_read(p).map_err(at(p))?.read_to_end(&mut bytes)?;
    let Ok(src) = String::from_utf8(bytes) else {
        return die(format!(
            "{} is not UTF-8 text. Convert it, then import again.",
            pretty(p.as_os_str())
        ));
    };
    let n = m.log_len()?;
    let mut last = if n > 0 {
        m.log_get(n - 1)?.date
    } else {
        "0000-00-00".into()
    };
    let max = m.knobs[Knob::EntryChars];
    let mut items = vec![];
    for (i, line) in (1..).zip(crate::py::lines(&src)) {
        if strip(line).is_empty() {
            continue;
        }
        let (date, text) = line.split_once(' ').unwrap_or((line, ""));
        if !date_shape(date) {
            return die(format!(
                "line {i}: expected 'YYYY-MM-DD <text>', got: {line}"
            ));
        }
        if !real_date(date) {
            return die(format!("line {i}: {date} is not a real date."));
        }
        if date < last.as_str() {
            return die(format!(
                "line {i}: date {date} precedes the previous memory ({last})."
            ));
        }
        let text = strip(text);
        if text.is_empty() || text.len() as u64 > max {
            return die(format!("line {i}: {} bytes, limit {max}.", text.len()));
        }
        last = date.to_string();
        items.push((last.clone(), text.to_string()));
    }
    if items.is_empty() {
        return die(format!("{file} has no memories."));
    }
    let base = m.log_append(&items)?;
    let count = items.len() as u64;
    say(
        out,
        &format!(
            "Imported {}, #{base} to #{}.",
            plural(count, "memory"),
            base + count - 1
        ),
    );
    let pending = m.pending_count(m.log_len()?)?;
    if pending > 0 {
        say(
            out,
            &format!(
                "{} pending. Run: {} nap",
                plural(pending, "compression"),
                m.me
            ),
        );
    }
    Ok(())
}
