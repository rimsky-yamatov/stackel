//! The standard library modules.
//!
//! The standard words are builtins: they are always in scope. A module such as
//! `std.io` only names a group of them, so `use std.io (print)` states what a
//! program needs, and `std.io.print` is a qualified name for `print`.
//!
//! Other modules (`std.option`, `std.result`, `std.list`, `std.text`, `std.sys`) are written in
//! Stackel; their source is embedded in the program (`stdlib/*.stk`) and loaded like any other
//! module.

/// (module, the words it provides)
pub const MODULES: &[(&str, &[&str])] = &[
    (
        "std.stack",
        &["dup", "drop", "swap", "over", "rot", "apply", "true", "false"],
    ),
    ("std.arith", &["add", "sub", "mul", "div", "mod", "neg"]),
    ("std.compare", &["lt", "gt", "le", "ge", "eq", "ne"]),
    ("std.logic", &["and", "or", "not"]),
    (
        "std.string",
        &[
            "concat",
            "length",
            "show",
            "substring",
            "index_of",
            "ord",
            "chr",
            "trim",
            "upper",
            "lower",
            "parse_int",
            "parse_float",
        ],
    ),
    ("std.io", &["print", "read", "eprint", "exit"]),
    ("std.control", &["fail", "capture"]),
    ("std.fs", &["file_read", "file_write", "file_exists"]),
    ("std.env", &["arg_count", "arg", "env_var"]),
];

/// The words of the standard module `name`, if there is such a module.
pub fn module(name: &str) -> Option<&'static [&'static str]> {
    MODULES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, words)| *words)
}

/// The source of a standard module that is written in Stackel.
pub fn source(name: &str) -> Option<&'static str> {
    match name {
        "std.option" => Some(include_str!("../stdlib/option.stk")),
        "std.result" => Some(include_str!("../stdlib/result.stk")),
        "std.list" => Some(include_str!("../stdlib/list.stk")),
        "std.text" => Some(include_str!("../stdlib/text.stk")),
        "std.sys" => Some(include_str!("../stdlib/sys.stk")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;

    #[test]
    fn every_listed_word_is_a_builtin() {
        for (name, words) in MODULES {
            for w in words.iter() {
                assert!(
                    builtins::is_builtin(w),
                    "`{}` of `{}` is not a builtin",
                    w,
                    name
                );
            }
        }
    }

    #[test]
    fn modules_are_found_by_name() {
        assert!(module("std.io").unwrap().contains(&"print"));
        assert!(module("std.nope").is_none());
    }

    #[test]
    fn a_module_is_either_a_name_group_or_embedded() {
        for (name, _) in MODULES {
            assert!(source(name).is_none(), "{} is both a name group and embedded", name);
        }
    }

    #[test]
    fn embedded_modules_carry_their_own_name() {
        for name in ["std.option", "std.result", "std.list", "std.text", "std.sys"] {
            let text = source(name).expect("the module is embedded");
            assert!(text.contains(&format!("module {}", name)), "{}", name);
        }
    }
}
