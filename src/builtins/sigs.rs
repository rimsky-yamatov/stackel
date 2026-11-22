//! Stack-effect signatures of the builtin words.
//!
//! The overloaded words use constrained type variables (see `types::NUM`):
//! `n` stands for `Int` or `Float`, `o` for `Int`, `Float` or `String`, and so on.
//! `apply` is not listed: the checker handles it specially.

use crate::types::{BASIC, NUM, ORD};

/// Constraints on the type variables of a signature: (variable, set of allowed types).
type Constraints = &'static [(&'static str, u8)];

/// (word, signature, constraints on the type variables)
pub const SIGNATURES: &[(&str, &str, Constraints)] = &[
    // stack words
    ("dup", "a -- a a", &[]),
    ("drop", "a --", &[]),
    ("swap", "a b -- b a", &[]),
    ("over", "a b -- a b a", &[]),
    ("rot", "a b c -- b c a", &[]),
    ("true", "-- Bool", &[]),
    ("false", "-- Bool", &[]),
    // arithmetic
    ("add", "n n -- n", &[("n", NUM)]),
    ("sub", "n n -- n", &[("n", NUM)]),
    ("mul", "n n -- n", &[("n", NUM)]),
    ("div", "n n -- n", &[("n", NUM)]),
    ("mod", "n n -- n", &[("n", NUM)]),
    ("neg", "n -- n", &[("n", NUM)]),
    // comparison and logic
    ("lt", "o o -- Bool", &[("o", ORD)]),
    ("gt", "o o -- Bool", &[("o", ORD)]),
    ("le", "o o -- Bool", &[("o", ORD)]),
    ("ge", "o o -- Bool", &[("o", ORD)]),
    ("eq", "e e -- Bool", &[("e", BASIC)]),
    ("ne", "e e -- Bool", &[("e", BASIC)]),
    ("and", "Bool Bool -- Bool", &[]),
    ("or", "Bool Bool -- Bool", &[]),
    ("not", "Bool -- Bool", &[]),
    // strings and I/O
    ("show", "s -- String", &[("s", BASIC)]),
    ("concat", "String String -- String", &[]),
    ("length", "String -- Int", &[]),
    ("print", "String -- ! IO", &[]),
    ("read", "-- String ! IO", &[]),
    // strings (characters are Unicode code points)
    ("substring", "String Int Int -- String", &[]),
    ("index_of", "String String -- Int", &[]),
    ("ord", "String -- Int", &[]),
    ("chr", "Int -- String", &[]),
    ("trim", "String -- String", &[]),
    ("upper", "String -- String", &[]),
    ("lower", "String -- String", &[]),
    ("parse_int", "String -- Int Bool", &[]),
    ("parse_float", "String -- Float Bool", &[]),
    // failures and captured output
    ("fail", "String -- a ! Fail", &[]),
    ("__try", "[-- a ! Fail e] -- r ! e", &[]),
    ("capture", "[-- a ! IO e] -- a String ! e", &[]),
    // the process, files and the environment
    ("eprint", "String -- ! IO", &[]),
    ("exit", "Int -- ! IO", &[]),
    ("file_read", "String -- String Bool ! Fs", &[]),
    ("file_write", "String String -- String Bool ! Fs", &[]),
    ("file_exists", "String -- Bool ! Fs", &[]),
    ("arg_count", "-- Int ! Env", &[]),
    ("arg", "Int -- String ! Env", &[]),
    ("env_var", "String -- String Bool ! Env", &[]),
];
