//! rx: a regex engine built from scratch.
//!
//! pattern --parse--> AST --compile--> bytecode --Pike VM--> matches + captures
//!
//! Supported: literals, `.`, classes `[a-z^]`, `\d\w\s` (and negations), `\b\B`, `^ $`,
//! groups `( )`, `(?: )`, `(?P<name> )`, alternation, `* + ? {n,m}` and lazy `*? +? ??`.
//! Not supported (by design, to keep linear time): backreferences and lookaround.
mod compile;
mod parse;
mod vm;

pub use parse::Error;

pub struct Regex {
    prog: compile::Program,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Match<'t> {
    text: &'t str,
    pub start: usize,
    pub end: usize,
}

impl<'t> Match<'t> {
    pub fn as_str(&self) -> &'t str {
        &self.text[self.start..self.end]
    }
}

pub struct Captures<'t> {
    text: &'t str,
    slots: Vec<Option<usize>>,
    names: Vec<(String, usize)>,
}

impl<'t> Captures<'t> {
    pub fn get(&self, i: usize) -> Option<Match<'t>> {
        match (self.slots.get(2 * i).copied().flatten(), self.slots.get(2 * i + 1).copied().flatten()) {
            (Some(start), Some(end)) => Some(Match { text: self.text, start, end }),
            _ => None,
        }
    }
    pub fn name(&self, name: &str) -> Option<Match<'t>> {
        let (_, i) = self.names.iter().find(|(n, _)| n == name)?;
        self.get(*i)
    }
    pub fn len(&self) -> usize {
        self.slots.len() / 2
    }
}

impl Regex {
    pub fn new(pattern: &str) -> Result<Regex, Error> {
        let ast = parse::parse(pattern)?;
        Ok(Regex { prog: compile::compile(&ast)? })
    }

    pub fn is_match(&self, text: &str) -> bool {
        vm::exec(&self.prog, text, 0).is_some()
    }

    pub fn find<'t>(&self, text: &'t str) -> Option<Match<'t>> {
        self.captures(text).and_then(|c| c.get(0))
    }

    pub fn captures<'t>(&self, text: &'t str) -> Option<Captures<'t>> {
        self.captures_at(text, 0)
    }

    fn captures_at<'t>(&self, text: &'t str, start: usize) -> Option<Captures<'t>> {
        let slots = vm::exec(&self.prog, text, start)?;
        Some(Captures { text, slots, names: self.prog.names.clone() })
    }

    /// All non-overlapping matches, left to right.
    pub fn find_all<'t>(&self, text: &'t str) -> Vec<Match<'t>> {
        self.captures_all(text).iter().filter_map(|c| c.get(0)).collect()
    }

    pub fn captures_all<'t>(&self, text: &'t str) -> Vec<Captures<'t>> {
        let mut out = Vec::new();
        let (mut pos, mut last_end) = (0usize, None);
        while pos <= text.len() {
            let Some(c) = self.captures_at(text, pos) else { break };
            let m = c.get(0).unwrap();
            let step = text[m.end..].chars().next().map_or(1, |c| c.len_utf8());
            if m.start == m.end && Some(m.end) == last_end {
                pos = m.end + step; // empty match glued to the previous match: skip
                continue;
            }
            last_end = Some(m.end);
            pos = if m.start == m.end { m.end + step } else { m.end };
            out.push(c);
        }
        out
    }

    /// Replaces every match. In `rep`, `$1`, `${name}` insert groups and `$$` is a literal `$`.
    pub fn replace_all(&self, text: &str, rep: &str) -> String {
        let mut out = String::new();
        let mut last = 0;
        for c in self.captures_all(text) {
            let m = c.get(0).unwrap();
            out.push_str(&text[last..m.start]);
            expand(rep, &c, &mut out);
            last = m.end;
        }
        out.push_str(&text[last..]);
        out
    }

    /// Human-readable bytecode listing.
    pub fn dump(&self) -> String {
        self.prog.insts.iter().enumerate().map(|(i, x)| format!("{i:04}  {x:?}\n")).collect()
    }
}

fn expand(rep: &str, c: &Captures, out: &mut String) {
    let b: Vec<char> = rep.chars().collect();
    let mut i = 0;
    while i < b.len() {
        if b[i] != '$' || i + 1 >= b.len() {
            out.push(b[i]);
            i += 1;
            continue;
        }
        i += 1;
        if b[i] == '$' {
            out.push('$');
            i += 1;
            continue;
        }
        let braced = b[i] == '{';
        let st = if braced { i + 1 } else { i };
        let mut j = st;
        while j < b.len() && (b[j].is_alphanumeric() || b[j] == '_') {
            j += 1;
        }
        let key: String = b[st..j].iter().collect();
        if braced && b.get(j) == Some(&'}') {
            j += 1;
        } else if braced {
            out.push('$');
            continue; // malformed ${..: emit literally
        }
        let m = key.parse::<usize>().ok().map_or_else(|| c.name(&key), |n| c.get(n));
        if let Some(m) = m {
            out.push_str(m.as_str());
        }
        i = j;
    }
}
