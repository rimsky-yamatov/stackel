//! Words that stop a computation or run a quotation in a controlled way.
//!
//! `fail` stops with a message (effect `Fail`). `__try` is the engine of `try` in `std.result`,
//! which gives it its type. `capture` runs a quotation and returns what it printed.

use std::rc::Rc;

use crate::error::{Error, Result};
use crate::eval::Interp;
use crate::value::{Data, Value};

pub const WORDS: &[&str] = &["fail", "__try", "capture"];

/// `Ok value` or `Err value` of the type `Result` of `std.result`.
fn result_value(ctor: &str, value: Value) -> Value {
    Value::Data(Rc::new(Data {
        ty: "std.result.Result".to_string(),
        ctor: format!("std.result.{}", ctor),
        fields: vec![value],
    }))
}

pub fn call(vm: &mut Interp, name: &str) -> Result<bool> {
    match name {
        // String -- a ! Fail   (stops the computation with a message; `try` catches it)
        "fail" => {
            let msg = vm.pop_str()?;
            return Err(Error::failure(msg));
        }
        // [-- a ! Fail e] -- r ! e
        // Runs the quotation. If it fails, the stack is what it was before, and the result is
        // `Err` of the message; otherwise the result is `Ok` of the value it left.
        "__try" => {
            let q = vm.pop_quote()?;
            let depth = vm.stack.len();
            match vm.exec(Rc::clone(&q.body), q.env.clone()) {
                Ok(()) => {
                    let value = vm.pop()?;
                    vm.push(result_value("Ok", value));
                }
                Err(e) if e.failure => {
                    vm.stack.truncate(depth);
                    vm.push(result_value("Err", Value::Str(e.msg)));
                }
                Err(e) => return Err(e),
            }
        }
        // [-- a ! IO e] -- a String ! e
        // Runs the quotation without printing what it prints: the output is returned instead.
        "capture" => {
            let q = vm.pop_quote()?;
            vm.captures.push(Vec::new());
            let result = vm.exec(Rc::clone(&q.body), q.env.clone());
            let buffer = vm.captures.pop().unwrap_or_default();
            result?;
            vm.push(Value::Str(String::from_utf8_lossy(&buffer).into_owned()));
        }
        _ => return Ok(false),
    }
    Ok(true)
}
