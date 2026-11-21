//! Basic string words, and console input and output.
//!
//! `print` and `read` have the effect `IO`.

use std::io::Write;

use crate::error::{Error, Result};
use crate::eval::Interp;
use crate::value::Value;

pub const WORDS: &[&str] = &["show", "concat", "length", "print", "read"];

fn io_error(e: std::io::Error) -> Error {
    Error::runtime(format!("I/O error: {}", e))
}

pub fn call(vm: &mut Interp, name: &str) -> Result<bool> {
    match name {
        // a -- String   (Int, Float, Bool or String)
        "show" => match vm.pop()? {
            Value::Quote(_) => return Err(Error::runtime("cannot show a quotation")),
            Value::Data(_) => return Err(Error::runtime("cannot show a data value")),
            other => vm.push(Value::Str(other.to_string())),
        },
        // String String -- String   (`"a" "b" concat` is "ab")
        "concat" => {
            let b = vm.pop_str()?;
            let a = vm.pop_str()?;
            vm.push(Value::Str(a + &b));
        }
        // String -- Int   (number of characters)
        "length" => {
            let s = vm.pop_str()?;
            vm.push(Value::Int(s.chars().count() as i64));
        }
        // String -- ! IO   (prints the string followed by a newline)
        "print" => {
            let s = vm.pop_str()?;
            vm.write_line(&s).map_err(io_error)?;
        }
        // -- String ! IO   (reads one line from standard input, without the newline)
        "read" => {
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).map_err(io_error)?;
            while line.ends_with('\n') || line.ends_with('\r') {
                line.pop();
            }
            vm.push(Value::Str(line));
        }
        _ => return Ok(false),
    }
    Ok(true)
}
