//! `stackel test`, and the source line in error messages.

use std::path::Path;

use stackel::modules::{load, load_source};
use stackel::source::MemFs;

#[test]
fn tests_are_run_and_reported() {
    let src = "double = dup add\n\
               \n\
               test_add = 1 2 add 3 eq\n\
               test_wrong = 1 2 add 4 eq\n\
               test_fails = \"boom\" fail drop true\n\
               test_shape = 1 2\n\
               test_double = 4 double 8 eq\n\
               helper = 1\n";
    let loaded = load_source(src).unwrap();
    let mut out: Vec<u8> = Vec::new();
    let (passed, failed) = loaded.test(&mut out).unwrap();
    assert_eq!((passed, failed), (2, 3));
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "test_add ... ok\n\
         test_wrong ... FAILED (it gave false)\n\
         test_fails ... FAILED (failed: boom)\n\
         test_shape ... FAILED (a test must leave exactly one Bool (true to pass))\n\
         test_double ... ok\n\
         5 tests: 2 passed, 3 failed\n"
    );
}

#[test]
fn a_test_that_does_not_type_check_is_an_error() {
    let loaded = load_source("test_bad = 1 \"x\" add\n").unwrap();
    let mut out: Vec<u8> = Vec::new();
    let e = loaded.test(&mut out).unwrap_err();
    assert!(e.msg.contains("expected Int, found String"), "{}", e);
    assert!(out.is_empty());
}

#[test]
fn a_program_without_tests_passes_trivially() {
    let loaded = load_source("main = 1 drop\n").unwrap();
    let mut out: Vec<u8> = Vec::new();
    assert_eq!(loaded.test(&mut out).unwrap(), (0, 0));
    assert_eq!(String::from_utf8(out).unwrap(), "0 tests: 0 passed, 0 failed\n");
}

#[test]
fn errors_carry_the_source_line() {
    let fs = MemFs::new()
        .with(
            "main.stk",
            "use util (bad)\n\nmain :: -- ! IO\nmain = 1 bad show print\n",
        )
        .with("util.stk", "pub bad :: Int -- Int\npub bad = \"x\" add\n");
    let loaded = load(&fs, Path::new("main.stk")).unwrap();
    let mut out: Vec<u8> = Vec::new();
    let e = loaded.run(&mut out).unwrap_err();
    let snippet = e.snippet.expect("the error has a source line");
    assert!(snippet.contains("2 | pub bad = \"x\" add"), "{}", snippet);
    assert!(snippet.contains('^'), "{}", snippet);
}

#[test]
fn single_source_errors_have_a_source_line_too() {
    let e = load_source("main :: -- ! IO\nmain = frobnicate\n")
        .unwrap()
        .check()
        .unwrap_err();
    assert_eq!(
        e.snippet.as_deref(),
        Some("  |\n2 | main = frobnicate\n  |        ^")
    );
}

#[test]
fn the_tests_example_passes() {
    let loaded = stackel::load_path(Path::new("examples/tests.stk"))
        .unwrap_or_else(|e| panic!("cannot load the example: {}", e));
    let mut out: Vec<u8> = Vec::new();
    let result = loaded
        .test(&mut out)
        .unwrap_or_else(|e| panic!("the example failed: {}", e));
    assert_eq!(result, (3, 0));
}

#[test]
fn the_rpn_example_passes_its_tests() {
    let loaded = stackel::load_path(Path::new("examples/rpn.stk"))
        .unwrap_or_else(|e| panic!("cannot load the example: {}", e));
    let mut out: Vec<u8> = Vec::new();
    let (passed, failed) = loaded
        .test(&mut out)
        .unwrap_or_else(|e| panic!("the example failed: {}", e));
    let report = String::from_utf8_lossy(&out).to_string();
    assert_eq!(failed, 0, "{}", report);
    assert_eq!(passed, 6, "{}", report);
}
