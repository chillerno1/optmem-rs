//! The store on disk: LOG.txt holds memory i at byte i*LOG_REC, and
//! TREE/<s> holds block [k*s, (k+1)*s) at byte k*TREE_REC. Each level file
//! is a dense prefix, so its length says how far that level got. Readers
//! never lock; writers serialize on `flock(.lock)`, the same lock the
//! Python tool takes, so the two can share a store.

use crate::block::Block;
use crate::fail::{Fail, at, die};
use crate::knobs::{Knobs, Overrides};
use crate::py::{expanduser, pretty, rstrip, split};
use std::ffi::OsStr;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// Record widths. Fixed, so a memory or a block is found by seeking.
pub const LOG_REC: u64 = 320;
pub const TREE_REC: u64 = 288;

/// One line of LOG.txt.
pub struct Memory {
    pub id: u64,
    pub date: String,
    pub text: String,
}

impl fmt::Display for Memory {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "#{} {} {}", self.id, self.date, self.text)
    }
}

/// An open memory: where it lives, how the tool names itself in what it
/// prints, and the sizes in effect.
pub struct Memo {
    pub dir: PathBuf,
    pub me: String,
    pub knobs: Knobs,
}

/// `$MEMORY_DIR` if set and non-empty, else `~/.optmem/memory`, with `~`
/// expanded and nothing else touched.
pub fn memory_dir() -> PathBuf {
    let raw = std::env::var_os("MEMORY_DIR").filter(|v| !v.is_empty());
    let raw = raw
        .as_deref()
        .map_or(&b"~/.optmem/memory"[..], OsStr::as_bytes);
    PathBuf::from(OsStr::from_bytes(&expanduser(raw)))
}

/// Open the memory for every command but `init`. The directory is only
/// ever created by `init`, so a typo in MEMORY_DIR is an error instead of a
/// second, empty identity.
pub fn open(me: String) -> Result<Memo, Fail> {
    let dir = memory_dir();
    if !dir.is_dir() {
        return die(format!(
            "No memory at {}.\nTo create one, run: {me} init\nTo use an existing one, point MEMORY_DIR at it.",
            dir.to_string_lossy()
        ));
    }
    makedirs(&dir.join("TREE"))?;
    let log = dir.join("LOG.txt");
    if !log.exists() {
        append(&log)?;
    }
    let knobs = overrides(&dir)?.knobs();
    Ok(Memo { dir, me, knobs })
}

/// The knobs this memory sets for itself, read from its `config` file.
pub fn overrides(dir: &Path) -> Result<Overrides, Fail> {
    let p = dir.join("config");
    if !p.exists() {
        return Ok(Overrides::default());
    }
    let mut bytes = vec![];
    open_read(&p).map_err(at(&p))?.read_to_end(&mut bytes)?;
    let shown = pretty(p.as_os_str());
    match String::from_utf8(bytes) {
        Ok(text) => Overrides::parse(&text, &shown),
        Err(_) => die(format!("{shown} is not UTF-8 text.")),
    }
}

/// `os.makedirs(p, exist_ok=True)`, reporting the level that failed.
pub fn makedirs(p: &Path) -> Result<(), Fail> {
    let (mut head, mut tail) = split(p.as_os_str().as_bytes());
    if tail.is_empty() {
        (head, tail) = split(head);
    }
    let head = Path::new(OsStr::from_bytes(head));
    if !head.as_os_str().is_empty() && !tail.is_empty() && !head.exists() {
        match makedirs(head) {
            Err(Fail::Io { err, .. }) if err.kind() == ErrorKind::AlreadyExists => {}
            other => other?,
        }
        if tail == b"." {
            return Ok(());
        }
    }
    match fs::create_dir(p) {
        Err(_) if p.is_dir() => Ok(()),
        other => other.map_err(at(p)),
    }
}

/// Python's `open(p, "a")`.
pub fn append(p: &Path) -> Result<File, Fail> {
    OpenOptions::new()
        .append(true)
        .create(true)
        .open(p)
        .map_err(at(p))
}

/// Python's `open(p, "rb")`, which refuses a directory at open time.
pub fn open_read(p: &Path) -> io::Result<File> {
    let f = File::open(p)?;
    if f.metadata()?.is_dir() {
        return Err(io::Error::from_raw_os_error(libc::EISDIR));
    }
    Ok(f)
}

/// Whole records in a file; a missing file holds none.
fn count(p: &Path, rec: u64) -> Result<u64, Fail> {
    match fs::metadata(p) {
        Ok(m) => Ok(m.len() / rec),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(0),
        Err(e) => Err(at(p)(e)),
    }
}

/// Drop a partial trailing record left by a crash. It was never
/// acknowledged. Callers hold the lock.
fn repair(p: &Path, rec: u64) -> Result<(), Fail> {
    let n = match fs::metadata(p) {
        Ok(m) => m.len(),
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(at(p)(e)),
    };
    if n % rec != 0 {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(p)
            .map_err(at(p))?
            .set_len(n - n % rec)?;
    }
    Ok(())
}

fn pad(text: &str, rec: u64) -> Result<Vec<u8>, Fail> {
    let mut b = text.as_bytes().to_vec();
    if b.len() as u64 > rec - 1 {
        return die(format!(
            "Too long: {} bytes. The record holds {}.",
            b.len(),
            rec - 1
        ));
    }
    b.resize(rec as usize - 1, b' ');
    b.push(b'\n');
    Ok(b)
}

/// Every tree level at or below `t`: 2, 4, 8, ...
fn levels(t: u64) -> impl Iterator<Item = u64> {
    std::iter::successors(Some(2u64), |s| s.checked_mul(2)).take_while(move |&s| s <= t)
}

impl Memo {
    pub fn log_path(&self) -> PathBuf {
        self.dir.join("LOG.txt")
    }

    fn tree_path(&self, size: u128) -> PathBuf {
        self.dir.join("TREE").join(size.to_string())
    }

    pub fn log_len(&self) -> Result<u64, Fail> {
        count(&self.log_path(), LOG_REC)
    }

    fn corrupt(&self, i: u64) -> Fail {
        Fail::Die(format!(
            "{}: memory #{i} is corrupt.",
            pretty(self.log_path().as_os_str())
        ))
    }

    /// Decode a run of log records whose first is memory `first`. They are
    /// sliced as bytes and decoded one by one. A record that is not UTF-8,
    /// or whose id is not ASCII digits, is corrupt.
    fn records(&self, buf: &[u8], first: u64) -> Result<Vec<Memory>, Fail> {
        let parse = |rec: &[u8]| {
            let line = rstrip(std::str::from_utf8(rec).ok()?);
            let (head, rest) = line.split_once(' ').unwrap_or((line, ""));
            let (date, text) = rest.split_once(' ').unwrap_or((rest, ""));
            let mut id = head.chars();
            id.next();
            if !crate::py::is_digits(id.as_str()) {
                return None;
            }
            Some(Memory {
                id: id.as_str().parse().ok()?,
                date: date.into(),
                text: text.into(),
            })
        };
        (first..)
            .zip(buf.chunks_exact(LOG_REC as usize))
            .map(|(i, rec)| parse(rec).ok_or_else(|| self.corrupt(i)))
            .collect()
    }

    /// The memories of a block, in one read.
    pub fn log_block(&self, b: Block) -> Result<Vec<Memory>, Fail> {
        match (u64::try_from(b.lo()), u64::try_from(b.lo() + b.size())) {
            (Ok(lo), Ok(hi)) => self.log_slice(lo, hi),
            _ => Ok(vec![]),
        }
    }

    /// Memories [lo,hi) in one read.
    fn log_slice(&self, lo: u64, hi: u64) -> Result<Vec<Memory>, Fail> {
        let p = self.log_path();
        let mut f = open_read(&p).map_err(at(&p))?;
        f.seek(SeekFrom::Start(lo * LOG_REC))?;
        let mut buf = vec![];
        f.take((hi - lo) * LOG_REC).read_to_end(&mut buf)?;
        self.records(&buf, lo)
    }

    pub fn log_get(&self, i: u64) -> Result<Memory, Fail> {
        self.log_slice(i, i + 1)?
            .pop()
            .ok_or_else(|| self.corrupt(i))
    }

    /// Every memory to the end of the file, streamed, never held whole.
    pub fn log_scan(&self, mut each: impl FnMut(Memory) -> Result<(), Fail>) -> Result<(), Fail> {
        let p = self.log_path();
        let mut f = open_read(&p).map_err(at(&p))?;
        let mut first = 0;
        loop {
            let mut buf = vec![];
            (&mut f).take(LOG_REC * 4096).read_to_end(&mut buf)?;
            if buf.is_empty() {
                return Ok(());
            }
            for m in self.records(&buf, first)? {
                each(m)?;
            }
            first += buf.len() as u64 / LOG_REC;
        }
    }

    /// The summary of a block, in one seek. None if not built yet.
    pub fn tree_get(&self, b: Block) -> Result<Option<String>, Fail> {
        let p = self.tree_path(b.size());
        let mut f = match open_read(&p) {
            Ok(f) => f,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(at(&p)(e)),
        };
        let off = b
            .index()
            .checked_mul(u128::from(TREE_REC))
            .and_then(|o| i64::try_from(o).ok());
        let Some(off) = off else { return Ok(None) };
        f.seek(SeekFrom::Start(off as u64))?;
        let mut rec = vec![];
        f.take(TREE_REC).read_to_end(&mut rec)?;
        match std::str::from_utf8(&rec) {
            Ok(s) => Ok(Some(rstrip(s).to_string()).filter(|s| !s.is_empty())),
            Err(_) => die(format!(
                "The summary of #{b} is corrupt. Run: {} forget {b}",
                self.me
            )),
        }
    }

    fn lock(&self) -> Result<File, Fail> {
        // Append mode, never truncate: the file only carries the lock.
        let lock = append(&self.dir.join(".lock"))?;
        lock.lock()?;
        Ok(lock)
    }

    /// Append memories, `(date, text)` each. The only way LOG.txt changes.
    /// Ids are assigned inside the lock, so two sessions noting at once are
    /// never handed the same id. Returns the first id used.
    pub fn log_append(&self, items: &[(String, String)]) -> Result<u64, Fail> {
        let _lock = self.lock()?;
        let p = self.log_path();
        repair(&p, LOG_REC)?;
        let base = self.log_len()?;
        let mut buf = vec![];
        for (id, (date, text)) in (base..).zip(items) {
            buf.extend(pad(&format!("#{id} {date} {text}"), LOG_REC)?);
        }
        let mut f = append(&p)?;
        f.write_all(&buf)?;
        f.sync_all()?;
        Ok(base)
    }

    /// Write a block. Blocks are built in order, so this appends one record
    /// to one level file, and only if that level has reached this block.
    pub fn tree_put(&self, b: Block, text: &str) -> Result<bool, Fail> {
        let _lock = self.lock()?;
        let p = self.tree_path(b.size());
        repair(&p, TREE_REC)?;
        if u128::from(count(&p, TREE_REC)?) != b.index() {
            return Ok(false);
        }
        let mut f = append(&p)?;
        f.write_all(&pad(text, TREE_REC)?)?;
        f.sync_all()?;
        Ok(true)
    }

    /// Forget a block and every block built from it, by truncating each
    /// level back to that point. The log is never touched. Returns what
    /// went, lowest level first.
    pub fn tree_drop(&self, b: Block) -> Result<Vec<Block>, Fail> {
        let _lock = self.lock()?;
        let mut gone = vec![];
        let Ok(mut size) = u64::try_from(b.size()) else {
            return Ok(gone);
        };
        while size <= self.log_len()? {
            let p = self.tree_path(size.into());
            let n = count(&p, TREE_REC)?;
            if let Ok(k) = u64::try_from(b.lo() / u128::from(size))
                && n > k
            {
                gone.extend((k..n).map(|i| Block::nth(i, size)));
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&p)
                    .map_err(at(&p))?
                    .set_len(k * TREE_REC)?;
            }
            size *= 2;
        }
        Ok(gone)
    }

    /// The first block that can be built and has not been, smallest first.
    /// One stat per level, never a scan.
    pub fn first_pending(&self, t: u64) -> Result<Option<Block>, Fail> {
        for size in levels(t) {
            let have = count(&self.tree_path(size.into()), TREE_REC)?;
            if have < t / size {
                return Ok(Some(Block::nth(have, size)));
            }
        }
        Ok(None)
    }

    /// How many blocks are pending. A level can hold more blocks than `t`
    /// needs, since memories keep arriving, so each level clamps at zero.
    pub fn pending_count(&self, t: u64) -> Result<u64, Fail> {
        let mut n = 0;
        for size in levels(t) {
            n += (t / size).saturating_sub(count(&self.tree_path(size.into()), TREE_REC)?);
        }
        Ok(n)
    }
}
