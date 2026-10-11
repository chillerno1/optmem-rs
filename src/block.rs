//! Blocks of memories, the tiling `wake` prints, and its paging.

use crate::fail::{Fail, die};
use std::fmt;

/// `[lo, hi)`: an aligned power-of-two range of memories. Made only by
/// parsing an id or by tiling. Ids parse into u128, so an id past that is
/// "not a block" (Python's ints are unbounded).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Block {
    lo: u128,
    hi: u128,
}

impl Block {
    /// Block `k` of the level whose blocks hold `size` memories.
    pub fn nth(k: u64, size: u64) -> Block {
        let (k, size) = (u128::from(k), u128::from(size));
        Block {
            lo: k * size,
            hi: (k + 1) * size,
        }
    }

    pub fn lo(self) -> u128 {
        self.lo
    }

    pub fn size(self) -> u128 {
        self.hi - self.lo
    }

    /// Where this block sits in its level file.
    pub fn index(self) -> u128 {
        self.lo / self.size()
    }

    pub fn halves(self) -> [Block; 2] {
        let mid = (self.lo + self.hi) / 2;
        [
            Block {
                lo: self.lo,
                hi: mid,
            },
            Block {
                lo: mid,
                hi: self.hi,
            },
        ]
    }

    /// The first memory, when this block is a single one.
    pub fn single(self) -> Option<u64> {
        if self.size() == 1 {
            u64::try_from(self.lo).ok()
        } else {
            None
        }
    }

    /// `<lo>-<hi>` as wake prints it: inclusive at both ends, and a real
    /// block. Without the shape check, `4-5` and `5-6` read the same record.
    pub fn parse(s: &str) -> Result<Block, Fail> {
        let digits = |(a, b): &(&str, &str)| crate::py::is_digits(a) && crate::py::is_digits(b);
        let Some((a, b)) = s.split_once('-').filter(digits) else {
            return die(format!("'{s}' is not a block id. Copy it from the prompt."));
        };
        let not_block = || {
            die(format!(
                "{s} is not a block. Copy the id printed by wake, like 16-31."
            ))
        };
        let (Ok(lo), Some(hi)) = (
            a.parse::<u128>(),
            b.parse::<u128>().ok().and_then(|b| b.checked_add(1)),
        ) else {
            return not_block();
        };
        if hi < lo || !(hi - lo).is_power_of_two() || hi - lo < 2 || lo % (hi - lo) != 0 {
            return not_block();
        }
        Ok(Block { lo, hi })
    }
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}-{}", self.lo, self.hi - 1)
    }
}

/// Tile `[0,t)` with aligned power-of-two blocks; keep a block whole iff its
/// size is at most `alpha` times its age.
fn tile(t: u64, alpha: f64) -> Vec<(u64, u64)> {
    let root = t.next_power_of_two();
    let (mut out, mut stack) = (vec![], vec![(0, root)]);
    while let Some((lo, hi)) = stack.pop() {
        if lo >= t {
            continue;
        }
        let size = hi - lo;
        if size > 1 && (hi > t || size as f64 > alpha * (t - lo) as f64) {
            let mid = (lo + hi) / 2;
            stack.push((mid, hi));
            stack.push((lo, mid));
        } else {
            out.push((lo, hi));
        }
    }
    out.sort();
    out
}

/// The blocks `wake` prints: about `budget` of them, finest near `t`.
pub fn cover(t: u64, budget: u64) -> Vec<Block> {
    let block = |(lo, hi): (u64, u64)| Block {
        lo: lo.into(),
        hi: hi.into(),
    };
    if t <= budget {
        return (0..t).map(|i| block((i, i + 1))).collect();
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if tile(t, mid).len() as u64 > budget {
            lo = mid
        } else {
            hi = mid
        }
    }
    let mut out = tile(t, hi);
    // Block sizes jump in powers of two, so alpha alone can undershoot the
    // budget. Spend what is left on the present, where detail is worth most.
    while (out.len() as u64) < budget {
        let Some(i) = out.iter().rposition(|(a, b)| b - a > 1) else {
            break;
        };
        let (a, b) = out[i];
        out.splice(i..=i, [(a, (a + b) / 2), ((a + b) / 2, b)]);
    }
    out.into_iter().map(block).collect()
}

/// Split the document into parts that survive any harness's output cap.
pub fn paginate(lines: Vec<String>, max_lines: u64, max_bytes: u64) -> Vec<Vec<String>> {
    let (mut parts, mut cur, mut size) = (vec![], Vec::<String>::new(), 0u64);
    for line in lines {
        let n = line.len() as u64 + 1;
        if !cur.is_empty() && (cur.len() as u64 >= max_lines || size + n > max_bytes) {
            parts.push(std::mem::take(&mut cur));
            size = 0;
        }
        cur.push(line);
        size += n;
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiling(t: u64, budget: u64) -> String {
        let half_open = |b: Block| format!("{}-{}", b.lo, b.hi);
        cover(t, budget)
            .into_iter()
            .map(half_open)
            .collect::<Vec<_>>()
            .join(";")
    }

    fn message(r: Result<Block, Fail>) -> String {
        match r {
            Err(Fail::Die(m)) => m,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn cover_matches_python_literals() {
        assert_eq!(tiling(0, 8), "");
        assert_eq!(tiling(1, 8), "0-1");
        assert_eq!(tiling(5, 8), "0-1;1-2;2-3;3-4;4-5");
        assert_eq!(tiling(8, 8), "0-1;1-2;2-3;3-4;4-5;5-6;6-7;7-8");
        assert_eq!(tiling(9, 8), "0-2;2-3;3-4;4-5;5-6;6-7;7-8;8-9");
        assert_eq!(tiling(13, 8), "0-4;4-6;6-8;8-9;9-10;10-11;11-12;12-13");
        let t97 = tiling(97, 96);
        assert!(t97.starts_with("0-2;2-3;3-4;"), "{t97}");
        assert!(t97.ends_with(";94-95;95-96;96-97"), "{t97}");
        assert_eq!(t97.split(';').count(), 96);
        assert_eq!(tiling(5, u64::MAX), "0-1;1-2;2-3;3-4;4-5");
        assert_eq!(
            tiling(100_000, 8),
            "0-65536;65536-98304;98304-99328;99328-99840;99840-99968;99968-99984;99984-99992;99992-100000"
        );
        assert_eq!(
            tiling(3_000_001, 8),
            "0-2097152;2097152-2621440;2621440-2883584;2883584-2949120;2949120-2981888;\
             2981888-2998272;2998272-2999296;2999296-2999808;2999808-2999936;2999936-3000000;\
             3000000-3000001"
        );
    }

    #[test]
    fn paginate_matches_python() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(
            paginate(s(&["aaaa", "bb", "é", "ccccccccccccccc", "d"]), 3, 8),
            [
                s(&["aaaa", "bb"]),
                s(&["é"]),
                s(&["ccccccccccccccc"]),
                s(&["d"])
            ]
        );
        assert_eq!(
            paginate(s(&["a", "b", "c", "d", "e"]), 2, 100),
            [s(&["a", "b"]), s(&["c", "d"]), s(&["e"])]
        );
        assert!(paginate(vec![], 2, 100).is_empty());
        assert_eq!(
            paginate(s(&["a", "b", "c"]), u64::MAX, u64::MAX),
            [s(&["a", "b", "c"])]
        );
    }

    #[test]
    fn block_ids_parse_inclusive_and_aligned() {
        let ok = |s| Block::parse(s).unwrap();
        assert_eq!((ok("16-31").lo, ok("16-31").hi), (16, 32));
        assert_eq!(ok("00-01").to_string(), "0-1");
        assert_eq!(
            ok("0-170141183460469231731687303715884105727").size(),
            1 << 127
        );
        assert_eq!(
            message(Block::parse("5-6")),
            "5-6 is not a block. Copy the id printed by wake, like 16-31."
        );
        assert_eq!(
            message(Block::parse("4-4")),
            "4-4 is not a block. Copy the id printed by wake, like 16-31."
        );
        assert_eq!(
            message(Block::parse("0-2")),
            "0-2 is not a block. Copy the id printed by wake, like 16-31."
        );
        assert_eq!(
            message(Block::parse("9-2")),
            "9-2 is not a block. Copy the id printed by wake, like 16-31."
        );
        assert_eq!(
            message(Block::parse("a-b")),
            "'a-b' is not a block id. Copy it from the prompt."
        );
        assert_eq!(
            message(Block::parse("1-2-3")),
            "'1-2-3' is not a block id. Copy it from the prompt."
        );
        assert_eq!(
            message(Block::parse("-1")),
            "'-1' is not a block id. Copy it from the prompt."
        );
        assert_eq!(
            message(Block::parse("\u{0660}-\u{0661}")),
            "'\u{0660}-\u{0661}' is not a block id. Copy it from the prompt."
        );
        for huge in [
            "0-1000000000000000000000000000000000000000",
            "99999999999999999999999999999999999999999-1",
        ] {
            assert_eq!(
                message(Block::parse(huge)),
                format!("{huge} is not a block. Copy the id printed by wake, like 16-31.")
            );
        }
    }
}
