//! Stack-shuffling words and boolean literals.
//!
//! `apply` is listed here so that its name is reserved, but the evaluator
//! implements it directly (it needs tail-call handling).

use crate::error::Result;
use crate::eval::Interp;
use crate::value::Value;

pub const WORDS: &[&str] = &[
    "dup", "drop", "swap", "over", "rot", "apply", "true", "false",
];

pub fn call(vm: &mut Interp, name: &str) -> Result<bool> {
    match name {
        // a -- a a
        "dup" => {
            let a = vm.pop()?;
            vm.push(a.clone());
            vm.push(a);
        }
        // a --
        "drop" => {
            vm.pop()?;
        }
        // a b -- b a
        "swap" => {
            let b = vm.pop()?;
            let a = vm.pop()?;
            vm.push(b);
            vm.push(a);
        }
        // a b -- a b a
        "over" => {
            let b = vm.pop()?;
            let a = vm.pop()?;
            vm.push(a.clone());
            vm.push(b);
            vm.push(a);
        }
        // a b c -- b c a
        "rot" => {
            let c = vm.pop()?;
            let b = vm.pop()?;
            let a = vm.pop()?;
            vm.push(b);
            vm.push(c);
            vm.push(a);
        }
        "true" => vm.push(Value::Bool(true)),
        "false" => vm.push(Value::Bool(false)),
        _ => return Ok(false),
    }
    Ok(true)
}
