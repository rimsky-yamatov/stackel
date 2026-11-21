//! Stackel: a pure, stack-oriented functional language with effect types.

pub mod ast;
pub mod builtins;
pub mod check;
pub mod error;
pub mod eval;
pub mod infer;
pub mod lexer;
pub mod manifest;
pub mod modules;
pub mod parser;
pub mod repl;
pub mod source;
pub mod stdlib;
pub mod types;
pub mod value;

use std::io::Write;
use std::path::Path;

impl modules::Loaded {
    /// Checks names, types and effects without running the program.
    pub fn check(&self) -> error::Result<()> {
        let mut sink = std::io::sink();
        eval::Interp::new(&mut sink)
            .load(&self.program)
            .map_err(|e| self.localize(e))?;
        check::check_program(&self.program).map_err(|e| self.localize(e))
    }

    /// Checks the program and runs it, writing program output to `out`.
    ///
    /// Nothing runs if the program is rejected by the checker. A program that calls `exit`
    /// ends with an error whose `exit` field holds the status.
    pub fn run(&self, out: &mut dyn Write) -> error::Result<()> {
        self.run_with_args(out, Vec::new())
    }

    /// Like `run`, with command-line arguments for the program (`arg_count`, `arg`).
    pub fn run_with_args(&self, out: &mut dyn Write, args: Vec<String>) -> error::Result<()> {
        let mut interp = eval::Interp::new(out);
        interp.set_args(args);
        interp.load(&self.program).map_err(|e| self.localize(e))?;
        check::check_program(&self.program).map_err(|e| self.localize(e))?;
        interp.run_main().map_err(|e| self.localize(e))
    }

    fn localize(&self, e: error::Error) -> error::Error {
        modules::localize(&self.files, &self.sources, e)
    }

    /// Checks the program, then runs its tests: the words of the entry module whose names
    /// start with `test_`. A test leaves exactly one Bool on the stack, and passes if it is
    /// `true`. A report is written to `out`; the result is the number of tests that passed and
    /// the number that failed.
    pub fn test(&self, out: &mut dyn Write) -> error::Result<(usize, usize)> {
        self.check()?;
        let names: Vec<String> = self
            .program
            .defs
            .iter()
            .filter(|d| d.name.starts_with("test_") && !d.name.contains('.'))
            .map(|d| d.name.clone())
            .collect();
        let mut passed = 0;
        let mut failed = 0;
        for name in &names {
            let (outcome, stack) = {
                let mut interp = eval::Interp::new(&mut *out);
                let outcome = match interp.load(&self.program) {
                    Ok(()) => interp.run_word(name),
                    Err(e) => Err(e),
                };
                (outcome, std::mem::take(&mut interp.stack))
            };
            let problem: Option<String> = match outcome {
                Ok(()) => match stack.as_slice() {
                    [value::Value::Bool(true)] => None,
                    [value::Value::Bool(false)] => Some("it gave false".to_string()),
                    _ => Some("a test must leave exactly one Bool (true to pass)".to_string()),
                },
                Err(e) => Some(if e.failure {
                    format!("failed: {}", e.msg)
                } else {
                    e.msg
                }),
            };
            match problem {
                None => {
                    passed += 1;
                    let _ = writeln!(out, "{} ... ok", name);
                }
                Some(why) => {
                    failed += 1;
                    let _ = writeln!(out, "{} ... FAILED ({})", name, why);
                }
            }
        }
        let _ = writeln!(
            out,
            "{} tests: {} passed, {} failed",
            names.len(),
            passed,
            failed
        );
        Ok((passed, failed))
    }
}

/// Parses and checks `src` without running it. Only the standard library can be imported.
pub fn check_source(src: &str) -> error::Result<()> {
    modules::load_source(src)?.check()
}

/// Parses, checks and runs `src`, writing program output to `out`.
/// Only the standard library can be imported.
///
/// Nothing runs if the program is rejected by the parser or the checker.
pub fn run_source(src: &str, out: &mut dyn Write) -> error::Result<()> {
    modules::load_source(src)?.run(out)
}

/// Loads the program at `path`: a `.stk` file, or a package directory (one with a
/// `stackel.toml`), together with the modules and packages it uses.
pub fn load_path(path: &Path) -> error::Result<modules::Loaded> {
    modules::load(&source::RealFs, path)
}
