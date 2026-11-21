//! Tree-walking evaluator.
//!
//! The machine has a single value stack. User-defined words are looked up by
//! name; a call in tail position replaces the current body instead of
//! recursing, so tail recursion runs in constant Rust stack space.

use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;

use crate::ast::{Node, Program};
use crate::builtins;
use crate::error::{Error, Result};
use crate::types::count_types;
use crate::value::{lookup, Binding, Closure, Data, Env, Value};

/// A constructor of a data type.
#[derive(Debug, Clone)]
pub(crate) struct Ctor {
    /// The name of the data type it builds.
    pub ty: String,
    /// The number of fields it takes from the stack.
    pub arity: usize,
}

pub struct Interp<'a> {
    pub(crate) stack: Vec<Value>,
    pub(crate) defs: HashMap<String, Rc<Vec<Node>>>,
    pub(crate) ctors: HashMap<String, Ctor>,
    pub(crate) out: &'a mut dyn Write,
    /// The command-line arguments of the program (`arg_count`, `arg`).
    pub(crate) args: Vec<String>,
    /// The output of the running `capture`s, innermost last.
    pub(crate) captures: Vec<Vec<u8>>,
}

fn type_error(expected: &str, found: &Value) -> Error {
    Error::runtime(format!(
        "type error: expected {}, found {}",
        expected,
        found.type_name()
    ))
}

/// Attaches the position of the failing word to a position-less run-time error.
fn with_position(e: Error, word: &str, line: usize, col: usize) -> Error {
    if e.line == 0 && e.exit.is_none() && !e.failure {
        Error::new(format!("{} (in `{}`)", e.msg, word), line, col)
    } else {
        e
    }
}

/// Verifies that every word referenced in `nodes` is a local name (bound by an enclosing `let`),
/// a user word, a constructor or a builtin, and that every `match` arm names a constructor.
fn check_words(
    defs: &HashMap<String, Rc<Vec<Node>>>,
    ctors: &HashMap<String, Ctor>,
    nodes: &[Node],
    locals: &[String],
) -> Result<()> {
    for node in nodes {
        match node {
            Node::Word { name, line, col } => {
                if !locals.contains(name)
                    && !defs.contains_key(name)
                    && !ctors.contains_key(name)
                    && !builtins::is_builtin(name)
                {
                    return Err(Error::new(
                        format!("unknown word `{}`", name),
                        *line,
                        *col,
                    ));
                }
            }
            Node::Quote(q) => check_words(defs, ctors, q, locals)?,
            Node::If { then_branch, else_branch, .. } => {
                check_words(defs, ctors, then_branch, locals)?;
                check_words(defs, ctors, else_branch, locals)?;
            }
            Node::Let { names, body, .. } => {
                let mut inner = locals.to_vec();
                inner.extend(names.iter().cloned());
                check_words(defs, ctors, body, &inner)?;
            }
            Node::Match { arms, .. } => {
                for arm in arms {
                    if !ctors.contains_key(&arm.ctor) {
                        return Err(Error::new(
                            format!("unknown constructor `{}`", arm.ctor),
                            arm.line,
                            arm.col,
                        ));
                    }
                    check_words(defs, ctors, &arm.body, locals)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

impl<'a> Interp<'a> {
    pub fn new(out: &'a mut dyn Write) -> Self {
        Interp {
            stack: Vec::new(),
            defs: HashMap::new(),
            ctors: HashMap::new(),
            out,
            args: Vec::new(),
            captures: Vec::new(),
        }
    }

    /// Writes a line of program output: to the innermost `capture`, or else to the output.
    pub(crate) fn write_line(&mut self, text: &str) -> std::io::Result<()> {
        match self.captures.last_mut() {
            Some(buffer) => {
                buffer.extend_from_slice(text.as_bytes());
                buffer.push(b'\n');
                Ok(())
            }
            None => writeln!(self.out, "{}", text),
        }
    }

    /// Sets the command-line arguments that the program sees.
    pub fn set_args(&mut self, args: Vec<String>) {
        self.args = args;
    }

    /// Registers the definitions of `program` and checks that every word used in a body exists.
    pub fn load(&mut self, program: &Program) -> Result<()> {
        for ty in &program.types {
            for ctor in &ty.ctors {
                let arity = count_types(&ctor.fields).map_err(|m| {
                    Error::new(
                        format!("{} (in constructor `{}`)", m, ctor.name),
                        ctor.line,
                        1,
                    )
                })?;
                if builtins::is_builtin(&ctor.name) || self.ctors.contains_key(&ctor.name) {
                    return Err(Error::new(
                        format!("constructor `{}` is defined more than once", ctor.name),
                        ctor.line,
                        1,
                    ));
                }
                self.ctors.insert(
                    ctor.name.clone(),
                    Ctor {
                        ty: ty.name.clone(),
                        arity,
                    },
                );
            }
        }
        for def in &program.defs {
            if self.ctors.contains_key(&def.name) {
                return Err(Error::new(
                    format!("word `{}` has the same name as a constructor", def.name),
                    def.line,
                    1,
                ));
            }
            if builtins::is_builtin(&def.name) {
                return Err(Error::new(
                    format!("cannot redefine the builtin word `{}`", def.name),
                    def.line,
                    1,
                ));
            }
            if self.defs.contains_key(&def.name) {
                return Err(Error::new(
                    format!("word `{}` is defined more than once", def.name),
                    def.line,
                    1,
                ));
            }
            self.defs.insert(def.name.clone(), Rc::clone(&def.body));
        }
        for sig in &program.sigs {
            if !self.defs.contains_key(&sig.name) {
                return Err(Error::new(
                    format!("type signature for `{}` has no definition", sig.name),
                    sig.line,
                    1,
                ));
            }
        }
        for def in &program.defs {
            check_words(&self.defs, &self.ctors, &def.body, &[])?;
        }
        Ok(())
    }

    /// Runs `main`. The checker guarantees that `main` leaves the stack empty; this is re-checked as a safety net.
    /// Runs the word `name` on the stack as it is.
    pub fn run_word(&mut self, name: &str) -> Result<()> {
        let body = match self.defs.get(name) {
            Some(body) => Rc::clone(body),
            None => return Err(Error::runtime(format!("no word `{}` is defined", name))),
        };
        self.exec(body, None)
    }

    pub fn run_main(&mut self) -> Result<()> {
        let main = match self.defs.get("main") {
            Some(body) => Rc::clone(body),
            None => return Err(Error::runtime("no `main` word is defined")),
        };
        self.exec(main, None)?;
        if !self.stack.is_empty() {
            return Err(Error::runtime(format!(
                "`main` must leave the stack empty, but {} value(s) remain",
                self.stack.len()
            )));
        }
        self.out
            .flush()
            .map_err(|e| Error::runtime(format!("I/O error: {}", e)))?;
        Ok(())
    }

    pub fn stack(&self) -> &[Value] {
        &self.stack
    }

    pub fn push(&mut self, v: Value) {
        self.stack.push(v);
    }

    pub fn pop(&mut self) -> Result<Value> {
        self.stack
            .pop()
            .ok_or_else(|| Error::runtime("stack underflow"))
    }

    pub fn pop_int(&mut self) -> Result<i64> {
        match self.pop()? {
            Value::Int(n) => Ok(n),
            other => Err(type_error("Int", &other)),
        }
    }

    pub fn pop_bool(&mut self) -> Result<bool> {
        match self.pop()? {
            Value::Bool(b) => Ok(b),
            other => Err(type_error("Bool", &other)),
        }
    }

    pub fn pop_str(&mut self) -> Result<String> {
        match self.pop()? {
            Value::Str(s) => Ok(s),
            other => Err(type_error("String", &other)),
        }
    }

    pub fn pop_quote(&mut self) -> Result<Rc<Closure>> {
        match self.pop()? {
            Value::Quote(q) => Ok(q),
            other => Err(type_error("Quote", &other)),
        }
    }

    /// Executes a body. Calls in tail position are turned into a jump.
    /// `env` holds the local names (`let`) that the body can see.
    pub fn exec(&mut self, body: Rc<Vec<Node>>, env: Env) -> Result<()> {
        let mut cur = body;
        let mut env = env;
        loop {
            let mut next: Option<(Rc<Vec<Node>>, Env)> = None;
            let len = cur.len();
            for (i, node) in cur.iter().enumerate() {
                let last = i + 1 == len;
                match node {
                    Node::Int(v) => self.stack.push(Value::Int(*v)),
                    Node::Float(v) => self.stack.push(Value::Float(*v)),
                    Node::Str(s) => self.stack.push(Value::Str(s.clone())),
                    Node::Quote(q) => self.stack.push(Value::Quote(Rc::new(Closure {
                        body: Rc::clone(q),
                        env: env.clone(),
                    }))),
                    Node::Match { arms, line, col } => {
                        let value = self
                            .pop()
                            .map_err(|e| with_position(e, "match", *line, *col))?;
                        let data = match value {
                            Value::Data(d) => d,
                            other => {
                                return Err(Error::new(
                                    format!(
                                        "type error: expected a data value, found {} (in `match`)",
                                        other.type_name()
                                    ),
                                    *line,
                                    *col,
                                ))
                            }
                        };
                        let arm = match arms.iter().find(|a| a.ctor == data.ctor) {
                            Some(a) => a,
                            None => {
                                return Err(Error::new(
                                    format!("no `match` arm for the constructor `{}`", data.ctor),
                                    *line,
                                    *col,
                                ))
                            }
                        };
                        for field in &data.fields {
                            self.stack.push(field.clone());
                        }
                        next = Some((Rc::clone(&arm.body), env.clone()));
                        break;
                    }
                    Node::If { then_branch, else_branch, .. } => {
                        let cond = self
                            .pop_bool()
                            .map_err(|e| Error::runtime(format!("{} (in guard)", e.msg)))?;
                        let branch = if cond { then_branch } else { else_branch };
                        next = Some((Rc::clone(branch), env.clone()));
                        break;
                    }
                    Node::Let { names, body, line, col } => {
                        let n = names.len();
                        if self.stack.len() < n {
                            return Err(Error::new("stack underflow (in `let`)", *line, *col));
                        }
                        let at = self.stack.len() - n;
                        let values = self.stack.split_off(at);
                        let mut inner = env.clone();
                        for (name, value) in names.iter().zip(values) {
                            inner = Some(Rc::new(Binding {
                                name: name.clone(),
                                value,
                                parent: inner,
                            }));
                        }
                        next = Some((Rc::clone(body), inner));
                        break;
                    }
                    Node::Word { name, line, col } => {
                        if let Some(v) = lookup(&env, name) {
                            self.stack.push(v);
                        } else if let Some(def) = self.defs.get(name) {
                            let def = Rc::clone(def);
                            if last {
                                next = Some((def, None));
                                break;
                            }
                            self.exec(def, None)?;
                        } else if name == "apply" {
                            let q = self
                                .pop_quote()
                                .map_err(|e| with_position(e, name, *line, *col))?;
                            if last {
                                next = Some((Rc::clone(&q.body), q.env.clone()));
                                break;
                            }
                            self.exec(Rc::clone(&q.body), q.env.clone())?;
                        } else if let Some(ctor) = self.ctors.get(name).cloned() {
                            if self.stack.len() < ctor.arity {
                                return Err(Error::new(
                                    format!("stack underflow (in `{}`)", name),
                                    *line,
                                    *col,
                                ));
                            }
                            let at = self.stack.len() - ctor.arity;
                            let fields = self.stack.split_off(at);
                            self.stack.push(Value::Data(Rc::new(Data {
                                ty: ctor.ty,
                                ctor: name.clone(),
                                fields,
                            })));
                        } else {
                            match builtins::call(self, name) {
                                Ok(true) => {}
                                Ok(false) => {
                                    return Err(Error::new(
                                        format!("unknown word `{}`", name),
                                        *line,
                                        *col,
                                    ))
                                }
                                Err(e) => return Err(with_position(e, name, *line, *col)),
                            }
                        }
                    }
                }
            }
            match next {
                Some((b, e)) => {
                    cur = b;
                    env = e;
                }
                None => return Ok(()),
            }
        }
    }
}
