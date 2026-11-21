//! Modules and packages: programs that are spread over several files.

use std::path::Path;

use stackel::modules::load;
use stackel::source::MemFs;

/// Loads the program at `entry` and runs it; the error is the printed message.
fn run(fs: &MemFs, entry: &str) -> Result<String, String> {
    let loaded = load(fs, Path::new(entry)).map_err(|e| e.to_string())?;
    let mut out: Vec<u8> = Vec::new();
    loaded.run(&mut out).map_err(|e| e.to_string())?;
    Ok(String::from_utf8(out).unwrap())
}

fn ok(fs: &MemFs, entry: &str) -> String {
    match run(fs, entry) {
        Ok(out) => out,
        Err(e) => panic!("the program failed: {}", e),
    }
}

fn err(fs: &MemFs, entry: &str) -> String {
    match run(fs, entry) {
        Ok(out) => panic!("expected an error, but the program printed {:?}", out),
        Err(e) => e,
    }
}

// ---- modules of one package ----

#[test]
fn imports_selected_words() {
    let fs = MemFs::new()
        .with(
            "main.stk",
            "use util (double)\n\nmain :: -- ! IO\nmain = 4 double show print\n",
        )
        .with(
            "util.stk",
            "pub double :: Int -- Int\npub double = dup add\n\nhelper = 1\n",
        );
    assert_eq!(ok(&fs, "main.stk"), "8\n");
}

#[test]
fn wildcard_imports_and_qualified_names() {
    let fs = MemFs::new()
        .with(
            "main.stk",
            "use util\n\nmain :: -- ! IO\nmain = 3 double show print 3 util.double show print\n",
        )
        .with(
            "util.stk",
            "pub double :: Int -- Int\npub double = dup add\n",
        );
    assert_eq!(ok(&fs, "main.stk"), "6\n6\n");
}

#[test]
fn nested_module_paths_are_directories() {
    let fs = MemFs::new()
        .with(
            "main.stk",
            "use a.b (x)\n\nmain :: -- ! IO\nmain = x show print\n",
        )
        .with("a/b.stk", "pub x :: -- Int\npub x = 7\n");
    assert_eq!(ok(&fs, "main.stk"), "7\n");
}

#[test]
fn private_words_cannot_be_imported() {
    let fs = MemFs::new()
        .with("main.stk", "use util (secret)\n\nmain = 1 drop\n")
        .with("util.stk", "secret = 1\n");
    let e = err(&fs, "main.stk");
    assert!(e.contains("`secret` is private in module `util`"), "{}", e);
    assert!(e.contains("main.stk"), "{}", e);
}

#[test]
fn private_words_stay_private_when_written_qualified() {
    let fs = MemFs::new()
        .with("main.stk", "use util\n\nmain = util.secret drop\n")
        .with("util.stk", "secret = 1\n");
    let e = err(&fs, "main.stk");
    assert!(e.contains("unknown word `util.secret`"), "{}", e);
}

#[test]
fn private_types_stay_private_when_written_qualified() {
    let fs = MemFs::new()
        .with(
            "main.stk",
            "use util\n\nf :: util.Hidden -- Int\nf = drop 1\n\nmain :: -- ! IO\nmain = 5 show print\n",
        )
        .with("util.stk", "type Hidden = Hid\n");
    let e = err(&fs, "main.stk");
    assert!(e.contains("unknown type `util.Hidden`"), "{}", e);
}

#[test]
fn unknown_names_are_rejected() {
    let fs = MemFs::new()
        .with("main.stk", "use util (nothing)\n\nmain = 1 drop\n")
        .with("util.stk", "pub one :: -- Int\npub one = 1\n");
    let e = err(&fs, "main.stk");
    assert!(e.contains("module `util` has no public name `nothing`"), "{}", e);
}

#[test]
fn missing_modules_are_reported() {
    let fs = MemFs::new().with("main.stk", "use nothere\n\nmain = 1 drop\n");
    let e = err(&fs, "main.stk");
    assert!(e.contains("module `nothere` not found"), "{}", e);
}

#[test]
fn import_cycles_are_rejected() {
    let fs = MemFs::new()
        .with("main.stk", "use a\n\nmain = 1 drop\n")
        .with("a.stk", "use b\n\npub x :: -- Int\npub x = 1\n")
        .with("b.stk", "use a\n\npub y :: -- Int\npub y = 2\n");
    let e = err(&fs, "main.stk");
    assert!(e.contains("circular import: a.stk -> b.stk -> a.stk"), "{}", e);
}

#[test]
fn public_words_need_signatures() {
    let fs = MemFs::new()
        .with("main.stk", "use util\n\nmain = 1 drop\n")
        .with("util.stk", "pub f = 1\n");
    let e = err(&fs, "main.stk");
    assert!(e.contains("public word `f` needs a type signature"), "{}", e);
    assert!(e.contains("util.stk"), "{}", e);
}

#[test]
fn names_cannot_contain_dots_or_redefine_builtins() {
    let fs = MemFs::new().with("main.stk", "a.b = 1\n\nmain = 1 drop\n");
    assert!(err(&fs, "main.stk").contains("cannot contain `.`"));
    let fs = MemFs::new()
        .with("main.stk", "use util\n\nmain = 1 drop\n")
        .with("util.stk", "dup = 1\n");
    assert!(err(&fs, "main.stk").contains("cannot redefine the builtin word `dup`"));
}

#[test]
fn use_from_is_not_supported_yet() {
    let fs = MemFs::new().with(
        "main.stk",
        "use json from \"pkg:json@^1\"\n\nmain = 1 drop\n",
    );
    let e = err(&fs, "main.stk");
    assert!(e.contains("`use ... from` is not supported yet"), "{}", e);
}

// ---- name resolution ----

#[test]
fn private_names_of_different_modules_do_not_clash() {
    let fs = MemFs::new()
        .with(
            "main.stk",
            "use a (f)\nuse b (g)\n\nmain :: -- ! IO\nmain = f show print g show print\n",
        )
        .with("a.stk", "pub f :: -- Int\npub f = helper\n\nhelper = 10\n")
        .with("b.stk", "pub g :: -- Int\npub g = helper\n\nhelper = 20\n");
    assert_eq!(ok(&fs, "main.stk"), "10\n20\n");
}

fn two_modules_with_f() -> MemFs {
    MemFs::new()
        .with("a.stk", "pub f :: -- Int\npub f = 1\n")
        .with("b.stk", "pub f :: -- Int\npub f = 2\n")
}

#[test]
fn a_name_imported_from_two_modules_is_ambiguous() {
    let fs = two_modules_with_f().with("main.stk", "use a\nuse b\n\nmain = f show print\n");
    let e = err(&fs, "main.stk");
    assert!(e.contains("`f` is ambiguous"), "{}", e);
}

#[test]
fn qualified_names_resolve_ambiguity() {
    let fs = two_modules_with_f().with(
        "main.stk",
        "use a\nuse b\n\nmain :: -- ! IO\nmain = a.f show print b.f show print\n",
    );
    assert_eq!(ok(&fs, "main.stk"), "1\n2\n");
}

#[test]
fn local_definitions_shadow_wildcard_imports() {
    let fs = two_modules_with_f().with(
        "main.stk",
        "use a\n\nf :: -- Int\nf = 3\n\nmain :: -- ! IO\nmain = f show print a.f show print\n",
    );
    assert_eq!(ok(&fs, "main.stk"), "3\n1\n");
}

#[test]
fn an_explicit_import_may_not_clash_with_a_local_definition() {
    let fs = two_modules_with_f().with(
        "main.stk",
        "use a (f)\n\nf = 3\n\nmain :: -- ! IO\nmain = f show print\n",
    );
    let e = err(&fs, "main.stk");
    assert!(
        e.contains("`f` is imported from `a`, but this module also defines it"),
        "{}",
        e
    );
}

#[test]
fn an_explicit_import_wins_over_a_wildcard_import() {
    let fs = two_modules_with_f().with(
        "main.stk",
        "use a\nuse b (f)\n\nmain :: -- ! IO\nmain = f show print\n",
    );
    assert_eq!(ok(&fs, "main.stk"), "2\n");
}

// ---- data types across modules ----

const OPT: &str = "pub type Option a = None | Some a\n\
                   \n\
                   pub describe :: (Option Int) -- String\n\
                   pub describe = match\n\
                   \x20 | None \"none\"\n\
                   \x20 | Some show\n";

#[test]
fn imported_data_types_and_constructors() {
    let fs = MemFs::new().with("opt.stk", OPT).with(
        "main.stk",
        "use opt (Option, None, Some, describe)\n\
         \n\
         main :: -- ! IO\n\
         main = 5 Some describe print None describe print\n",
    );
    assert_eq!(ok(&fs, "main.stk"), "5\nnone\n");
}

#[test]
fn qualified_type_and_constructor_names() {
    let fs = MemFs::new()
        .with("opt.stk", "pub type Option a = None | Some a\n")
        .with(
            "main.stk",
            "use opt\n\
             \n\
             f :: (opt.Option Int) -- Int\n\
             f = match\n\
             \x20 | opt.None 0\n\
             \x20 | opt.Some\n\
             \n\
             main :: -- ! IO\n\
             main = 3 opt.Some f show print opt.None f show print\n",
        );
    assert_eq!(ok(&fs, "main.stk"), "3\n0\n");
}

#[test]
fn types_of_different_modules_are_different_types() {
    let fs = MemFs::new()
        .with("a.stk", "pub type T = MkA\n\npub mkA :: -- T\npub mkA = MkA\n")
        .with("b.stk", "pub type T = MkB\n\npub mkB :: -- T\npub mkB = MkB\n")
        .with(
            "main.stk",
            "use a\nuse b\n\ntakesA :: a.T --\ntakesA = drop\n\nmain = mkB takesA\n",
        );
    let e = err(&fs, "main.stk");
    assert!(e.contains("a.T") && e.contains("b.T"), "{}", e);
}

// ---- the standard library ----

#[test]
fn standard_modules_give_qualified_names() {
    let fs = MemFs::new().with(
        "main.stk",
        "use std.io\n\nmain :: -- ! IO\nmain = \"x\" std.io.print\n",
    );
    assert_eq!(ok(&fs, "main.stk"), "x\n");
}

#[test]
fn standard_imports_are_checked() {
    let fs = MemFs::new().with("main.stk", "use std.nope\n\nmain = 1 drop\n");
    assert!(err(&fs, "main.stk").contains("unknown standard library module `std.nope`"));
    let fs = MemFs::new().with("main.stk", "use std.io (nope)\n\nmain = 1 drop\n");
    assert!(err(&fs, "main.stk").contains("module `std.io` has no word `nope`"));
}

// ---- errors name the file ----

#[test]
fn type_errors_name_the_file() {
    let fs = MemFs::new()
        .with(
            "main.stk",
            "use util (bad)\n\nmain :: -- ! IO\nmain = 1 bad show print\n",
        )
        .with("util.stk", "pub bad :: Int -- Int\npub bad = \"x\" add\n");
    let e = err(&fs, "main.stk");
    assert!(e.contains("util.stk at 2:"), "{}", e);
}

#[test]
fn run_time_errors_name_the_file() {
    let fs = MemFs::new()
        .with(
            "main.stk",
            "use util (boom)\n\nmain :: -- ! IO\nmain = 5 boom show print\n",
        )
        .with("util.stk", "pub boom :: Int -- Int\npub boom = 0 div\n");
    let e = err(&fs, "main.stk");
    assert!(e.contains("util.stk"), "{}", e);
    assert!(e.contains("division by zero"), "{}", e);
}

// ---- packages ----

const MATHX: &str = "pub square :: Int -- Int\n\
                     pub square = dup mul\n\
                     \n\
                     pub cube :: Int -- Int\n\
                     pub cube = dup square mul\n";

/// A package `app` that depends on the package `mathx`, whose root module is `lib`.
/// `allow` is extra text in the declaration of the dependency.
fn app_with(lib: &str, allow: &str) -> MemFs {
    MemFs::new()
        .with(
            "app/stackel.toml",
            &format!(
                "[package]\nname = \"app\"\n\n[deps]\nmathx = {{ path = \"../mathx\"{} }}\n",
                allow
            ),
        )
        .with(
            "app/src/main.stk",
            "use mathx\n\nmain :: -- ! IO\nmain = 3 mathx.cube show print\n",
        )
        .with("mathx/stackel.toml", "[package]\nname = \"mathx\"\n")
        .with("mathx/src/lib.stk", lib)
}

#[test]
fn a_package_uses_a_dependency() {
    assert_eq!(ok(&app_with(MATHX, ""), "app"), "27\n");
}

#[test]
fn a_file_inside_a_package_belongs_to_it() {
    assert_eq!(ok(&app_with(MATHX, ""), "app/src/main.stk"), "27\n");
}

#[test]
fn a_pure_dependency_may_have_an_empty_ceiling() {
    assert_eq!(ok(&app_with(MATHX, ", allow = []"), "app"), "27\n");
}

fn shouting_lib() -> String {
    format!(
        "pub shout :: String -- ! IO\npub shout = print\n\n{}",
        MATHX
    )
}

#[test]
fn a_dependency_may_not_exceed_its_effect_ceiling() {
    let fs = app_with(&shouting_lib(), ", allow = []");
    let e = err(&fs, "app");
    assert!(e.contains("the package `mathx` may perform no effects"), "{}", e);
    assert!(e.contains("`mathx.shout` performs the `IO` effect"), "{}", e);
    assert!(e.contains("mathx/src/lib.stk"), "{}", e);
}

#[test]
fn a_dependency_may_use_the_effects_it_is_allowed() {
    let fs = app_with(&shouting_lib(), ", allow = [\"IO\"]");
    assert_eq!(ok(&fs, "app"), "27\n");
}

#[test]
fn a_dependency_without_a_ceiling_is_not_limited() {
    let fs = app_with(&shouting_lib(), "");
    assert_eq!(ok(&fs, "app"), "27\n");
}

#[test]
fn a_ceiling_also_catches_effects_that_come_from_calls() {
    // `mathx` is limited to pure words. Its word `loud` has no signature; it gets the
    // `IO` effect from the package `noisy`, which has no limit of its own.
    let fs = MemFs::new()
        .with(
            "app/stackel.toml",
            "[package]\nname = \"app\"\n\n[deps]\nmathx = { path = \"../mathx\", allow = [] }\n",
        )
        .with(
            "app/src/main.stk",
            "use mathx\n\nmain :: -- ! IO\nmain = 3 mathx.cube show print\n",
        )
        .with(
            "mathx/stackel.toml",
            "[package]\nname = \"mathx\"\n\n[deps]\nnoisy = { path = \"../noisy\" }\n",
        )
        .with(
            "mathx/src/lib.stk",
            &format!("use noisy (shout)\n\nloud = \"!\" shout\n\n{}", MATHX),
        )
        .with("noisy/stackel.toml", "[package]\nname = \"noisy\"\n")
        .with(
            "noisy/src/lib.stk",
            "pub shout :: String -- ! IO\npub shout = print\n",
        );
    let e = err(&fs, "app");
    assert!(e.contains("the package `mathx` may perform no effects"), "{}", e);
    assert!(e.contains("`mathx.loud` performs the `IO` effect"), "{}", e);
}

#[test]
fn modules_of_a_package_import_each_other() {
    let fs = MemFs::new()
        .with("app/stackel.toml", "[package]\nname = \"app\"\n\n[deps]\nmathx = { path = \"../mathx\" }\n")
        .with(
            "app/src/main.stk",
            "use mathx (quad)\n\nmain :: -- ! IO\nmain = 5 quad show print\n",
        )
        .with("mathx/stackel.toml", "[package]\nname = \"mathx\"\n")
        .with(
            "mathx/src/lib.stk",
            "use helpers (twice)\n\npub quad :: Int -- Int\npub quad = twice twice\n",
        )
        .with(
            "mathx/src/helpers.stk",
            "pub twice :: Int -- Int\npub twice = dup add\n",
        );
    assert_eq!(ok(&fs, "app"), "20\n");
}

#[test]
fn dependency_modules_are_reached_through_the_package_name() {
    let fs = MemFs::new()
        .with("app/stackel.toml", "[package]\nname = \"app\"\n\n[deps]\nmathx = { path = \"../mathx\" }\n")
        .with(
            "app/src/main.stk",
            "use mathx.extra (five)\n\nmain :: -- ! IO\nmain = five show print\n",
        )
        .with("mathx/stackel.toml", "[package]\nname = \"mathx\"\n")
        .with("mathx/src/extra.stk", "pub five :: -- Int\npub five = 5\n");
    assert_eq!(ok(&fs, "app"), "5\n");
}

#[test]
fn a_manifest_may_name_the_entry_file() {
    let fs = MemFs::new()
        .with(
            "app/stackel.toml",
            "[package]\nname = \"app\"\nentry = \"src/app.stk\"\n",
        )
        .with("app/src/app.stk", "main :: -- ! IO\nmain = \"hi\" print\n");
    assert_eq!(ok(&fs, "app"), "hi\n");
}

#[test]
fn a_directory_without_a_manifest_is_not_a_package() {
    let fs = MemFs::new().with("x/main.stk", "main = 1 drop\n");
    assert!(err(&fs, "x").contains("not a package"));
}

#[test]
fn dependency_declarations_are_checked() {
    // the dependency is named differently from its package
    let fs = app_with(MATHX, "").with("mathx/stackel.toml", "[package]\nname = \"other\"\n");
    let e = err(&fs, "app");
    assert!(e.contains("points to a package named `other`"), "{}", e);
    assert!(e.contains("app/stackel.toml"), "{}", e);

    // registry dependencies cannot be fetched yet
    let fs = MemFs::new()
        .with(
            "app/stackel.toml",
            "[package]\nname = \"app\"\n\n[deps]\nmathx = \"^1.0\"\n",
        )
        .with("app/src/main.stk", "main = 1 drop\n");
    assert!(err(&fs, "app").contains("registry dependencies are not supported yet"));

    // the effect names in `allow` must exist
    let fs = app_with(MATHX, ", allow = [\"Net\"]");
    assert!(err(&fs, "app").contains("unknown effect `Net`"));

    // the package of a dependency must exist
    let fs = MemFs::new()
        .with(
            "app/stackel.toml",
            "[package]\nname = \"app\"\n\n[deps]\nmathx = { path = \"../nowhere\" }\n",
        )
        .with("app/src/main.stk", "main = 1 drop\n");
    assert!(err(&fs, "app").contains("has no stackel.toml"));
}

#[test]
fn a_broken_manifest_is_reported_with_its_file() {
    let fs = MemFs::new()
        .with("app/stackel.toml", "[package]\nname = 5\n")
        .with("app/src/main.stk", "main = 1 drop\n");
    let e = err(&fs, "app");
    assert!(e.contains("app/stackel.toml at 2:"), "{}", e);
}

#[test]
fn the_package_example_runs() {
    let loaded = stackel::load_path(Path::new("examples/pkg-demo"))
        .unwrap_or_else(|e| panic!("cannot load the example: {}", e));
    let mut out: Vec<u8> = Vec::new();
    loaded
        .run(&mut out)
        .unwrap_or_else(|e| panic!("the example failed: {}", e));
    assert_eq!(String::from_utf8(out).unwrap(), "75\n16\n27\n");
}

#[test]
fn let_names_hide_imported_names() {
    let fs = two_modules_with_f().with(
        "main.stk",
        "use a (f)\n\nmain :: -- ! IO\nmain = 5 let f in f show print\n",
    );
    assert_eq!(ok(&fs, "main.stk"), "5\n");
}

#[test]
fn ceilings_distinguish_files_from_the_console() {
    let lib = format!(
        "pub load :: String -- String ! Fs\npub load = file_read drop\n\n{}",
        MATHX
    );
    let e = err(&app_with(&lib, ", allow = [\"IO\"]"), "app");
    assert!(e.contains("the package `mathx` may perform only IO"), "{}", e);
    assert!(e.contains("`mathx.load` performs the `Fs` effect"), "{}", e);
    assert_eq!(ok(&app_with(&lib, ", allow = [\"Fs\"]"), "app"), "27\n");
}
