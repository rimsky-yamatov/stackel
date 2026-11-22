//! The interactive session.

use stackel::repl::{run, Session};

fn session(inputs: &[&str]) -> String {
    let mut s = Session::new();
    let mut out: Vec<u8> = Vec::new();
    for input in inputs {
        s.enter(input, &mut out);
    }
    String::from_utf8(out).unwrap()
}

#[test]
fn words_run_on_the_stack_of_the_session() {
    assert_eq!(
        session(&["1 2", "add", "dup mul"]),
        "stack: 1 2\nstack: 3\nstack: 9\n"
    );
}

#[test]
fn definitions_are_kept() {
    assert_eq!(
        session(&["double = dup add", "5 double"]),
        "ok\nstack: 10\n"
    );
}

#[test]
fn a_definition_replaces_the_earlier_one() {
    assert_eq!(
        session(&["f = 1", "f", "f = 2", "f"]),
        "ok\nstack: 1\nok\nstack: 1 2\n"
    );
}

#[test]
fn a_type_error_leaves_the_stack_alone() {
    let out = session(&["1 2", "\"x\" add", "add"]);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "{}", out);
    assert_eq!(lines[0], "stack: 1 2");
    assert!(lines[1].starts_with("error: "), "{}", out);
    assert_eq!(lines[2], "stack: 3");
}

#[test]
fn a_run_time_error_leaves_the_stack_alone() {
    let out = session(&["1 0", "div", "drop drop"]);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "{}", out);
    assert_eq!(lines[0], "stack: 1 0");
    assert!(lines[1].contains("division by zero"), "{}", out);
    assert_eq!(lines[2], "stack: empty");
}

#[test]
fn a_definition_that_does_not_check_is_not_kept() {
    let out = session(&["f = 1 \"x\" add", "f"]);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2, "{}", out);
    assert!(lines[0].starts_with("error: "), "{}", out);
    assert!(lines[1].contains("unknown word `f`"), "{}", out);
}

#[test]
fn data_values_are_shown_with_their_constructors() {
    assert_eq!(
        session(&["type Option a = None | Some a", "5 Some", "Some", ":clear"]),
        "ok\nstack: Some 5\nstack: Some (Some 5)\nstack: empty\n"
    );
}

#[test]
fn the_type_command_shows_types() {
    assert_eq!(
        session(&[
            ":type 1",
            ":type dup add",
            "double = dup add",
            ":type double"
        ]),
        "1 :: -- Int\ndup add :: a -- a\nok\ndouble :: a -- a\n"
    );
}

#[test]
fn imports_and_the_standard_library_work() {
    assert_eq!(
        session(&["use std.list (range, sum)", "1 5 range sum"]),
        "ok\nstack: 10\n"
    );
}

#[test]
fn exit_ends_the_session() {
    let mut s = Session::new();
    let mut out: Vec<u8> = Vec::new();
    s.enter("1 exit", &mut out);
    assert_eq!(String::from_utf8(out).unwrap(), "exit 1\n");
    assert!(s.is_done());
}

#[test]
fn commands() {
    let out = session(&[
        ":stack", "1 2", ":stack", ":clear", ":items", "f = 1", ":items", ":reset", ":items",
        ":bogus",
    ]);
    assert_eq!(
        out,
        "stack: empty\nstack: 1 2\nstack: 1 2\nstack: empty\n(no definitions)\nok\nf = 1\nok\n\
         (no definitions)\nunknown command `:bogus` (try :help)\n"
    );
}

#[test]
fn files_can_be_loaded() {
    let path = std::env::temp_dir().join(format!("stackel-repl-{}.stk", std::process::id()));
    std::fs::write(&path, "g = 5 dup mul\n").unwrap();
    let load = format!(":load {}", path.to_string_lossy());
    let out = session(&[load.as_str(), "g"]);
    let _ = std::fs::remove_file(&path);
    assert_eq!(out, "ok\nstack: 25\n");
}

#[test]
fn the_loop_reads_lines_continuations_and_blocks() {
    let text = "f = 1\n  2 add\n\nf\n:{\n3 4\nadd\n:}\n:quit\n5\n";
    let mut input = text.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    run(&mut input, &mut out);
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("ok\n"), "{}", out);
    assert!(out.contains("stack: 3\n"), "{}", out);
    assert!(out.contains("stack: 3 7\n"), "{}", out);
    assert!(!out.contains("stack: 3 7 5"), "{}", out);
}
