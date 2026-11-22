//! End-to-end tests: run Stackel source and compare the output.

fn run(src: &str) -> Result<String, String> {
    let mut out: Vec<u8> = Vec::new();
    stackel::run_source(src, &mut out).map_err(|e| e.to_string())?;
    Ok(String::from_utf8(out).unwrap())
}

fn ok(src: &str) -> String {
    run(src).unwrap_or_else(|e| panic!("program failed: {}", e))
}

fn err(src: &str) -> String {
    match run(src) {
        Ok(out) => panic!("expected an error, but the program printed {:?}", out),
        Err(e) => e,
    }
}

#[test]
fn hello_world() {
    assert_eq!(
        ok("main :: -- ! IO\nmain = \"Hello, Stackel!\" print\n"),
        "Hello, Stackel!\n"
    );
}

#[test]
fn module_and_use_are_accepted() {
    let src = "module hello\nuse std.io (print)\nmain = \"hi\" print\n";
    assert_eq!(ok(src), "hi\n");
}

#[test]
fn integer_arithmetic_uses_deeper_value_as_first_operand() {
    let src = "main = 10 3 sub show print 1 2 add 3 mul show print 7 2 mod show print 20 4 div show print";
    assert_eq!(ok(src), "7\n9\n1\n5\n");
}

#[test]
fn float_arithmetic() {
    assert_eq!(ok("main = 1.5 2.5 add show print"), "4.0\n");
}

#[test]
fn numeric_words_are_polymorphic_over_int_and_float() {
    let src = "double = dup add\n\nmain = 3 double show print 1.5 double show print\n";
    assert_eq!(ok(src), "6\n3.0\n");
}

#[test]
fn comparison_and_logic() {
    let src = "main = 1 2 lt show print \"b\" \"a\" gt show print true false or show print 3 3 ne show print";
    assert_eq!(ok(src), "true\ntrue\ntrue\nfalse\n");
}

#[test]
fn stack_words() {
    assert_eq!(ok("main = 1 2 swap show print show print"), "1\n2\n");
    assert_eq!(
        ok("main = 1 2 over show print show print show print"),
        "1\n2\n1\n"
    );
    assert_eq!(
        ok("main = 1 2 3 rot show print show print show print"),
        "1\n3\n2\n"
    );
    assert_eq!(ok("main = 5 dup add show print"), "10\n");
}

#[test]
fn strings() {
    let src = "main = \"foo\" \"bar\" concat dup print length show print";
    assert_eq!(ok(src), "foobar\n6\n");
}

#[test]
fn factorial() {
    let src = "fact :: Int -- Int\nfact = dup 1 le\n  | then drop 1\n  | else dup 1 sub fact mul\n\nmain = 10 fact show print\n";
    assert_eq!(ok(src), "3628800\n");
}

#[test]
fn nested_guards() {
    let src = "sign = dup 0 lt\n  | then drop \"negative\"\n  | else 0 eq\n    | then \"zero\"\n    | else \"positive\"\n\nmain = -5 sign print 0 sign print 7 sign print\n";
    assert_eq!(ok(src), "negative\nzero\npositive\n");
}

#[test]
fn tail_calls_run_in_constant_stack_space() {
    let src = "count :: Int -- Int\ncount = dup 0 le\n  | then\n  | else 1 sub count\n\nmain = 1000000 count show print\n";
    assert_eq!(ok(src), "0\n");
}

#[test]
fn mutual_tail_recursion() {
    let src = "even :: Int -- Bool\neven = dup 0 eq\n  | then drop true\n  | else 1 sub odd\n\nodd :: Int -- Bool\nodd = dup 0 eq\n  | then drop false\n  | else 1 sub even\n\nmain = 10 even show print 7 even show print\n";
    assert_eq!(ok(src), "true\nfalse\n");
}

#[test]
fn quotations_and_apply() {
    assert_eq!(ok("main = 3 [ 1 add ] apply show print"), "4\n");
    let src = "inc = [ 1 add ] apply\n\nmain = 1 inc inc show print\n";
    assert_eq!(ok(src), "3\n");
}

#[test]
fn higher_order_word() {
    let src = "twice :: Int [Int -- Int] -- Int\ntwice = dup rot swap apply swap apply\n\nmain = 3 [ 1 add ] twice show print\n";
    assert_eq!(ok(src), "5\n");
}

#[test]
fn comments_are_ignored() {
    let src = "# a comment\nmain = 1 # trailing\n  show print\n";
    assert_eq!(ok(src), "1\n");
}

#[test]
fn runtime_errors() {
    assert!(err("main = 1 0 div show print").contains("division by zero"));
    assert!(err("main = 9223372036854775807 1 add show print").contains("integer overflow"));
}

#[test]
fn runtime_errors_report_positions() {
    assert_eq!(
        err("main = 1 0 div show print"),
        "error at 1:12: division by zero (in `div`)"
    );
}

#[test]
fn nothing_runs_when_the_program_does_not_type_check() {
    let src = "main = \"started\" print 1 \"a\" add show print";
    let mut out: Vec<u8> = Vec::new();
    assert!(stackel::run_source(src, &mut out).is_err());
    assert!(out.is_empty());
}

#[test]
fn load_errors() {
    assert_eq!(
        err("main :: -- ! IO\nmain = frobnicate"),
        "error at 2:8: unknown word `frobnicate`"
    );
    assert!(err("f = 1").contains("no `main`"));
    assert!(err("dup = 1\nmain = 1").contains("cannot redefine"));
    assert!(err("f = 1\nf = 2\nmain = f drop").contains("more than once"));
    assert!(err("g :: Int -- Int\nmain = 1 drop").contains("no definition"));
}

#[test]
fn parse_errors() {
    assert!(err("main = [ 1").contains("unclosed"));
    assert!(err("main = dup | then drop").contains("else"));
    assert!(err("main = \"abc").contains("unterminated"));
}

#[test]
fn optional_values() {
    let src = "type Option a = None | Some a\n\
               \n\
               describe :: (Option Int) -- String\n\
               describe = match\n\
               \x20 | None \"none\"\n\
               \x20 | Some show\n\
               \n\
               main = 5 Some describe print None describe print\n";
    assert_eq!(ok(src), "5\nnone\n");
}

#[test]
fn recursive_data_types() {
    let src = "type List a = Nil | Cons a (List a)\n\
               \n\
               sum :: (List Int) -- Int\n\
               sum = match\n\
               \x20 | Nil 0\n\
               \x20 | Cons sum add\n\
               \n\
               size :: (List a) -- Int\n\
               size = match\n\
               \x20 | Nil 0\n\
               \x20 | Cons swap drop size 1 add\n\
               \n\
               main = 1 2 3 Nil Cons Cons Cons dup sum show print size show print\n";
    assert_eq!(ok(src), "6\n3\n");
}

#[test]
fn constructors_with_several_fields() {
    let src = "type Pair a b = Pair a b\n\
               \n\
               swapPair :: (Pair a b) -- (Pair b a)\n\
               swapPair = match\n\
               \x20 | Pair swap Pair\n\
               \n\
               second :: (Pair a b) -- b\n\
               second = match\n\
               \x20 | Pair swap drop\n\
               \n\
               main = 1 \"x\" Pair swapPair second show print\n";
    assert_eq!(ok(src), "1\n");
}

#[test]
fn enumerations() {
    let src = "type Color = Red | Green | Blue\n\
               \n\
               name :: Color -- String\n\
               name = match\n\
               \x20 | Red \"red\"\n\
               \x20 | Green \"green\"\n\
               \x20 | Blue \"blue\"\n\
               \n\
               main = Red name print Blue name print\n";
    assert_eq!(ok(src), "red\nblue\n");
}

#[test]
fn a_guard_can_end_a_match_arm() {
    let src = "type Option a = None | Some a\n\
               \n\
               absOrZero :: (Option Int) -- Int\n\
               absOrZero = match\n\
               \x20 | None 0\n\
               \x20 | Some dup 0 lt\n\
               \x20   | then neg\n\
               \x20   | else\n\
               \n\
               main = -5 Some absOrZero show print 7 Some absOrZero show print None absOrZero show print\n";
    assert_eq!(ok(src), "5\n7\n0\n");
}

#[test]
fn match_arms_may_perform_effects() {
    let src = "type Option a = None | Some a\n\
               \n\
               greet :: (Option String) -- ! IO\n\
               greet = match\n\
               \x20 | None \"nobody\" print\n\
               \x20 | Some print\n\
               \n\
               main :: -- ! IO\n\
               main = \"bob\" Some greet None greet\n";
    assert_eq!(ok(src), "bob\nnobody\n");
}

#[test]
fn effects_are_checked_before_running() {
    let src = "pure :: String --\npure = print\nmain :: -- ! IO\nmain = \"x\" pure\n";
    let mut out: Vec<u8> = Vec::new();
    assert!(stackel::run_source(src, &mut out).is_err());
    assert!(out.is_empty());
}

#[test]
fn let_binds_values_from_the_stack() {
    // the last name is the top of the stack
    let src = "main = 5 3 let a b in a b sub show print b a sub show print\n";
    assert_eq!(ok(src), "2\n-2\n");
}

#[test]
fn let_names_hide_words_with_the_same_name() {
    let src = "main = 5 let dup in dup dup add show print\n";
    assert_eq!(ok(src), "10\n");
}

#[test]
fn nested_lets_shadow_outer_names() {
    let src = "main = 1 2 let a b in 10 let a in a b add show print\n";
    assert_eq!(ok(src), "12\n");
}

#[test]
fn quotations_capture_local_names() {
    let src = "adder :: Int -- [Int -- Int]\n\
               adder = let n in [ n add ]\n\
               \n\
               main = 7 3 adder apply show print\n";
    assert_eq!(ok(src), "10\n");
}

#[test]
fn let_in_match_arms_and_before_guards() {
    let src = "type List a = Nil | Cons a (List a)\n\
               \n\
               sum :: (List Int) -- Int\n\
               sum = match\n\
               \x20 | Nil 0\n\
               \x20 | Cons let x tl in tl sum x add\n\
               \n\
               abs :: Int -- Int\n\
               abs = let n in n 0 lt\n\
               \x20 | then n neg\n\
               \x20 | else n\n\
               \n\
               main = 1 2 3 Nil Cons Cons Cons sum show print -4 abs show print 4 abs show print\n";
    assert_eq!(ok(src), "6\n4\n4\n");
}

#[test]
fn tail_calls_inside_let_run_in_constant_stack() {
    let src = "sumTo :: Int Int -- Int\n\
               sumTo = let n acc in n 0 le\n\
               \x20 | then acc\n\
               \x20 | else n 1 sub acc n add sumTo\n\
               \n\
               main = 100000 0 sumTo show print\n";
    assert_eq!(ok(src), "5000050000\n");
}
