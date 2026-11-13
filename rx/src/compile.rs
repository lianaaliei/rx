use crate::parse::{Assertion, Ast, Class, Error, Node};

#[derive(Clone, Debug)]
pub enum Inst {
    Char(char),
    Any,
    Class(Class),
    /// Try the first target, then the second (priority order = leftmost-first semantics).
    Split(usize, usize),
    Jmp(usize),
    Save(usize),
    Assert(Assertion),
    Match,
}

pub struct Program {
    pub insts: Vec<Inst>,
    pub ncaps: usize,
    pub names: Vec<(String, usize)>,
}

const MAX_INSTS: usize = 100_000;

struct Compiler {
    insts: Vec<Inst>,
}

pub fn compile(ast: &Ast) -> Result<Program, Error> {
    let mut c = Compiler { insts: Vec::new() };
    c.emit(Inst::Save(0))?;
    c.node(&ast.root)?;
    c.emit(Inst::Save(1))?;
    c.emit(Inst::Match)?;
    Ok(Program { insts: c.insts, ncaps: ast.ncaps, names: ast.names.clone() })
}

impl Compiler {
    fn emit(&mut self, i: Inst) -> Result<usize, Error> {
        if self.insts.len() >= MAX_INSTS {
            return Err(Error { msg: "pattern too large".into(), pos: 0 });
        }
        self.insts.push(i);
        Ok(self.insts.len() - 1)
    }

    fn node(&mut self, n: &Node) -> Result<(), Error> {
        match n {
            Node::Empty => {}
            Node::Char(c) => { self.emit(Inst::Char(*c))?; }
            Node::Any => { self.emit(Inst::Any)?; }
            Node::Class(c) => { self.emit(Inst::Class(c.clone()))?; }
            Node::Assert(a) => { self.emit(Inst::Assert(*a))?; }
            Node::Cat(v) => {
                for x in v {
                    self.node(x)?;
                }
            }
            Node::Alt(v) => {
                let mut jumps = Vec::new();
                for (i, x) in v.iter().enumerate() {
                    if i + 1 < v.len() {
                        let split = self.emit(Inst::Split(0, 0))?;
                        self.node(x)?;
                        jumps.push(self.emit(Inst::Jmp(0))?);
                        self.insts[split] = Inst::Split(split + 1, self.insts.len());
                    } else {
                        self.node(x)?;
                    }
                }
                let end = self.insts.len();
                for j in jumps {
                    self.insts[j] = Inst::Jmp(end);
                }
            }
            Node::Group { node, index } => match index {
                Some(k) => {
                    self.emit(Inst::Save(2 * k))?;
                    self.node(node)?;
                    self.emit(Inst::Save(2 * k + 1))?;
                }
                None => self.node(node)?,
            },
            Node::Repeat { node, min, max, greedy } => {
                for _ in 0..*min {
                    self.node(node)?;
                }
                let split = |s: usize, end: usize| if *greedy { Inst::Split(s + 1, end) } else { Inst::Split(end, s + 1) };
                match max {
                    None => {
                        let l = self.emit(Inst::Split(0, 0))?;
                        self.node(node)?;
                        self.emit(Inst::Jmp(l))?;
                        self.insts[l] = split(l, self.insts.len());
                    }
                    Some(m) => {
                        let mut splits = Vec::new();
                        for _ in *min..*m {
                            splits.push(self.emit(Inst::Split(0, 0))?);
                            self.node(node)?;
                        }
                        let end = self.insts.len();
                        for s in splits {
                            self.insts[s] = split(s, end);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
