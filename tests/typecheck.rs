//! Tests for the static checker: programs are checked, not run.

fn ok(src: &str) {
    if let Err(e) = stackel::check_source(src) {
        panic!("expected the program to type-check, but got: {}", e);
    }
}

fn err(src: &str) -> String {
    match stackel::check_source(src) {
        Ok(()) => panic!(
            "expected a type error, but the program was accepted:\n{}",
            src
        ),
        Err(e) => e.to_string(),
    }
}

#[test]
fn polymorphic_words() {
    ok("dup2 :: a -- a a\n\
        dup2 = dup\n\
        \n\
        swap2 :: a b -- b a\n\
        swap2 = swap\n\
        \n\
        main :: -- ! IO\n\
        main = 1 dup2 add show print \"x\" \"y\" swap2 concat print\n");
}

#[test]
fn unannotated_words_are_generalized() {
    ok("dup3 = dup dup\n\
        main = 1 dup3 add add show print \"a\" dup3 concat concat print\n");
}

#[test]
fn numeric_words_work_on_ints_and_floats() {
    ok("main = 1 2 add show print 1.5 2.5 add show print");
}

#[test]
fn ints_and_floats_do_not_mix() {
    let e = err("main = 1 2.5 add show print");
    assert!(e.contains("type error in `add`"), "{}", e);
    assert!(e.contains("expected Int, found Float"), "{}", e);
}

#[test]
fn strings_are_not_numbers() {
    assert!(err("main = 1 \"a\" add show print").contains("expected Int, found String"));
    assert!(err("main = \"a\" 1 add show print").contains("expected a number"));
}

#[test]
fn errors_report_the_position_of_the_word() {
    let e = err("main = 1 \"a\" add show print");
    assert!(e.starts_with("error at 1:14:"), "{}", e);
}

#[test]
fn rigid_type_variables_cannot_be_specialized() {
    let e = err("id :: a -- a\nid = 1 add\nmain = 1 id show print");
    assert!(e.contains("expected a number"), "{}", e);
    ok("idf :: a -- a\nidf = dup drop\nmain = 1 idf show print");
}

#[test]
fn wrong_number_of_results() {
    let e = err("f :: Int -- Int\nf = drop\nmain = 1 f show print");
    assert!(e.contains("`f` leaves 0 value(s)"), "{}", e);
}

#[test]
fn consuming_more_than_declared() {
    let e = err("g :: -- Int\ng = 1 add\nmain = g show print");
    assert!(e.contains("takes 1 more value(s)"), "{}", e);
}

#[test]
fn wrong_result_type() {
    let e = err("h :: Int -- String\nh = 1 add\nmain = 1 h print");
    assert!(e.contains("the result of `h`"), "{}", e);
    assert!(e.contains("expected String, found Int"), "{}", e);
}

#[test]
fn guard_condition_must_be_bool() {
    let e = err("main = 1 | then | else");
    assert!(e.contains("type error in guard"), "{}", e);
    assert!(e.contains("expected Bool, found Int"), "{}", e);
}

#[test]
fn guard_branches_must_have_the_same_type() {
    let e = err("f :: Int -- Int\n\
         f = dup 0 lt\n\
         \x20 | then drop \"x\"\n\
         \x20 | else 1 add\n\
         \n\
         main :: -- ! IO\n\
         main = 1 f show print\n");
    assert!(e.contains("branches do not match"), "{}", e);
}

#[test]
fn guard_branches_must_leave_the_same_number_of_values() {
    let e = err("f :: Int -- Int\n\
         f = dup 0 lt\n\
         \x20 | then 1\n\
         \x20 | else\n\
         \n\
         main = 1 f show print\n");
    assert!(e.contains("the `then` branch has the type"), "{}", e);
}

#[test]
fn an_empty_branch_is_fine_when_the_effects_agree() {
    ok("count :: Int -- Int\n\
        count = dup 0 le\n\
        \x20 | then\n\
        \x20 | else 1 sub count\n\
        \n\
        main = 5 count show print\n");
}

#[test]
fn recursive_words_need_a_signature() {
    let e = err("loop = loop\nmain = loop");
    assert!(
        e.contains("recursive word `loop` needs a type signature"),
        "{}",
        e
    );
}

#[test]
fn main_must_have_the_empty_type() {
    let e = err("main = 1");
    assert!(e.contains("has the type `-- Int`"), "{}", e);
    let e = err("main = drop");
    assert!(e.contains("has the type `a --`"), "{}", e);
    let e = err("main :: -- ! IO\nmain = 1");
    assert!(e.contains("`main` leaves 1 value(s)"), "{}", e);
}

#[test]
fn quotations_can_be_applied() {
    ok("main = 3 [ 1 add ] apply show print");
    ok("twice :: Int [Int -- Int] -- Int\n\
        twice = dup rot swap apply swap apply\n\
        main = 3 [ 1 add ] twice show print\n");
}

#[test]
fn applied_quotation_must_fit_the_stack() {
    let e = err("main = 1 [ \"a\" concat ] apply show print");
    assert!(e.contains("type error in `apply`"), "{}", e);
    assert!(e.contains("expected String, found Int"), "{}", e);
}

#[test]
fn apply_needs_a_quotation() {
    let e = err("main = 1 apply");
    assert!(e.contains("expected a quotation, found Int"), "{}", e);
}

#[test]
fn apply_needs_a_known_quotation_type() {
    let e = err("f = apply\nmain = 1 f");
    assert!(e.contains("give this word a type signature"), "{}", e);
}

#[test]
fn quotation_types_must_match_exactly_for_now() {
    // Stack-polymorphic quotations need explicit row variables (a later milestone).
    let e =
        err("apply2 :: Int [Int -- Int] -- Int\napply2 = apply\nmain = 1 [ ] apply2 show print");
    assert!(e.contains("type error in `apply2`"), "{}", e);
}

#[test]
fn signature_errors() {
    assert!(err("f :: Foo -- Int\nf = 1\nmain = f drop").contains("unknown type `Foo`"));
    assert!(
        err("f :: Int -- Int\nf :: Int -- Int\nf = 1\nmain = f drop")
            .contains("duplicate type signature")
    );
    assert!(err("f :: Int Int\nf = 1\nmain = f drop").contains("needs `--`"));
}

#[test]
fn declared_effects_are_accepted_for_now() {
    ok("f :: String -- ! IO\n\
        f = print\n\
        main :: -- ! IO\n\
        main = \"hi\" f\n");
}

#[test]
fn a_word_must_declare_the_effects_it_performs() {
    let e = err("f :: String --\nf = print\nmain = \"x\" f");
    assert!(e.contains("this performs the `IO` effect"), "{}", e);
    assert!(e.contains("word `f` does not declare it"), "{}", e);
    assert!(e.starts_with("error at 2:5:"), "{}", e);
}

#[test]
fn effects_propagate_through_unannotated_words() {
    let e = err("greet = \"hi\" print\npure :: --\npure = greet\nmain = pure");
    assert!(e.contains("type error in `greet`"), "{}", e);
    assert!(e.contains("word `pure` does not declare it"), "{}", e);
    ok("greet = \"hi\" print\nmain :: -- ! IO\nmain = greet");
}

#[test]
fn main_must_declare_io_when_it_prints() {
    let e = err("main :: --\nmain = \"x\" print");
    assert!(e.contains("word `main` does not declare it"), "{}", e);
}

#[test]
fn declared_effects_may_be_broader_than_needed() {
    ok("f :: Int -- Int ! IO\nf = 1 add\nmain :: -- ! IO\nmain = 1 f show print");
}

#[test]
fn guards_perform_the_effects_of_both_branches() {
    let e = err("f :: Bool --\nf = | then \"x\" print | else\nmain = true f");
    assert!(e.contains("this performs the `IO` effect"), "{}", e);
}

#[test]
fn applying_a_quotation_performs_its_effects() {
    let e = err("f :: --\nf = [ \"x\" print ] apply\nmain = f");
    assert!(e.contains("type error in `apply`"), "{}", e);
    assert!(e.contains("this performs the `IO` effect"), "{}", e);
}

#[test]
fn building_a_quotation_performs_nothing() {
    ok("g :: -- [String -- ! IO]\ng = [ print ]\nmain = g drop");
}

#[test]
fn quotation_effects_must_match_the_expected_type() {
    let e = err("run :: String [String --] --\n\
         run = apply\n\
         main :: -- ! IO\n\
         main = \"x\" [ print ] run\n");
    assert!(e.contains("found [String -- ! IO]"), "{}", e);
    ok("run :: String [String -- ! IO] -- ! IO\n\
        run = apply\n\
        main :: -- ! IO\n\
        main = \"x\" [ print ] run\n");
}

#[test]
fn unknown_effects_are_rejected() {
    assert!(err("f :: -- ! Net\nf = 1 drop\nmain = f").contains("unknown effect `Net`"));
}

const OPTION: &str = "type Option a = None | Some a\n";

fn with_option(rest: &str) -> String {
    format!("{}{}", OPTION, rest)
}

#[test]
fn data_types_and_match_type_check() {
    ok(&with_option(
        "describe :: (Option Int) -- String\n\
         describe = match\n\
         \x20 | None \"none\"\n\
         \x20 | Some show\n\
         \n\
         main = 5 Some describe print None describe print\n",
    ));
}

#[test]
fn constructors_are_polymorphic() {
    ok(&with_option(
        "main = None None drop drop 1 Some drop \"x\" Some drop",
    ));
}

#[test]
fn constructor_arguments_are_checked() {
    let e = err(&with_option(
        "f :: (Option Int) -- Int\n\
         f = match\n\
         \x20 | None 0\n\
         \x20 | Some\n\
         \n\
         main = \"x\" Some f drop\n",
    ));
    assert!(e.contains("expected Int, found String"), "{}", e);
}

#[test]
fn match_must_be_exhaustive() {
    let e = err(&with_option(
        "f :: (Option Int) -- Int\nf = match | Some\nmain = 1 Some f drop",
    ));
    assert!(e.contains("not exhaustive: missing None"), "{}", e);
}

#[test]
fn match_arms_must_be_unique() {
    let e = err(&with_option(
        "f :: (Option Int) -- Int\nf = match | Some | Some | None 0\nmain = 1 Some f drop",
    ));
    assert!(
        e.contains("duplicate arm for the constructor `Some`"),
        "{}",
        e
    );
}

#[test]
fn match_arms_must_belong_to_one_type() {
    let e = err("type A = X | Y\ntype B = Z\nf = match | X | Z\nmain = X f");
    assert!(
        e.contains("constructor `Z` belongs to type `B`, not `A`"),
        "{}",
        e
    );
}

#[test]
fn match_arms_must_have_the_same_type() {
    let e = err(&with_option(
        "f :: (Option Int) -- Int\n\
         f = match\n\
         \x20 | None \"x\"\n\
         \x20 | Some\n\
         \n\
         main = 1 Some f drop\n",
    ));
    assert!(e.contains("the `match` arms do not match"), "{}", e);
    let e = err(&with_option(
        "f :: (Option Int) --\nf = match | None | Some\nmain = 1 Some f",
    ));
    assert!(e.contains("the arm for `None` has the type"), "{}", e);
    assert!(e.contains("but the arm for `Some` has the type"), "{}", e);
}

#[test]
fn match_needs_a_value_of_the_matched_type() {
    let e = err(&with_option(
        "f :: Int -- Int\nf = match | None 0 | Some\nmain = 1 f drop",
    ));
    assert!(e.contains("expected (Option a), found Int"), "{}", e);
}

#[test]
fn unknown_constructors_in_match_are_rejected() {
    let e = err("f = match | Foo 1\nmain = 1 drop");
    assert!(e.contains("unknown constructor `Foo`"), "{}", e);
}

#[test]
fn match_arms_can_perform_effects() {
    let e = err(&with_option(
        "f :: (Option String) --\nf = match | None | Some print\nmain = None f",
    ));
    assert!(e.contains("this performs the `IO` effect"), "{}", e);
}

#[test]
fn data_values_cannot_be_shown_or_compared() {
    let e = err("type Color = Red\nmain = Red show print");
    assert!(
        e.contains("expected Int, Float, String or Bool, found Color"),
        "{}",
        e
    );
    let e = err("type Color = Red\nmain = Red Red eq drop");
    assert!(e.contains("found Color"), "{}", e);
}

#[test]
fn data_declaration_errors() {
    assert!(err("type T = A Foo\nmain = A drop").contains("unknown type `Foo`"));
    assert!(err("type T = A b\nmain = 1 drop").contains("unknown type variable `b`"));
    assert!(err("type Int = A\nmain = 1 drop").contains("builtin type"));
    assert!(
        err("type T = A\ntype T = B\nmain = 1 drop").contains("type `T` is defined more than once")
    );
    assert!(err("type T a a = A\nmain = 1 drop").contains("duplicate type parameter `a`"));
    assert!(
        err("type Option a = None | Some a\ntype T = A Option\nmain = 1 drop")
            .contains("takes 1 parameter(s)")
    );
    assert!(err("type T = A\ntype U = A\nmain = 1 drop")
        .contains("constructor `A` is defined more than once"));
    assert!(err("type T = A\nA = 1\nmain = 1 drop").contains("same name as a constructor"));
}

#[test]
fn recursive_data_types_type_check() {
    ok("type List a = Nil | Cons a (List a)\n\
        \n\
        size :: (List a) -- Int\n\
        size = match\n\
        \x20 | Nil 0\n\
        \x20 | Cons swap drop size 1 add\n\
        \n\
        main = 1 2 Nil Cons Cons size drop\n");
}

#[test]
fn let_types_its_names_from_the_stack() {
    ok("f :: Int Int -- Int\nf = let a b in b a sub\nmain = 1 2 f drop");
}

#[test]
fn let_names_have_one_type_and_are_checked() {
    let e = err("f :: Int -- Int\nf = let a in a \"x\" add\nmain = 1 f drop");
    assert!(e.contains("expected Int, found String"), "{}", e);
}

#[test]
fn let_names_are_not_visible_in_other_words() {
    let e = err("g = a\nf = 1 let a in g\nmain = f drop");
    assert!(e.contains("unknown word `a`"), "{}", e);
}

#[test]
fn let_names_hide_words() {
    ok("main = \"x\" let print in print drop");
}

#[test]
fn closures_see_the_names_of_their_let() {
    ok("adder :: Int -- [Int -- Int]\nadder = let n in [ n add ]\nmain = 7 3 adder apply drop");
}

const EACH: &str = "type List a = Nil | Cons a (List a)\n\
                    \n\
                    each :: (List a) [a -- ! e] -- ! e\n\
                    each = let xs f in xs match\n\
                    \x20 | Nil\n\
                    \x20 | Cons let x tl in x f apply tl f each\n\
                    \n";

#[test]
fn effect_variables_pass_the_effects_of_a_quotation_through() {
    ok("twice :: [-- ! e] -- ! e\n\
        twice = dup apply apply\n\
        \n\
        main :: -- ! IO\n\
        main = [ \"hi\" print ] twice\n");
    // a pure quotation gives a pure call
    ok("twice :: [-- ! e] -- ! e\n\
        twice = dup apply apply\n\
        \n\
        pure :: --\n\
        pure = [ 1 drop ] twice\n\
        \n\
        main = pure\n");
}

#[test]
fn the_caller_must_declare_the_effects_of_the_quotation() {
    let e = err("twice :: [-- ! e] -- ! e\n\
                 twice = dup apply apply\n\
                 \n\
                 main :: -- ! Fs\n\
                 main = [ \"hi\" print ] twice\n");
    assert!(e.contains("this performs the `IO` effect"), "{}", e);
}

#[test]
fn a_word_must_declare_the_effect_variable_it_uses() {
    let e = err("bad :: [-- ! e] --\nbad = apply\nmain = [ ] bad\n");
    assert!(e.contains("this performs the `e` effect"), "{}", e);
}

#[test]
fn an_effect_variable_is_not_a_concrete_effect() {
    // inside the word, `e` is not IO: a quotation of type [-- ! e] cannot be used as [-- ! IO]
    let e = err("run :: [-- ! IO] -- ! IO\n\
                 run = apply\n\
                 bad :: [-- ! e] -- ! e\n\
                 bad = run\n\
                 main = [ ] bad\n");
    assert!(e.contains("expected [-- ! IO], found [-- ! e]"), "{}", e);
}

#[test]
fn polymorphic_words_work_on_data() {
    ok(&format!(
        "{}show_all :: (List Int) -- ! IO\n\
         show_all = [ show print ] each\n\
         \n\
         main :: -- ! IO\n\
         main = 1 2 Nil Cons Cons show_all\n",
        EACH
    ));
    // with a pure quotation, the word is pure
    ok(&format!(
        "{}count :: (List Int) -- Int\n\
         count = 0 swap [ drop ] each\n\
         \n\
         main = 1 Nil Cons count drop\n",
        EACH
    ));
}
