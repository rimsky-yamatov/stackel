//! Built-in words.

mod arith;
mod control;
mod io;
mod sigs;
mod stack;
mod sys;
mod text;

use std::collections::HashMap;

use crate::error::Result;
use crate::eval::Interp;
use crate::lexer::{lex, Tok};
use crate::types::{parse_sig, Adts, Scheme};

pub fn is_builtin(name: &str) -> bool {
    stack::WORDS.contains(&name)
        || arith::WORDS.contains(&name)
        || io::WORDS.contains(&name)
        || control::WORDS.contains(&name)
        || text::WORDS.contains(&name)
        || sys::WORDS.contains(&name)
}

/// Runs the built-in word `name`. Returns `Ok(false)` if there is no such builtin.
pub fn call(vm: &mut Interp, name: &str) -> Result<bool> {
    Ok(stack::call(vm, name)?
        || arith::call(vm, name)?
        || io::call(vm, name)?
        || control::call(vm, name)?
        || text::call(vm, name)?
        || sys::call(vm, name)?)
}

/// The stack-effect types of the builtin words (everything except `apply`).
pub fn signatures() -> HashMap<String, Scheme> {
    let mut map = HashMap::new();
    for (name, sig, constraints) in sigs::SIGNATURES {
        let toks: Vec<Tok> = lex(sig)
            .expect("a builtin signature lexes")
            .into_iter()
            .map(|t| t.tok)
            .collect();
        let scheme =
            parse_sig(&toks, constraints, &Adts::new()).expect("a builtin signature parses");
        map.insert(name.to_string(), scheme);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_has_a_signature_except_apply() {
        let sigs = signatures();
        for w in stack::WORDS
            .iter()
            .chain(arith::WORDS.iter())
            .chain(io::WORDS.iter())
            .chain(control::WORDS.iter())
            .chain(text::WORDS.iter())
            .chain(sys::WORDS.iter())
        {
            if *w == "apply" {
                continue;
            }
            assert!(sigs.contains_key(*w), "missing signature for `{}`", w);
        }
    }

    #[test]
    fn every_signature_belongs_to_a_builtin() {
        for name in signatures().keys() {
            assert!(
                is_builtin(name),
                "`{}` has a signature but is not a builtin",
                name
            );
        }
    }
}
