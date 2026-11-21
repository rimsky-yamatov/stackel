//! Types used by the static checker, and the parser for type signatures.
//!
//! A word has a *stack effect* `ins -- outs`: it takes `ins` from the top of
//! the stack (the last one is the top) and leaves `outs`; the rest of the
//! stack is untouched. Lowercase names in a signature are type variables.

use std::collections::HashMap;

use crate::lexer::Tok;

/// The data types of a program: name -> number of type parameters.
pub type Adts = HashMap<String, usize>;

// Bit sets of the basic types a type variable may stand for.
pub const INT: u8 = 1;
pub const FLOAT: u8 = 2;
pub const STR: u8 = 4;
pub const BOOL: u8 = 8;
pub const QUOTE: u8 = 16;
/// A user-defined data type.
pub const CON: u8 = 32;
/// Any type.
pub const ANY: u8 = 63;
/// `Int` or `Float`.
pub const NUM: u8 = INT | FLOAT;
/// Types that can be ordered: `Int`, `Float` or `String`.
pub const ORD: u8 = INT | FLOAT | STR;
/// All basic (non-quotation) types.
pub const BASIC: u8 = INT | FLOAT | STR | BOOL;

#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Int,
    Float,
    Str,
    Bool,
    /// A quotation `[ ins -- outs ! effects ]` (`effects` is sorted).
    Quote(Vec<Ty>, Vec<Ty>, Vec<String>),
    /// A data type applied to its arguments, e.g. `(Option Int)`.
    Con(String, Vec<Ty>),
    /// A unification variable (an index into the inference state).
    Var(usize),
    /// A generalized variable of a `Scheme`.
    Param(usize),
    /// A rigid variable: a type variable of a declared signature while its body is checked.
    Rigid(usize),
}

/// A (possibly polymorphic) stack effect.
#[derive(Debug, Clone, PartialEq)]
pub struct Scheme {
    /// Names of the parameters (type variables), used in messages.
    pub names: Vec<String>,
    /// For each parameter, the set of types it may stand for.
    pub masks: Vec<u8>,
    pub ins: Vec<Ty>,
    pub outs: Vec<Ty>,
    /// Effects the word may perform (`! IO`), sorted. A lowercase name is an effect variable:
    /// it stands for the effects of a quotation that is passed to the word (`! e`).
    pub effects: Vec<String>,
}

/// The effects a signature may mention: `IO` (the console and exiting), `Fs` (files),
/// `Env` (the command line and environment variables) and `Fail` (a computation that can stop
/// with a message: `fail`, `try`).
pub const KNOWN_EFFECTS: &[&str] = &["IO", "Fs", "Env", "Fail"];

/// Whether an effect name is a variable (`e` in a signature, `?e3` while checking) and not a
/// concrete effect such as `IO`.
pub fn is_effect_var(name: &str) -> bool {
    name.chars()
        .next()
        .map_or(false, |c| c.is_lowercase() || c == '?')
}

fn normalize_effects(mut names: Vec<String>) -> Result<Vec<String>, String> {
    for n in &names {
        if is_effect_var(n) {
            if !n.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Err(format!("`{}` is not a valid effect variable", n));
            }
        } else if !KNOWN_EFFECTS.contains(&n.as_str()) {
            return Err(format!(
                "unknown effect `{}` (known effects: {})",
                n,
                KNOWN_EFFECTS.join(", ")
            ));
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

/// Replaces `Param(i)` by `vars[i]`.
pub fn subst_params(t: &Ty, vars: &[Ty]) -> Ty {
    match t {
        Ty::Param(i) => vars[*i].clone(),
        Ty::Quote(ins, outs, effects) => Ty::Quote(
            ins.iter().map(|x| subst_params(x, vars)).collect(),
            outs.iter().map(|x| subst_params(x, vars)).collect(),
            effects.clone(),
        ),
        Ty::Con(name, args) => Ty::Con(
            name.clone(),
            args.iter().map(|x| subst_params(x, vars)).collect(),
        ),
        other => other.clone(),
    }
}

/// Adds the names of the effect variables in the quotation types inside `t` to `out`.
pub fn effect_vars_in(t: &Ty, out: &mut Vec<String>) {
    match t {
        Ty::Quote(ins, outs, effects) => {
            for x in ins.iter().chain(outs.iter()) {
                effect_vars_in(x, out);
            }
            for e in effects {
                if is_effect_var(e) && !out.contains(e) {
                    out.push(e.clone());
                }
            }
        }
        Ty::Con(_, args) => {
            for x in args {
                effect_vars_in(x, out);
            }
        }
        _ => {}
    }
}

/// Renames the effect variables in the quotation types inside `t`.
pub fn rename_effect_vars(t: &Ty, map: &HashMap<String, String>) -> Ty {
    match t {
        Ty::Quote(ins, outs, effects) => Ty::Quote(
            ins.iter().map(|x| rename_effect_vars(x, map)).collect(),
            outs.iter().map(|x| rename_effect_vars(x, map)).collect(),
            effects
                .iter()
                .map(|e| map.get(e).cloned().unwrap_or_else(|| e.clone()))
                .collect(),
        ),
        Ty::Con(name, args) => Ty::Con(
            name.clone(),
            args.iter().map(|x| rename_effect_vars(x, map)).collect(),
        ),
        other => other.clone(),
    }
}

/// Describes the set of types a constrained variable may stand for.
pub fn describe_mask(mask: u8) -> String {
    match mask {
        INT => "Int".to_string(),
        FLOAT => "Float".to_string(),
        STR => "String".to_string(),
        BOOL => "Bool".to_string(),
        QUOTE => "a quotation".to_string(),
        CON => "a data type".to_string(),
        NUM => "a number (Int or Float)".to_string(),
        ORD => "Int, Float or String".to_string(),
        BASIC => "Int, Float, String or Bool".to_string(),
        ANY => "any type".to_string(),
        _ => "a different type".to_string(),
    }
}

/// `a`, `b`, ..., `z`, `a1`, `b1`, ...
pub fn var_name(i: usize) -> String {
    let c = (b'a' + (i % 26) as u8) as char;
    if i < 26 {
        c.to_string()
    } else {
        format!("{}{}", c, i / 26)
    }
}

const ROW_ERROR: &str =
    "explicit row variables are only supported as the same `..r` on both sides of `--`";

struct SigParser<'a> {
    toks: &'a [Tok],
    pos: usize,
    names: Vec<String>,
    masks: Vec<u8>,
    constraints: &'a [(&'a str, u8)],
    adts: &'a Adts,
}

impl<'a> SigParser<'a> {
    fn peek(&self) -> Option<&'a Tok> {
        self.toks.get(self.pos)
    }

    fn param(&mut self, name: &str) -> Ty {
        if let Some(i) = self.names.iter().position(|n| n == name) {
            return Ty::Param(i);
        }
        let mask = self
            .constraints
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, m)| *m)
            .unwrap_or(ANY);
        self.names.push(name.to_string());
        self.masks.push(mask);
        Ty::Param(self.names.len() - 1)
    }

    fn named(&mut self, w: &str) -> Result<Ty, String> {
        match w {
            "Int" => Ok(Ty::Int),
            "Float" => Ok(Ty::Float),
            "String" => Ok(Ty::Str),
            "Bool" => Ok(Ty::Bool),
            _ => {
                if let Some(arity) = self.adts.get(w) {
                    return if *arity == 0 {
                        Ok(Ty::Con(w.to_string(), Vec::new()))
                    } else {
                        Err(format!(
                            "type `{}` takes {} parameter(s); write it in parentheses, e.g. ({} ...)",
                            w, arity, w
                        ))
                    };
                }
                if w.contains('.') {
                    return Err(format!("unknown type `{}`", w));
                }
                let first = w.chars().next().unwrap_or('_');
                if first.is_lowercase() {
                    Ok(self.param(w))
                } else {
                    Err(format!("unknown type `{}`", w))
                }
            }
        }
    }

    /// One type: a name, a quotation type, or a parenthesized type application.
    fn one_type(&mut self) -> Result<Ty, String> {
        match self.peek() {
            Some(Tok::Word(w)) => {
                self.pos += 1;
                self.named(w)
            }
            Some(Tok::LBracket) => {
                self.pos += 1;
                self.quote()
            }
            Some(Tok::LParen) => {
                self.pos += 1;
                self.applied()
            }
            Some(other) => Err(format!("unexpected {:?} in a type", other)),
            None => Err("a type is missing".to_string()),
        }
    }

    /// The rest of `(Name arg ...)`, after the `(`.
    fn applied(&mut self) -> Result<Ty, String> {
        let name = match self.peek() {
            Some(Tok::Word(w)) => {
                self.pos += 1;
                w.clone()
            }
            _ => return Err("expected a type name after `(`".to_string()),
        };
        let arity = match self.adts.get(&name) {
            Some(n) => *n,
            None => return Err(format!("unknown type `{}`", name)),
        };
        let mut args = Vec::with_capacity(arity);
        for _ in 0..arity {
            if matches!(self.peek(), Some(Tok::RParen) | None) {
                return Err(format!("type `{}` takes {} parameter(s)", name, arity));
            }
            args.push(self.one_type()?);
        }
        match self.peek() {
            Some(Tok::RParen) => {
                self.pos += 1;
                Ok(Ty::Con(name, args))
            }
            _ => Err(format!("type `{}` takes {} parameter(s)", name, arity)),
        }
    }

    /// Parses a run of types, with an optional leading row variable (`..r`).
    fn types(&mut self) -> Result<(Option<String>, Vec<Ty>), String> {
        let mut row = None;
        if let Some(Tok::Word(w)) = self.peek() {
            if w.starts_with("..") {
                row = Some(w.clone());
                self.pos += 1;
            }
        }
        let mut tys = Vec::new();
        loop {
            match self.peek() {
                None | Some(Tok::DashDash) | Some(Tok::RBracket) => break,
                Some(Tok::Word(w)) if w == "!" => break,
                Some(Tok::Word(_)) | Some(Tok::LBracket) | Some(Tok::LParen) => {
                    let ty = self.one_type()?;
                    tys.push(ty);
                }
                Some(other) => return Err(format!("unexpected {:?} in a type signature", other)),
            }
        }
        Ok((row, tys))
    }

    /// Reads effect names after a `!`. Inside a quotation the list ends at the `]`.
    fn effect_list(&mut self, in_quote: bool) -> Result<Vec<String>, String> {
        let mut names = Vec::new();
        loop {
            match self.peek() {
                None => break,
                Some(Tok::RBracket) if in_quote => break,
                Some(Tok::Comma) => self.pos += 1,
                Some(Tok::Word(w)) => {
                    self.pos += 1;
                    let name: String = w.chars().filter(|c| *c != '{' && *c != '}').collect();
                    if !name.is_empty() {
                        names.push(name);
                    }
                }
                Some(other) => return Err(format!("unexpected {:?} after `!`", other)),
            }
        }
        normalize_effects(names)
    }

    /// Parses the rest of a quotation type, after the `[`.
    fn quote(&mut self) -> Result<Ty, String> {
        let (r1, ins) = self.types()?;
        match self.peek() {
            Some(Tok::DashDash) => self.pos += 1,
            _ => return Err("a quotation type needs `--`".to_string()),
        }
        let (r2, outs) = self.types()?;
        if r1 != r2 {
            return Err(ROW_ERROR.to_string());
        }
        let mut effects = Vec::new();
        if let Some(Tok::Word(w)) = self.peek() {
            if w == "!" {
                self.pos += 1;
                effects = self.effect_list(true)?;
            }
        }
        match self.peek() {
            Some(Tok::RBracket) => {
                self.pos += 1;
                Ok(Ty::Quote(ins, outs, effects))
            }
            _ => Err("unclosed `[` in a type signature".to_string()),
        }
    }
}

/// Parses the tokens of a type signature (the part after `::`).
///
/// `constraints` restricts named type variables (used for the builtin words,
/// e.g. `("n", NUM)`); user signatures pass an empty slice. `adts` lists the
/// data types that may be mentioned.
pub fn parse_sig(
    tokens: &[Tok],
    constraints: &[(&str, u8)],
    adts: &Adts,
) -> Result<Scheme, String> {
    let mut p = SigParser {
        toks: tokens,
        pos: 0,
        names: Vec::new(),
        masks: Vec::new(),
        constraints,
        adts,
    };
    let (r1, ins) = p.types()?;
    match p.peek() {
        Some(Tok::DashDash) => p.pos += 1,
        _ => return Err("a type signature needs `--` between inputs and outputs".to_string()),
    }
    let (r2, outs) = p.types()?;
    if r1 != r2 {
        return Err(ROW_ERROR.to_string());
    }
    let mut effects = Vec::new();
    match p.peek() {
        None => {}
        Some(Tok::Word(w)) if w == "!" => {
            p.pos += 1;
            effects = p.effect_list(false)?;
        }
        Some(Tok::DashDash) => {
            return Err("a type signature can contain only one `--`".to_string());
        }
        Some(_) => return Err("unmatched `]` in a type signature".to_string()),
    }
    Ok(Scheme {
        names: p.names,
        masks: p.masks,
        ins,
        outs,
        effects,
    })
}

/// Parses the field types of a constructor of a type with parameters `params`.
/// Only the type's own parameters may appear as type variables.
pub fn parse_fields(tokens: &[Tok], params: &[String], adts: &Adts) -> Result<Vec<Ty>, String> {
    let mut p = SigParser {
        toks: tokens,
        pos: 0,
        names: params.to_vec(),
        masks: vec![ANY; params.len()],
        constraints: &[],
        adts,
    };
    let (row, tys) = p.types()?;
    if row.is_some() {
        return Err("row variables are not allowed in a constructor".to_string());
    }
    if p.pos != tokens.len() {
        return Err(format!("unexpected {:?} in a constructor", tokens[p.pos]));
    }
    if p.names.len() != params.len() {
        return Err(format!("unknown type variable `{}`", p.names[params.len()]));
    }
    Ok(tys)
}

/// Counts the top-level types in a run of tokens; a parenthesized or bracketed group counts as one.
pub fn count_types(tokens: &[Tok]) -> Result<usize, String> {
    let mut depth = 0usize;
    let mut count = 0usize;
    for t in tokens {
        match t {
            Tok::LParen | Tok::LBracket => {
                if depth == 0 {
                    count += 1;
                }
                depth += 1;
            }
            Tok::RParen | Tok::RBracket => {
                if depth == 0 {
                    return Err("unbalanced bracket".to_string());
                }
                depth -= 1;
            }
            Tok::Word(_) => {
                if depth == 0 {
                    count += 1;
                }
            }
            other => {
                if depth == 0 {
                    return Err(format!("unexpected {:?}", other));
                }
            }
        }
    }
    if depth != 0 {
        return Err("unbalanced bracket".to_string());
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn toks(s: &str) -> Vec<Tok> {
        lex(s).unwrap().into_iter().map(|t| t.tok).collect()
    }

    fn sig(s: &str) -> Result<Scheme, String> {
        parse_sig(&toks(s), &[], &Adts::new())
    }

    fn adts() -> Adts {
        let mut m = Adts::new();
        m.insert("Option".to_string(), 1);
        m.insert("Color".to_string(), 0);
        m.insert("Pair".to_string(), 2);
        m
    }

    #[test]
    fn concrete_types() {
        let s = sig("Int Int -- Int").unwrap();
        assert_eq!(s.ins, vec![Ty::Int, Ty::Int]);
        assert_eq!(s.outs, vec![Ty::Int]);
        assert!(s.names.is_empty());
    }

    #[test]
    fn type_variables() {
        let s = sig("a b -- b a").unwrap();
        assert_eq!(s.names, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(s.ins, vec![Ty::Param(0), Ty::Param(1)]);
        assert_eq!(s.outs, vec![Ty::Param(1), Ty::Param(0)]);
    }

    #[test]
    fn empty_sides() {
        let s = sig("--").unwrap();
        assert!(s.ins.is_empty() && s.outs.is_empty());
        let s = sig("a --").unwrap();
        assert_eq!(s.ins.len(), 1);
        assert!(s.outs.is_empty());
    }

    #[test]
    fn quotation_types() {
        let s = sig("[Int -- Int] Int -- Int").unwrap();
        assert_eq!(s.ins[0], Ty::Quote(vec![Ty::Int], vec![Ty::Int], vec![]));
        assert_eq!(s.ins[1], Ty::Int);
    }

    #[test]
    fn quotation_types_can_carry_effects() {
        let s = sig("[String -- ! IO] --").unwrap();
        assert_eq!(
            s.ins[0],
            Ty::Quote(vec![Ty::Str], vec![], vec!["IO".to_string()])
        );
    }

    #[test]
    fn effects_are_recorded() {
        assert_eq!(sig("-- ! IO").unwrap().effects, vec!["IO".to_string()]);
        assert_eq!(
            sig("String -- ! {IO}").unwrap().effects,
            vec!["IO".to_string()]
        );
        assert_eq!(sig("-- ! IO IO").unwrap().effects, vec!["IO".to_string()]);
        assert!(sig("-- ! Net").unwrap_err().contains("unknown effect `Net`"));
    }

    #[test]
    fn matching_row_variables_are_accepted() {
        let s = sig("..r a -- ..r a a").unwrap();
        assert_eq!(s.ins, vec![Ty::Param(0)]);
        assert_eq!(s.outs, vec![Ty::Param(0), Ty::Param(0)]);
    }

    #[test]
    fn constraints_apply_to_named_variables() {
        let s = parse_sig(&toks("n n -- n"), &[("n", NUM)], &Adts::new()).unwrap();
        assert_eq!(s.masks, vec![NUM]);
    }

    #[test]
    fn data_types_in_signatures() {
        let s = parse_sig(
            &toks("(Option Int) Color -- (Pair a (Option a))"),
            &[],
            &adts(),
        )
        .unwrap();
        assert_eq!(s.names, vec!["a".to_string()]);
        assert_eq!(
            s.ins,
            vec![
                Ty::Con("Option".to_string(), vec![Ty::Int]),
                Ty::Con("Color".to_string(), vec![]),
            ]
        );
        assert_eq!(
            s.outs,
            vec![Ty::Con(
                "Pair".to_string(),
                vec![
                    Ty::Param(0),
                    Ty::Con("Option".to_string(), vec![Ty::Param(0)])
                ]
            )]
        );
    }

    #[test]
    fn data_type_errors() {
        let a = adts();
        let err = |s: &str| parse_sig(&toks(s), &[], &a).unwrap_err();
        assert!(err("Option -- Int").contains("takes 1 parameter(s)"));
        assert!(err("(Option) -- Int").contains("takes 1 parameter(s)"));
        assert!(err("(Option Int Int) -- Int").contains("takes 1 parameter(s)"));
        assert!(err("(Nope Int) -- Int").contains("unknown type `Nope`"));
        assert!(err("(Option Int -- Int").contains("takes 1 parameter(s)"));
    }

    #[test]
    fn constructor_fields() {
        let mut a = adts();
        a.insert("List".to_string(), 1);
        let params = vec!["a".to_string()];
        let tys = parse_fields(&toks("a (List a)"), &params, &a).unwrap();
        assert_eq!(
            tys,
            vec![
                Ty::Param(0),
                Ty::Con("List".to_string(), vec![Ty::Param(0)])
            ]
        );
        assert!(parse_fields(&toks("b"), &params, &a)
            .unwrap_err()
            .contains("unknown type variable `b`"));
        assert!(parse_fields(&toks("Int -- Int"), &params, &a).is_err());
        assert!(parse_fields(&toks(""), &params, &a).unwrap().is_empty());
    }

    #[test]
    fn errors() {
        assert!(sig("..a -- ..b").is_err());
        assert!(sig("Foo -- Int").is_err());
        assert!(sig("Int Int").is_err());
        assert!(sig("Int -- Int -- Int").is_err());
        assert!(sig("[Int -- Int").is_err());
        assert!(sig("Int -- Int ]").is_err());
    }

    #[test]
    fn counting_types() {
        assert_eq!(count_types(&toks("")), Ok(0));
        assert_eq!(count_types(&toks("a (List a)")), Ok(2));
        assert_eq!(count_types(&toks("[Int -- Int] Int")), Ok(2));
        assert_eq!(count_types(&toks("Int Int Int")), Ok(3));
        assert!(count_types(&toks("(List a")).is_err());
        assert!(count_types(&toks("a)")).is_err());
        assert!(count_types(&toks("Int , Int")).is_err());
    }

    #[test]
    fn effect_variables_are_allowed_in_signatures() {
        let s = sig("[a -- b ! e] -- ! e").unwrap();
        assert_eq!(s.effects, vec!["e".to_string()]);
        match &s.ins[0] {
            Ty::Quote(_, _, effects) => assert_eq!(effects, &vec!["e".to_string()]),
            other => panic!("expected a quotation, found {:?}", other),
        }
        let s = sig("-- ! IO e").unwrap();
        assert_eq!(s.effects, vec!["IO".to_string(), "e".to_string()]);
        assert!(sig("-- ! Net").is_err());
    }

    #[test]
    fn effect_variables_can_be_found_and_renamed() {
        let t = Ty::Quote(
            vec![Ty::Int],
            vec![],
            vec!["IO".to_string(), "e".to_string()],
        );
        let mut found = Vec::new();
        effect_vars_in(&t, &mut found);
        assert_eq!(found, vec!["e".to_string()]);
        let mut map = HashMap::new();
        map.insert("e".to_string(), "?e7".to_string());
        assert_eq!(
            rename_effect_vars(&t, &map),
            Ty::Quote(
                vec![Ty::Int],
                vec![],
                vec!["IO".to_string(), "?e7".to_string()]
            )
        );
    }
}
