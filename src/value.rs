//! Run-time values.

use std::fmt;
use std::rc::Rc;

use crate::ast::Node;

/// A value built by a constructor of a user-defined data type.
#[derive(Debug, Clone, PartialEq)]
pub struct Data {
    /// The name of the data type.
    pub ty: String,
    pub ctor: String,
    /// The fields, in the order they were on the stack (the first was deepest).
    pub fields: Vec<Value>,
}

impl Drop for Data {
    /// Drops a long chain of values (such as a list) without recursing once per element.
    fn drop(&mut self) {
        let mut pending: Vec<Value> = std::mem::take(&mut self.fields);
        while let Some(value) = pending.pop() {
            if let Value::Data(rc) = value {
                if let Ok(mut data) = Rc::try_unwrap(rc) {
                    pending.append(&mut data.fields);
                }
            }
        }
    }
}

/// A quotation (`[ ... ]`) together with the local names (`let`) that were visible where it was made.
#[derive(Debug, PartialEq)]
pub struct Closure {
    pub body: Rc<Vec<Node>>,
    pub env: Env,
}

/// The local names in scope: a list that quotations can share.
pub type Env = Option<Rc<Binding>>;

#[derive(Debug, PartialEq)]
pub struct Binding {
    pub name: String,
    pub value: Value,
    pub parent: Env,
}

/// The value of the local name `name`; inner bindings hide outer ones.
pub fn lookup(env: &Env, name: &str) -> Option<Value> {
    let mut cur = env;
    while let Some(binding) = cur {
        if binding.name == name {
            return Some(binding.value.clone());
        }
        cur = &binding.parent;
    }
    None
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    /// An anonymous word (`[ ... ]`) together with the local names it can see.
    Quote(Rc<Closure>),
    Data(Rc<Data>),
}

impl Value {
    pub fn type_name(&self) -> String {
        match self {
            Value::Int(_) => "Int".to_string(),
            Value::Float(_) => "Float".to_string(),
            Value::Str(_) => "String".to_string(),
            Value::Bool(_) => "Bool".to_string(),
            Value::Quote(_) => "Quote".to_string(),
            Value::Data(d) => d.ty.clone(),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(x) => write!(f, "{:?}", x),
            Value::Str(s) => write!(f, "{}", s),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Quote(_) => write!(f, "[...]"),
            Value::Data(d) => {
                write!(f, "{}", d.ctor)?;
                for field in &d.fields {
                    write!(f, " {}", field)?;
                }
                Ok(())
            }
        }
    }
}
