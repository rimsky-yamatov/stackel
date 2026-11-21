//! Arithmetic, comparison and logic words.
//!
//! Binary words take the deeper value as the first operand: `10 3 sub` is `10 - 3`.
//! There are no implicit conversions: both operands must have the same type.

use std::cmp::Ordering;

use crate::error::{Error, Result};
use crate::eval::Interp;
use crate::value::Value;

pub const WORDS: &[&str] = &[
    "add", "sub", "mul", "div", "mod", "neg", "lt", "gt", "le", "ge", "eq", "ne", "and", "or",
    "not",
];

pub fn call(vm: &mut Interp, name: &str) -> Result<bool> {
    match name {
        "add" | "sub" | "mul" | "div" | "mod" => binary_arith(vm, name)?,
        "lt" | "gt" | "le" | "ge" => compare(vm, name)?,
        "eq" | "ne" => equality(vm, name)?,
        "and" | "or" => {
            let b = vm.pop_bool()?;
            let a = vm.pop_bool()?;
            let r = if name == "and" { a && b } else { a || b };
            vm.push(Value::Bool(r));
        }
        "not" => {
            let a = vm.pop_bool()?;
            vm.push(Value::Bool(!a));
        }
        "neg" => match vm.pop()? {
            Value::Int(n) => match n.checked_neg() {
                Some(r) => vm.push(Value::Int(r)),
                None => return Err(Error::runtime("integer overflow")),
            },
            Value::Float(x) => vm.push(Value::Float(-x)),
            other => {
                return Err(Error::runtime(format!(
                    "type error: `neg` expects an Int or a Float, found {}",
                    other.type_name()
                )))
            }
        },
        _ => return Ok(false),
    }
    Ok(true)
}

fn binary_arith(vm: &mut Interp, name: &str) -> Result<()> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    let r = match (a, b) {
        (Value::Int(x), Value::Int(y)) => Value::Int(int_op(name, x, y)?),
        (Value::Float(x), Value::Float(y)) => Value::Float(float_op(name, x, y)),
        (a, b) => {
            return Err(Error::runtime(format!(
                "type error: `{}` expects two Ints or two Floats, found {} and {}",
                name,
                a.type_name(),
                b.type_name()
            )))
        }
    };
    vm.push(r);
    Ok(())
}

fn int_op(name: &str, x: i64, y: i64) -> Result<i64> {
    let r = match name {
        "add" => x.checked_add(y),
        "sub" => x.checked_sub(y),
        "mul" => x.checked_mul(y),
        "div" => {
            if y == 0 {
                return Err(Error::runtime("division by zero"));
            }
            x.checked_div(y)
        }
        "mod" => {
            if y == 0 {
                return Err(Error::runtime("modulo by zero"));
            }
            x.checked_rem(y)
        }
        _ => unreachable!("not an arithmetic word: {}", name),
    };
    r.ok_or_else(|| Error::runtime("integer overflow"))
}

fn float_op(name: &str, x: f64, y: f64) -> f64 {
    match name {
        "add" => x + y,
        "sub" => x - y,
        "mul" => x * y,
        "div" => x / y,
        "mod" => x % y,
        _ => unreachable!("not an arithmetic word: {}", name),
    }
}

fn compare(vm: &mut Interp, name: &str) -> Result<()> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    let ord = match (&a, &b) {
        (Value::Int(x), Value::Int(y)) => x.partial_cmp(y),
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y),
        (Value::Str(x), Value::Str(y)) => x.partial_cmp(y),
        _ => {
            return Err(Error::runtime(format!(
                "type error: `{}` expects two Ints, two Floats or two Strings, found {} and {}",
                name,
                a.type_name(),
                b.type_name()
            )))
        }
    };
    let result = match ord {
        // NaN is unordered: every comparison is false.
        None => false,
        Some(o) => match name {
            "lt" => o == Ordering::Less,
            "gt" => o == Ordering::Greater,
            "le" => o != Ordering::Greater,
            "ge" => o != Ordering::Less,
            _ => unreachable!("not a comparison word: {}", name),
        },
    };
    vm.push(Value::Bool(result));
    Ok(())
}

fn equality(vm: &mut Interp, name: &str) -> Result<()> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    let same = match (&a, &b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        _ => {
            return Err(Error::runtime(format!(
                "type error: `{}` expects two values of the same basic type, found {} and {}",
                name,
                a.type_name(),
                b.type_name()
            )))
        }
    };
    vm.push(Value::Bool(if name == "eq" { same } else { !same }));
    Ok(())
}
