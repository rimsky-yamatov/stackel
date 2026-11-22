//! Syntax tree.

use std::collections::HashMap;
use std::rc::Rc;

use crate::lexer::Tok;

/// One element of a word body.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Int(i64),
    Float(f64),
    Str(String),
    /// A call to a word. `col` is 1-based.
    Word {
        name: String,
        line: usize,
        col: usize,
    },
    /// A quotation `[ ... ]`.
    Quote(Rc<Vec<Node>>),
    /// `match | Ctor ... | Ctor ...`: pops a data value and runs the arm of its constructor,
    /// with the constructor's fields pushed on the stack. Always the last node of a body.
    /// `line` and `col` (1-based) locate the `match` word.
    Match {
        arms: Vec<Arm>,
        line: usize,
        col: usize,
    },
    /// `| then ... | else ...`: pops a Bool and runs one branch. Always the last node of a body.
    /// `line` and `col` (1-based) locate the `|`.
    If {
        then_branch: Rc<Vec<Node>>,
        else_branch: Rc<Vec<Node>>,
        line: usize,
        col: usize,
    },
    /// `let a b c in body`: binds the values on top of the stack to local names for the rest
    /// of the body (the last name is the top of the stack). Always the last node of a body.
    /// `line` and `col` (1-based) locate the `let` word.
    Let {
        names: Vec<String>,
        body: Rc<Vec<Node>>,
        line: usize,
        col: usize,
    },
}

/// One arm of a `match`.
#[derive(Debug, Clone, PartialEq)]
pub struct Arm {
    pub ctor: String,
    pub body: Rc<Vec<Node>>,
    /// Position (1-based) of the `|` that starts the arm.
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Def {
    pub name: String,
    pub public: bool,
    pub body: Rc<Vec<Node>>,
    pub line: usize,
}

/// A type signature. The tokens are kept raw until the type checker lands (M2).
#[derive(Debug, Clone, PartialEq)]
pub struct Sig {
    pub name: String,
    pub public: bool,
    pub tokens: Vec<Tok>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Use {
    pub path: String,
    pub source: Option<String>,
    pub names: Vec<String>,
    pub line: usize,
}

/// One constructor of a data type.
#[derive(Debug, Clone, PartialEq)]
pub struct CtorDef {
    pub name: String,
    /// The raw tokens of the field types; they are parsed by the checker.
    pub fields: Vec<Tok>,
    pub line: usize,
}

/// `type Name params = Ctor fields | Ctor fields`
#[derive(Debug, Clone, PartialEq)]
pub struct TypeDef {
    pub name: String,
    pub public: bool,
    pub params: Vec<String>,
    pub ctors: Vec<CtorDef>,
    pub line: usize,
}

/// Where a word comes from, for packages whose effects are limited by `allow` in `stackel.toml`.
#[derive(Debug, Clone, PartialEq)]
pub struct Origin {
    pub package: String,
    /// The effects the package may perform.
    pub allow: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub module: Option<String>,
    pub uses: Vec<Use>,
    pub types: Vec<TypeDef>,
    pub sigs: Vec<Sig>,
    pub defs: Vec<Def>,
    /// Words of packages with an effect ceiling (global word name -> origin).
    pub origins: HashMap<String, Origin>,
}
