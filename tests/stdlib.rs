//! The standard library modules that are written in Stackel.

use stackel::run_source;

fn run(src: &str) -> Result<String, String> {
    let mut out: Vec<u8> = Vec::new();
    run_source(src, &mut out).map_err(|e| e.to_string())?;
    Ok(String::from_utf8(out).unwrap())
}

fn ok(src: &str) -> String {
    match run(src) {
        Ok(out) => out,
        Err(e) => panic!("the program failed: {}", e),
    }
}

fn err(src: &str) -> String {
    match run(src) {
        Ok(out) => panic!("expected an error, but the program printed {:?}", out),
        Err(e) => e,
    }
}

#[test]
fn all_standard_modules_check() {
    // an error inside the library itself would show up here
    ok("use std.option\nuse std.result\nuse std.list\n\nmain = 1 drop\n");
}

#[test]
fn std_option() {
    let src = "use std.option (Option, None, Some, is_some, is_none, unwrap_or, map, and_then)\n\
               \n\
               half :: Int -- (Option Int)\n\
               half = dup 2 mod 0 eq\n\
               \x20 | then 2 div Some\n\
               \x20 | else drop None\n\
               \n\
               main :: -- ! IO\n\
               main = 5 Some is_some show print\n\
               \x20      None is_none show print\n\
               \x20      None 7 unwrap_or show print\n\
               \x20      4 Some [ 1 add ] map 0 unwrap_or show print\n\
               \x20      8 half [ half ] and_then 0 unwrap_or show print\n\
               \x20      6 half [ half ] and_then 0 unwrap_or show print\n";
    assert_eq!(ok(src), "true\ntrue\n7\n5\n2\n0\n");
}

#[test]
fn std_result() {
    let src = "use std.result (Result, Ok, Err, is_ok, unwrap_or, map, map_err, and_then)\n\
               \n\
               check :: Int -- (Result String Int)\n\
               check = dup 0 lt\n\
               \x20 | then drop \"negative\" Err\n\
               \x20 | else Ok\n\
               \n\
               main :: -- ! IO\n\
               main = 3 check is_ok show print\n\
               \x20      -3 check is_ok show print\n\
               \x20      3 check [ 2 mul ] map 0 unwrap_or show print\n\
               \x20      -3 check [ 2 mul ] map 0 unwrap_or show print\n\
               \x20      -3 check [ \"error: \" swap concat ] map_err [ 1 add ] map 9 unwrap_or show print\n\
               \x20      3 check [ check ] and_then 0 unwrap_or show print\n";
    assert_eq!(ok(src), "true\nfalse\n6\n0\n9\n3\n");
}

const LIST_HEAD: &str = "use std.list (List, Nil, Cons, size, append, reverse, map, filter, fold, range, sum, head, nth)\n\
                         use std.option (unwrap_or)\n\
                         \n\
                         show_list :: (List Int) -- String\n\
                         show_list = \"\" [ show concat \" \" concat ] fold\n\
                         \n";

fn with_list(rest: &str) -> String {
    format!("{}{}", LIST_HEAD, rest)
}

#[test]
fn std_list() {
    let src = with_list(
        "main :: -- ! IO\n\
         main = 0 5 range show_list print\n\
         \x20      0 5 range [ 2 mul ] map show_list print\n\
         \x20      0 10 range [ 2 mod 0 eq ] filter show_list print\n\
         \x20      0 3 range 7 9 range append show_list print\n\
         \x20      0 5 range reverse show_list print\n\
         \x20      0 5 range size show print\n\
         \x20      1 11 range sum show print\n\
         \x20      0 5 range head 99 unwrap_or show print\n\
         \x20      Nil head 99 unwrap_or show print\n\
         \x20      10 20 30 Nil Cons Cons Cons 1 nth 99 unwrap_or show print\n\
         \x20      10 20 30 Nil Cons Cons Cons 5 nth 99 unwrap_or show print\n",
    );
    assert_eq!(
        ok(&src),
        "0 1 2 3 4 \n0 2 4 6 8 \n0 2 4 6 8 \n0 1 2 7 8 \n4 3 2 1 0 \n5\n55\n0\n99\n20\n99\n"
    );
}

#[test]
fn long_lists_need_no_native_stack() {
    // The library works with loops, so the length of a list does not matter. (The thread of a
    // test has a small stack: a function that called itself once per element would overflow it.)
    let src = with_list(
        "main :: -- ! IO\n\
         main = 0 20000 range [ 1 add ] map reverse sum show print\n\
         \x20      0 20000 range size show print\n",
    );
    assert_eq!(ok(&src), "200010000\n20000\n");
}

#[test]
fn modules_with_the_same_word_names_need_qualification() {
    let e = err("use std.list\nuse std.option\n\nmain = Nil [ 1 add ] map drop\n");
    assert!(e.contains("`map` is ambiguous"), "{}", e);

    let src = "use std.list\n\
               use std.option\n\
               \n\
               main :: -- ! IO\n\
               main = 1 Nil Cons [ 1 add ] std.list.map std.list.sum show print\n";
    assert_eq!(ok(src), "2\n");
}

#[test]
fn library_types_are_checked() {
    let e = err("use std.option (Some, unwrap_or)\n\nmain = 1 Some \"x\" unwrap_or drop\n");
    assert!(e.contains("expected Int, found String"), "{}", e);
}

#[test]
fn the_stdlib_example_runs() {
    let src = std::fs::read_to_string("examples/stdlib.stk").expect("the example exists");
    assert_eq!(ok(&src), "385\n5\n6\n");
}

fn ok_with_args(src: &str, args: &[&str]) -> String {
    let loaded = stackel::modules::load_source(src).unwrap_or_else(|e| panic!("{}", e));
    let mut out: Vec<u8> = Vec::new();
    loaded
        .run_with_args(&mut out, args.iter().map(|a| a.to_string()).collect())
        .unwrap_or_else(|e| panic!("the program failed: {}", e));
    String::from_utf8(out).unwrap()
}

#[test]
fn std_text() {
    let src =
        "use std.text (split, join, lines, starts_with, ends_with, contains, to_int, to_float)\n\
               use std.list (size)\n\
               use std.option (unwrap_or)\n\
               \n\
               main :: -- ! IO\n\
               main = \"a,b,,c\" \",\" split size show print\n\
               \x20      \"a,b,,c\" \",\" split \"|\" join print\n\
               \x20      \"one\\ntwo\\nthree\" lines size show print\n\
               \x20      \"hello\" \"he\" starts_with show print\n\
               \x20      \"hello\" \"lo\" ends_with show print\n\
               \x20      \"hello\" \"xyz\" contains show print\n\
               \x20      \"hello\" \"ell\" contains show print\n\
               \x20      \"123\" to_int 0 unwrap_or show print\n\
               \x20      \"12x\" to_int -1 unwrap_or show print\n\
               \x20      \"2.5\" to_float 0.0 unwrap_or show print\n\
               \x20      \"abc\" \"\" split size show print\n";
    assert_eq!(
        ok(src),
        "4\na|b||c\n3\ntrue\ntrue\nfalse\ntrue\n123\n-1\n2.5\n1\n"
    );
}

#[test]
fn std_sys() {
    let path = std::env::temp_dir().join(format!("stackel-sys-{}.txt", std::process::id()));
    let p = path.to_string_lossy().replace('\\', "/");
    let src = format!(
        "use std.sys (args, var, read_file, write_file)\n\
         use std.list (size)\n\
         use std.result (is_ok, unwrap_or)\n\
         use std.option (is_none)\n\
         \n\
         main :: -- ! IO Fs Env\n\
         main = args size show print\n\
         \x20      \"{p}\" \"data\" write_file is_ok show print\n\
         \x20      \"{p}\" read_file \"\" unwrap_or print\n\
         \x20      \"{p}.missing\" read_file is_ok show print\n\
         \x20      \"STACKEL_NO_SUCH_VARIABLE\" var is_none show print\n",
        p = p
    );
    let out = ok_with_args(&src, &["x", "y", "z"]);
    let _ = std::fs::remove_file(&path);
    assert_eq!(out, "3\ntrue\ndata\nfalse\ntrue\n");
}

#[test]
fn the_cat_example_prints_files() {
    let dir = std::env::temp_dir();
    let first = dir.join(format!("stackel-cat-a-{}.txt", std::process::id()));
    let second = dir.join(format!("stackel-cat-b-{}.txt", std::process::id()));
    std::fs::write(&first, "alpha").unwrap();
    std::fs::write(&second, "beta").unwrap();
    let src = std::fs::read_to_string("examples/cat.stk").expect("the example exists");
    let a = first.to_string_lossy().to_string();
    let b = second.to_string_lossy().to_string();
    let out = ok_with_args(&src, &[a.as_str(), b.as_str()]);
    let _ = std::fs::remove_file(&first);
    let _ = std::fs::remove_file(&second);
    assert_eq!(out, "alpha\nbeta\n");
}

#[test]
fn higher_order_words_pass_effects_through() {
    let src = "use std.list (range, map, each, size)\n\
               \n\
               main :: -- ! IO\n\
               main = 1 4 range [ dup show print ] map size show print\n\
               \x20      1 4 range [ show print ] each\n";
    assert_eq!(ok(src), "1\n2\n3\n3\n1\n2\n3\n");
}

#[test]
fn unannotated_words_can_use_polymorphic_words() {
    let src = "use std.list (range, map, sum)\n\
               \n\
               double_all = [ 2 mul ] map\n\
               \n\
               main :: -- ! IO\n\
               main = 1 4 range double_all sum show print\n";
    assert_eq!(ok(src), "12\n");
}

#[test]
fn a_pure_word_cannot_map_with_an_effectful_quotation() {
    let e = err("use std.list (List, range, map)\n\
                 \n\
                 noisy :: (List Int) -- (List Int)\n\
                 noisy = [ dup show print ] map\n\
                 \n\
                 main = 1 2 range noisy drop\n");
    assert!(e.contains("this performs the `IO` effect"), "{}", e);
}

#[test]
fn option_and_result_pass_effects_through() {
    let src = "use std.option (Some, map)\n\
               use std.result (Ok)\n\
               \n\
               main :: -- ! IO\n\
               main = 7 Some [ dup show print ] map drop\n\
               \x20      1 Ok [ dup show print ] std.result.map drop\n";
    assert_eq!(ok(src), "7\n1\n");
}

const SAFE_DIV: &str = "safeDiv :: Int Int -- Int ! Fail\n\
                        safeDiv = dup 0 eq\n\
                        \x20 | then drop drop \"division by zero\" fail\n\
                        \x20 | else div\n\
                        \n";

#[test]
fn fail_and_try() {
    let src = format!(
        "use std.result (Result, Ok, Err, try, unwrap_or)\n\
         \n\
         {}\
         report :: (Result String Int) -- ! IO\n\
         report = match\n\
         \x20 | Ok show print\n\
         \x20 | Err print\n\
         \n\
         main :: -- ! IO\n\
         main = [ 10 2 safeDiv ] try report\n\
         \x20      [ 10 0 safeDiv ] try report\n\
         \x20      [ 10 0 safeDiv ] try 7 unwrap_or show print\n",
        SAFE_DIV
    );
    assert_eq!(ok(&src), "5\ndivision by zero\n7\n");
}

#[test]
fn an_uncaught_failure_stops_the_program() {
    let e = err("main :: -- ! IO Fail\n\
                 main = \"first\" print \"boom\" fail drop \"second\" print\n");
    assert!(e.contains("uncaught failure: boom"), "{}", e);
}

#[test]
fn failing_words_must_declare_fail() {
    let e = err("f :: -- Int\nf = \"x\" fail\nmain = f drop\n");
    assert!(e.contains("this performs the `Fail` effect"), "{}", e);
}

#[test]
fn try_needs_a_quotation_that_can_fail() {
    let e = err("use std.result (try)\n\nmain = [ 1 ] try drop\n");
    assert!(e.contains("found [-- Int]"), "{}", e);
}

#[test]
fn try_passes_the_other_effects_through() {
    // `print` is not caught: the caller performs IO and has to declare it
    let e = err("use std.result (try)\n\n\
                 main :: --\n\
                 main = [ \"x\" print \"y\" fail ] try drop\n");
    assert!(e.contains("this performs the `IO` effect"), "{}", e);
}

#[test]
fn try_restores_the_stack() {
    // The quotation leaves 5 on the stack before it fails: `try` removes it. 1 and 2 stay.
    let src = format!(
        "use std.result (try, is_ok)\n\
         \n\
         {}\
         main :: -- ! IO\n\
         main = 1 2 [ 5 6 0 safeDiv add ] try is_ok show print\n\
         \x20      add show print\n",
        SAFE_DIV
    );
    assert_eq!(ok(&src), "false\n3\n");
}

#[test]
fn capture_returns_what_a_quotation_prints() {
    let src = "main :: -- ! IO\n\
               main = [ \"hello\" print \"world\" print 42 ] capture length show print show print\n\
               \x20      [ \"hidden\" print 1 ] capture drop drop\n\
               \x20      \"visible\" print\n";
    assert_eq!(ok(src), "12\n42\nvisible\n");
}

#[test]
fn capture_passes_the_other_effects_through() {
    let e = err(
        "main :: -- ! IO\nmain = [ \"x\" print 1 \"x\" file_read drop drop ] capture drop drop\n",
    );
    assert!(e.contains("this performs the `Fs` effect"), "{}", e);
}
