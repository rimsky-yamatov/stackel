//! Words that reach outside the program: the process, files and the environment.
//!
//! Effects: `Fs` for files, `Env` for the command line and the environment variables, and
//! `IO` for the standard error output and for `exit`.
//!
//! An operation that can fail leaves a text and a flag: `path file_read` gives the contents
//! and `true`, or an error message and `false`.

use crate::error::{Error, Result};
use crate::eval::Interp;
use crate::value::Value;

pub const WORDS: &[&str] = &[
    "eprint",
    "exit",
    "file_read",
    "file_write",
    "file_exists",
    "arg_count",
    "arg",
    "env_var",
];

pub fn call(vm: &mut Interp, name: &str) -> Result<bool> {
    match name {
        // String -- ! IO   (prints the string and a newline on the standard error output)
        "eprint" => {
            let s = vm.pop_str()?;
            eprintln!("{}", s);
        }
        // Int -- ! IO   (stops the program with this exit status)
        "exit" => {
            let code = vm.pop_int()?;
            return Err(Error::exit(code as i32));
        }
        // String -- String Bool ! Fs
        "file_read" => {
            let path = vm.pop_str()?;
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    vm.push(Value::Str(text));
                    vm.push(Value::Bool(true));
                }
                Err(e) => {
                    vm.push(Value::Str(format!("cannot read {}: {}", path, e)));
                    vm.push(Value::Bool(false));
                }
            }
        }
        // String String -- String Bool ! Fs   (`path text file_write`: "" and true, or a message and false)
        "file_write" => {
            let text = vm.pop_str()?;
            let path = vm.pop_str()?;
            match std::fs::write(&path, text) {
                Ok(()) => {
                    vm.push(Value::Str(String::new()));
                    vm.push(Value::Bool(true));
                }
                Err(e) => {
                    vm.push(Value::Str(format!("cannot write {}: {}", path, e)));
                    vm.push(Value::Bool(false));
                }
            }
        }
        // String -- Bool ! Fs
        "file_exists" => {
            let path = vm.pop_str()?;
            vm.push(Value::Bool(std::path::Path::new(&path).exists()));
        }
        // -- Int ! Env   (the number of command-line arguments of the program)
        "arg_count" => {
            let n = vm.args.len() as i64;
            vm.push(Value::Int(n));
        }
        // Int -- String ! Env   (argument number `n`, counting from 0; "" if there is none)
        "arg" => {
            let i = vm.pop_int()?;
            let s = if i >= 0 {
                vm.args.get(i as usize).cloned().unwrap_or_default()
            } else {
                String::new()
            };
            vm.push(Value::Str(s));
        }
        // String -- String Bool ! Env   (the value of the environment variable, and whether it is set)
        "env_var" => {
            let var = vm.pop_str()?;
            match std::env::var(&var) {
                Ok(v) => {
                    vm.push(Value::Str(v));
                    vm.push(Value::Bool(true));
                }
                Err(_) => {
                    vm.push(Value::Str(String::new()));
                    vm.push(Value::Bool(false));
                }
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
