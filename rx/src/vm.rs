//! Pike VM: simulates all NFA threads in lock-step, so matching is O(text * program)
//! no matter how pathological the pattern (no catastrophic backtracking).
use crate::compile::{Inst, Program};
use crate::parse::Assertion;

struct Threads {
    items: Vec<(usize, Vec<Option<usize>>)>,
    seen: Vec<bool>,
}

impl Threads {
    fn new(n: usize) -> Self {
        Threads { items: Vec::new(), seen: vec![false; n] }
    }
    fn clear(&mut self) {
        self.items.clear();
        self.seen.iter_mut().for_each(|s| *s = false);
    }
}

fn is_word(c: Option<char>) -> bool {
    matches!(c, Some(c) if c.is_alphanumeric() || c == '_')
}

fn holds(a: Assertion, text: &str, pos: usize) -> bool {
    match a {
        Assertion::Start => pos == 0,
        Assertion::End => pos == text.len(),
        Assertion::WordBoundary | Assertion::NotWordBoundary => {
            let b = is_word(text[..pos].chars().next_back()) != is_word(text[pos..].chars().next());
            (a == Assertion::WordBoundary) == b
        }
    }
}

/// Follows empty transitions (jumps, splits, saves, assertions). The `seen` set both
/// prevents infinite loops on patterns like `(a*)*` and enforces thread priority.
fn add(prog: &Program, list: &mut Threads, pc: usize, pos: usize, text: &str, caps: &mut Vec<Option<usize>>) {
    if list.seen[pc] {
        return;
    }
    list.seen[pc] = true;
    match &prog.insts[pc] {
        Inst::Jmp(x) => add(prog, list, *x, pos, text, caps),
        Inst::Split(a, b) => {
            add(prog, list, *a, pos, text, caps);
            add(prog, list, *b, pos, text, caps);
        }
        Inst::Save(n) => {
            let old = caps[*n];
            caps[*n] = Some(pos);
            add(prog, list, pc + 1, pos, text, caps);
            caps[*n] = old;
        }
        Inst::Assert(a) => {
            if holds(*a, text, pos) {
                add(prog, list, pc + 1, pos, text, caps);
            }
        }
        _ => list.items.push((pc, caps.clone())),
    }
}

/// Leftmost-first search starting at byte offset `start`. Returns capture slots.
pub fn exec(prog: &Program, text: &str, start: usize) -> Option<Vec<Option<usize>>> {
    let n = prog.insts.len();
    let (mut clist, mut nlist) = (Threads::new(n), Threads::new(n));
    let mut matched: Option<Vec<Option<usize>>> = None;
    let mut pos = start;
    loop {
        if matched.is_none() {
            add(prog, &mut clist, 0, pos, text, &mut vec![None; prog.ncaps * 2]);
        }
        let ch = text[pos..].chars().next();
        let next = pos + ch.map_or(0, |c| c.len_utf8());
        for (pc, mut caps) in clist.items.drain(..) {
            match &prog.insts[pc] {
                Inst::Char(c) => {
                    if ch == Some(*c) {
                        add(prog, &mut nlist, pc + 1, next, text, &mut caps);
                    }
                }
                Inst::Any => {
                    if matches!(ch, Some(c) if c != '\n') {
                        add(prog, &mut nlist, pc + 1, next, text, &mut caps);
                    }
                }
                Inst::Class(cl) => {
                    if matches!(ch, Some(c) if cl.matches(c)) {
                        add(prog, &mut nlist, pc + 1, next, text, &mut caps);
                    }
                }
                Inst::Match => {
                    matched = Some(caps);
                    break; // lower-priority threads are cut off
                }
                _ => {}
            }
        }
        std::mem::swap(&mut clist, &mut nlist);
        nlist.clear();
        if ch.is_none() || (matched.is_some() && clist.items.is_empty()) {
            break;
        }
        pos = next;
    }
    matched
}
