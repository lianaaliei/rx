use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Assertion {
    Start,
    End,
    WordBoundary,
    NotWordBoundary,
}

#[derive(Clone, Debug)]
pub struct Class {
    pub ranges: Vec<(char, char)>,
    pub negated: bool,
}

impl Class {
    pub fn matches(&self, c: char) -> bool {
        self.ranges.iter().any(|&(a, b)| a <= c && c <= b) != self.negated
    }
}

#[derive(Clone, Debug)]
pub enum Node {
    Empty,
    Char(char),
    Any,
    Class(Class),
    Cat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat { node: Box<Node>, min: u32, max: Option<u32>, greedy: bool },
    Group { node: Box<Node>, index: Option<usize> },
    Assert(Assertion),
}

pub struct Ast {
    pub root: Node,
    /// Number of capture groups including the implicit group 0.
    pub ncaps: usize,
    pub names: Vec<(String, usize)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub msg: String,
    pub pos: usize,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "regex error at {}: {}", self.pos, self.msg)
    }
}
impl std::error::Error for Error {}

const MAX_DEPTH: usize = 200;
const MAX_REPEAT: u32 = 1000;

struct Parser {
    s: Vec<char>,
    i: usize,
    depth: usize,
    ncaps: usize,
    names: Vec<(String, usize)>,
}

type R<T> = Result<T, Error>;

pub fn parse(pattern: &str) -> R<Ast> {
    let mut p = Parser { s: pattern.chars().collect(), i: 0, depth: 0, ncaps: 1, names: Vec::new() };
    let root = p.alt()?;
    if p.i < p.s.len() {
        return Err(p.err("unmatched ')'"));
    }
    Ok(Ast { root, ncaps: p.ncaps, names: p.names })
}

fn shorthand(c: char) -> Option<(Vec<(char, char)>, bool)> {
    let (r, neg): (Vec<(char, char)>, bool) = match c {
        'd' => (vec![('0', '9')], false),
        'D' => (vec![('0', '9')], true),
        'w' => (vec![('0', '9'), ('A', 'Z'), ('_', '_'), ('a', 'z')], false),
        'W' => (vec![('0', '9'), ('A', 'Z'), ('_', '_'), ('a', 'z')], true),
        's' => (vec![(' ', ' '), ('\t', '\r')], false),
        'S' => (vec![(' ', ' '), ('\t', '\r')], true),
        _ => return None,
    };
    Some((r, neg))
}

fn push_u32_range(out: &mut Vec<(char, char)>, lo: u32, hi: u32) {
    // chars exclude the surrogate block D800..=DFFF
    let mut add = |a: u32, b: u32| {
        if a <= b {
            if let (Some(x), Some(y)) = (char::from_u32(a), char::from_u32(b)) {
                out.push((x, y));
            }
        }
    };
    if lo < 0xD800 {
        add(lo, hi.min(0xD7FF));
    }
    if hi > 0xDFFF {
        add(lo.max(0xE000), hi);
    }
}

fn complement(mut r: Vec<(char, char)>) -> Vec<(char, char)> {
    r.sort();
    let mut out = Vec::new();
    let mut next: u32 = 0;
    for (a, b) in r {
        if a as u32 > next {
            push_u32_range(&mut out, next, a as u32 - 1);
        }
        next = next.max(b as u32 + 1);
    }
    if next <= 0x10FFFF {
        push_u32_range(&mut out, next, 0x10FFFF);
    }
    out
}

impl Parser {
    fn err(&self, msg: &str) -> Error {
        Error { msg: msg.to_string(), pos: self.i }
    }
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn alt(&mut self) -> R<Node> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.err("nesting too deep"));
        }
        let mut alts = vec![self.cat()?];
        while self.eat('|') {
            alts.push(self.cat()?);
        }
        self.depth -= 1;
        Ok(if alts.len() == 1 { alts.pop().unwrap() } else { Node::Alt(alts) })
    }

    fn cat(&mut self) -> R<Node> {
        let mut items = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            items.push(self.repeat()?);
        }
        Ok(match items.len() {
            0 => Node::Empty,
            1 => items.pop().unwrap(),
            _ => Node::Cat(items),
        })
    }

    fn repeat(&mut self) -> R<Node> {
        let mut node = self.atom()?;
        loop {
            let (min, max) = match self.peek() {
                Some('*') => { self.i += 1; (0, None) }
                Some('+') => { self.i += 1; (1, None) }
                Some('?') => { self.i += 1; (0, Some(1)) }
                Some('{') => match self.braces() {
                    Some(r) => r?,
                    None => break, // not a quantifier: leave '{' for the next atom
                },
                _ => break,
            };
            if matches!(node, Node::Assert(_)) {
                return Err(self.err("cannot repeat an assertion"));
            }
            let greedy = !self.eat('?');
            node = Node::Repeat { node: Box::new(node), min, max, greedy };
        }
        Ok(node)
    }

    /// Parses {n}, {n,}, {n,m}. `None` if the text is not a valid quantifier (literal brace).
    fn braces(&mut self) -> Option<R<(u32, Option<u32>)>> {
        let save = self.i;
        self.i += 1;
        let num = |p: &mut Parser| -> Option<u32> {
            let st = p.i;
            while p.peek().map_or(false, |c| c.is_ascii_digit()) {
                p.i += 1;
            }
            p.s[st..p.i].iter().collect::<String>().parse().ok()
        };
        let min = match num(self) {
            Some(n) => n,
            None => { self.i = save; return None; }
        };
        let max = if self.eat(',') { num(self) } else { Some(min) };
        if !self.eat('}') {
            self.i = save;
            return None;
        }
        if min > MAX_REPEAT || max.map_or(false, |m| m > MAX_REPEAT) {
            return Some(Err(self.err("repetition count too large")));
        }
        if max.map_or(false, |m| m < min) {
            return Some(Err(self.err("invalid repetition range")));
        }
        Some(Ok((min, max)))
    }

    fn atom(&mut self) -> R<Node> {
        let c = self.peek().ok_or_else(|| self.err("unexpected end"))?;
        self.i += 1;
        match c {
            '(' => self.group(),
            '[' => self.class(),
            '.' => Ok(Node::Any),
            '^' => Ok(Node::Assert(Assertion::Start)),
            '$' => Ok(Node::Assert(Assertion::End)),
            '*' | '+' | '?' => { self.i -= 1; Err(self.err("nothing to repeat")) }
            '\\' => self.escape(),
            c => Ok(Node::Char(c)),
        }
    }

    fn group(&mut self) -> R<Node> {
        let mut index = None;
        if self.eat('?') {
            if self.eat(':') {
                // non-capturing
            } else if self.eat('P') && self.eat('<') {
                let st = self.i;
                while self.peek().map_or(false, |c| c.is_alphanumeric() || c == '_') {
                    self.i += 1;
                }
                let name: String = self.s[st..self.i].iter().collect();
                if name.is_empty() || !self.eat('>') {
                    return Err(self.err("invalid group name"));
                }
                index = Some(self.ncaps);
                self.names.push((name, self.ncaps));
                self.ncaps += 1;
            } else {
                return Err(self.err("unsupported group syntax"));
            }
        } else {
            index = Some(self.ncaps);
            self.ncaps += 1;
        }
        let node = self.alt()?;
        if !self.eat(')') {
            return Err(self.err("missing ')'"));
        }
        Ok(Node::Group { node: Box::new(node), index })
    }

    fn escape(&mut self) -> R<Node> {
        let c = self.peek().ok_or_else(|| self.err("trailing backslash"))?;
        self.i += 1;
        if let Some((ranges, negated)) = shorthand(c) {
            return Ok(Node::Class(Class { ranges, negated }));
        }
        Ok(match c {
            'b' => Node::Assert(Assertion::WordBoundary),
            'B' => Node::Assert(Assertion::NotWordBoundary),
            c => Node::Char(self.escaped_char(c)?),
        })
    }

    fn escaped_char(&self, c: char) -> R<char> {
        Ok(match c {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            c if !c.is_alphanumeric() => c,
            _ => return Err(self.err("unknown escape")),
        })
    }

    fn class(&mut self) -> R<Node> {
        let negated = self.eat('^');
        let mut ranges: Vec<(char, char)> = Vec::new();
        let mut first = true;
        loop {
            let c = self.peek().ok_or_else(|| self.err("missing ']'"))?;
            self.i += 1;
            if c == ']' && !first {
                break;
            }
            first = false;
            let lo = if c == '\\' {
                let e = self.peek().ok_or_else(|| self.err("trailing backslash"))?;
                self.i += 1;
                if let Some((r, neg)) = shorthand(e) {
                    ranges.extend(if neg { complement(r) } else { r });
                    continue;
                }
                self.escaped_char(e)?
            } else {
                c
            };
            if self.peek() == Some('-') && self.s.get(self.i + 1).map_or(false, |&n| n != ']') {
                self.i += 1;
                let mut hi = self.peek().unwrap();
                self.i += 1;
                if hi == '\\' {
                    let e = self.peek().ok_or_else(|| self.err("trailing backslash"))?;
                    self.i += 1;
                    hi = self.escaped_char(e)?;
                }
                if hi < lo {
                    return Err(self.err("invalid class range"));
                }
                ranges.push((lo, hi));
            } else {
                ranges.push((lo, lo));
            }
        }
        Ok(Node::Class(Class { ranges, negated }))
    }
}
