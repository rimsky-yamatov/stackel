//! Runs every program in `examples/` and compares what it prints with a golden file.
//!
//! For `examples/NAME.stk` (or the package directory `examples/NAME/`):
//!
//! * `NAME.expected` is the expected standard output (required);
//! * `NAME.stdin` is fed to the program's standard input (optional);
//! * `NAME.args` has one program argument per line (optional);
//! * `NAME.cmd` holds the subcommand, `run` (default), `check` or `test` (optional).
//!
//! Every example must be covered: a `.stk` file or a package without a golden file fails the
//! test, so a new example cannot be added without checking what it prints.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// `(name, path)` of every example: top-level `.stk` files and package directories.
fn examples() -> Vec<(String, String)> {
    let dir = root().join("examples");
    let mut found = Vec::new();
    for entry in fs::read_dir(&dir).expect("examples/ exists") {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let rel = format!("examples/{}", path.file_name().unwrap().to_string_lossy());
        if path.is_dir() && path.join("stackel.toml").exists() {
            found.push((name, rel));
        } else if path.extension().map_or(false, |e| e == "stk") {
            found.push((name, rel));
        }
    }
    found.sort();
    found
}

fn read_opt(name: &str, ext: &str) -> Option<String> {
    fs::read_to_string(root().join("examples").join(format!("{}.{}", name, ext))).ok()
}

fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
}

fn run_example(name: &str, path: &str) -> Result<String, String> {
    let cmd = read_opt(name, "cmd").map(|c| c.trim().to_string());
    let cmd = cmd.as_deref().unwrap_or("run");
    let mut command = Command::new(env!("CARGO_BIN_EXE_stackel"));
    command.current_dir(root()).arg(cmd).arg(path);
    if let Some(args) = read_opt(name, "args") {
        for line in args.lines().filter(|l| !l.is_empty()) {
            command.arg(line);
        }
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the stackel binary starts");
    let input = read_opt(name, "stdin").unwrap_or_default();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .expect("stdin is written");
    let out = child.wait_with_output().expect("the program finishes");
    if out.status.success() {
        Ok(normalize(&String::from_utf8_lossy(&out.stdout)))
    } else {
        Err(format!(
            "{} {} failed ({}): {}",
            cmd,
            path,
            out.status,
            normalize(&String::from_utf8_lossy(&out.stderr))
        ))
    }
}

#[test]
fn every_example_has_a_golden_file() {
    let missing: Vec<String> = examples()
        .into_iter()
        .filter(|(name, _)| read_opt(name, "expected").is_none())
        .map(|(_, path)| path)
        .collect();
    assert!(
        missing.is_empty(),
        "examples without a .expected file: {:?}",
        missing
    );
}

#[test]
fn every_example_prints_what_its_golden_file_says() {
    let mut failures = Vec::new();
    for (name, path) in examples() {
        let expected = match read_opt(&name, "expected") {
            Some(e) => normalize(&e),
            None => continue, // reported by the test above
        };
        match run_example(&name, &path) {
            Ok(actual) if actual == expected => {}
            Ok(actual) => failures.push(format!(
                "{}: output differs\n--- expected\n{}--- actual\n{}",
                path, expected, actual
            )),
            Err(e) => failures.push(e),
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn no_golden_file_is_left_without_an_example() {
    let known: Vec<String> = examples().into_iter().map(|(n, _)| n).collect();
    let dir: &Path = &root().join("examples");
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let ext = path.extension().map(|e| e.to_string_lossy().to_string());
        if matches!(ext.as_deref(), Some("expected" | "stdin" | "args" | "cmd")) {
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            assert!(known.contains(&name), "{:?} has no example", path);
        }
    }
}
