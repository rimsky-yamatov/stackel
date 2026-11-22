//! String, file, environment and process words.

use stackel::error::Error;
use stackel::modules::load_source;

fn run(src: &str, args: &[&str]) -> Result<String, Error> {
    let loaded = load_source(src)?;
    let mut out: Vec<u8> = Vec::new();
    loaded.run_with_args(&mut out, args.iter().map(|a| a.to_string()).collect())?;
    Ok(String::from_utf8(out).unwrap())
}

fn ok(src: &str) -> String {
    match run(src, &[]) {
        Ok(out) => out,
        Err(e) => panic!("the program failed: {}", e),
    }
}

fn err(src: &str) -> String {
    match run(src, &[]) {
        Ok(out) => panic!("expected an error, but the program printed {:?}", out),
        Err(e) => e.to_string(),
    }
}

#[test]
fn string_words() {
    let src = "main :: -- ! IO\n\
               main = \"hello world\" 6 11 substring print\n\
               \x20      \"hello\" 1 3 substring print\n\
               \x20      \"hello\" -5 99 substring print\n\
               \x20      \"hello\" 4 2 substring length show print\n\
               \x20      \"a,b,c\" \",\" index_of show print\n\
               \x20      \"abc\" \"x\" index_of show print\n\
               \x20      \"A\" ord show print\n\
               \x20      \"\" ord show print\n\
               \x20      65 chr print\n\
               \x20      955 chr \"x\" concat length show print\n\
               \x20      \"  hi  \" trim print\n\
               \x20      \"MiXed\" upper print\n\
               \x20      \"MiXed\" lower print\n";
    assert_eq!(
        ok(src),
        "world\nel\nhello\n0\n1\n-1\n65\n-1\nA\n2\nhi\nMIXED\nmixed\n"
    );
}

#[test]
fn string_words_count_characters_not_bytes() {
    let src = "main :: -- ! IO\n\
               main = \"héllo\" length show print\n\
               \x20      \"héllo\" 1 3 substring print\n\
               \x20      \"héllo\" \"llo\" index_of show print\n\
               \x20      -1 chr length show print\n\
               \x20      1114112 chr length show print\n";
    assert_eq!(ok(src), "5\nél\n2\n0\n0\n");
}

#[test]
fn parsing_words_give_a_number_and_a_flag() {
    let src = "main :: -- ! IO\n\
               main = \"42\" parse_int show print show print\n\
               \x20      \"-7\" parse_int show print show print\n\
               \x20      \"4x\" parse_int show print show print\n\
               \x20      \"2.5\" parse_float show print show print\n\
               \x20      \"abc\" parse_float show print drop\n";
    assert_eq!(ok(src), "true\n42\ntrue\n-7\nfalse\n0\ntrue\n2.5\nfalse\n");
}

#[test]
fn file_words() {
    let path = std::env::temp_dir().join(format!("stackel-test-{}.txt", std::process::id()));
    let p = path.to_string_lossy().replace('\\', "/");
    let src = format!(
        "main :: -- ! IO Fs\n\
         main = \"{p}\" \"hello file\" file_write show print drop\n\
         \x20      \"{p}\" file_read show print print\n\
         \x20      \"{p}\" file_exists show print\n\
         \x20      \"{p}.missing\" file_exists show print\n\
         \x20      \"{p}.missing\" file_read show print drop\n",
        p = p
    );
    let out = ok(&src);
    let _ = std::fs::remove_file(&path);
    assert_eq!(out, "true\ntrue\nhello file\ntrue\nfalse\nfalse\n");
}

#[test]
fn environment_words() {
    std::env::set_var("STACKEL_TEST_VARIABLE", "xyz");
    let src = "main :: -- ! IO Env\n\
               main = arg_count show print\n\
               \x20      0 arg print\n\
               \x20      1 arg print\n\
               \x20      5 arg length show print\n\
               \x20      \"STACKEL_TEST_VARIABLE\" env_var show print print\n\
               \x20      \"STACKEL_NO_SUCH_VARIABLE\" env_var show print length show print\n";
    let out = run(src, &["first", "second"]).unwrap_or_else(|e| panic!("{}", e));
    assert_eq!(out, "2\nfirst\nsecond\n0\ntrue\nxyz\nfalse\n0\n");
}

#[test]
fn exit_stops_the_program_with_a_status() {
    let src = "main :: -- ! IO\nmain = \"before\" print 3 exit \"after\" print\n";
    let loaded = load_source(src).unwrap();
    let mut out: Vec<u8> = Vec::new();
    let e = loaded.run(&mut out).unwrap_err();
    assert_eq!(e.exit, Some(3));
    assert_eq!(String::from_utf8(out).unwrap(), "before\n");
}

#[test]
fn files_and_the_environment_are_effects() {
    let e = err("main :: -- ! IO\nmain = \"x\" file_read drop drop\n");
    assert!(e.contains("`Fs`"), "{}", e);
    let e = err("main :: -- ! IO\nmain = arg_count drop\n");
    assert!(e.contains("`Env`"), "{}", e);
    ok("main :: -- ! IO Fs Env\nmain = \"/no/such/file\" file_read drop drop arg_count drop\n");
}

#[test]
fn the_new_words_are_listed_in_their_modules() {
    ok(
        "use std.string (substring, parse_int)\nuse std.io (eprint, exit)\n\
        use std.fs (file_read)\nuse std.env (arg)\n\nmain = 1 drop\n",
    );
}
