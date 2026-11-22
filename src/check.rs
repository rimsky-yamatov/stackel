//! Static checker: infers and checks stack-effect types before a program runs.
//!
//! The checker walks a word's body while keeping a symbolic stack. Each word
//! applied to the stack unifies its inputs with the values on top; if the
//! stack does not hold enough values, the missing ones become inputs of the
//! word being checked (this is how stack effects are inferred).
//!
//! * A word with a signature is checked against it, starting from a stack that
//!   holds exactly its declared inputs.
//! * A word without a signature is inferred (and generalized) on first use.
//!   Recursive words need a signature.
//! * `main` must have the type `--`.
//!
//! Effects (`! IO`) are tracked alongside the stack: a word may only perform
//! the effects its signature declares. Applying a quotation performs the
//! effects in the quotation's type; a quotation literal performs nothing
//! until it is applied.
//!
//! Limitations (to be lifted in later milestones): there are no explicit row
//! variables, so quotation types are exact (including their effects); `apply`
//! needs the type of the quotation to be known.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::ast::{Arm, Def, Node, Program};
use crate::builtins;
use crate::error::{Error, Result};
use crate::infer::Infer;
use crate::types::{is_effect_var, parse_fields, parse_sig, subst_params, Adts, Scheme, Ty, ANY};

type Pos = (usize, usize);

/// The effects performed so far, each with the word that performs it first.
type Effects = BTreeMap<String, (String, Pos)>;

/// The symbolic stack: `ins` are the values the word needs from below, `outs` the current top part.
#[derive(Clone)]
struct State {
    ins: Vec<Ty>,
    outs: Vec<Ty>,
    effects: Effects,
}

impl State {
    fn empty() -> Self {
        State {
            ins: Vec::new(),
            outs: Vec::new(),
            effects: BTreeMap::new(),
        }
    }
}

fn add_effects(st: &mut State, effects: &[String], word: &str, pos: Pos) {
    for effect in effects {
        st.effects
            .entry(effect.clone())
            .or_insert_with(|| (word.to_string(), pos));
    }
}

fn type_error(what: &str, pos: Pos, msg: &str) -> Error {
    Error::new(format!("type error in {}: {}", what, msg), pos.0, pos.1)
}

/// How to talk about the two sides when two branches are merged (a guard or `match` arms).
struct Branches<'a> {
    /// The construct, as in "type error in ...".
    what: &'a str,
    /// Both sides, as in "... do not match".
    pair: &'a str,
    left: &'a str,
    right: &'a str,
}

struct Checker<'p> {
    infer: Infer,
    defs: HashMap<&'p str, &'p Def>,
    declared: HashMap<String, Scheme>,
    inferred: HashMap<String, Scheme>,
    builtin: HashMap<String, Scheme>,
    in_progress: HashSet<String>,
    /// The data types: name -> number of type parameters.
    adts: Adts,
    /// Constructor name -> (data type name, type of the constructor word).
    ctors: HashMap<String, (String, Scheme)>,
    /// Data type name -> its constructors, in declaration order.
    adt_ctors: HashMap<String, Vec<String>>,
    /// The local names (`let`) that are in scope, innermost last.
    locals: Vec<(String, Ty)>,
}

/// Checks the types of all words of `program`.
/// The type of the word `name`, declared or inferred, as text such as `Int -- Int ! IO`.
pub fn type_of(program: &Program, name: &str) -> Result<String> {
    let mut checker = Checker::new(program)?;
    for def in &program.defs {
        checker.check_def(def)?;
    }
    let scheme = match checker
        .declared
        .get(name)
        .or_else(|| checker.inferred.get(name))
    {
        Some(s) => s.clone(),
        None => {
            return Err(Error::runtime(format!("no word `{}` is defined", name)));
        }
    };
    let (ins, outs, effects) = checker.infer.instantiate_with_effects(&scheme);
    Ok(checker.infer.show_scheme(&ins, &outs, &effects))
}

pub fn check_program(program: &Program) -> Result<()> {
    let mut checker = Checker::new(program)?;
    for def in &program.defs {
        checker.check_def(def)?;
    }
    checker.check_main()?;
    checker.check_ceilings(program)
}

impl<'p> Checker<'p> {
    fn new(program: &'p Program) -> Result<Self> {
        let mut adts = Adts::new();
        for t in &program.types {
            if matches!(t.name.as_str(), "Int" | "Float" | "String" | "Bool") {
                return Err(Error::new(
                    format!("`{}` is a builtin type and cannot be redefined", t.name),
                    t.line,
                    1,
                ));
            }
            if adts.contains_key(&t.name) {
                return Err(Error::new(
                    format!("type `{}` is defined more than once", t.name),
                    t.line,
                    1,
                ));
            }
            for (i, p) in t.params.iter().enumerate() {
                if t.params[..i].contains(p) {
                    return Err(Error::new(
                        format!("duplicate type parameter `{}` in type `{}`", p, t.name),
                        t.line,
                        1,
                    ));
                }
            }
            adts.insert(t.name.clone(), t.params.len());
        }
        let mut ctors: HashMap<String, (String, Scheme)> = HashMap::new();
        let mut adt_ctors: HashMap<String, Vec<String>> = HashMap::new();
        for t in &program.types {
            let mut names = Vec::new();
            for c in &t.ctors {
                if ctors.contains_key(&c.name) {
                    return Err(Error::new(
                        format!("constructor `{}` is defined more than once", c.name),
                        c.line,
                        1,
                    ));
                }
                let fields = parse_fields(&c.fields, &t.params, &adts).map_err(|m| {
                    Error::new(format!("{} (in constructor `{}`)", m, c.name), c.line, 1)
                })?;
                let args: Vec<Ty> = (0..t.params.len()).map(Ty::Param).collect();
                let scheme = Scheme {
                    names: t.params.clone(),
                    masks: vec![ANY; t.params.len()],
                    ins: fields,
                    outs: vec![Ty::Con(t.name.clone(), args)],
                    effects: Vec::new(),
                };
                ctors.insert(c.name.clone(), (t.name.clone(), scheme));
                names.push(c.name.clone());
            }
            adt_ctors.insert(t.name.clone(), names);
        }

        let mut declared: HashMap<String, Scheme> = HashMap::new();
        for sig in &program.sigs {
            if declared.contains_key(&sig.name) {
                return Err(Error::new(
                    format!("duplicate type signature for `{}`", sig.name),
                    sig.line,
                    1,
                ));
            }
            let scheme = parse_sig(&sig.tokens, &[], &adts).map_err(|m| {
                Error::new(
                    format!("{} (in the signature of `{}`)", m, sig.name),
                    sig.line,
                    1,
                )
            })?;
            declared.insert(sig.name.clone(), scheme);
        }
        let defs: HashMap<&'p str, &'p Def> =
            program.defs.iter().map(|d| (d.name.as_str(), d)).collect();
        Ok(Checker {
            infer: Infer::new(),
            defs,
            declared,
            inferred: HashMap::new(),
            builtin: builtins::signatures(),
            in_progress: HashSet::new(),
            adts,
            ctors,
            adt_ctors,
            locals: Vec::new(),
        })
    }

    fn check_def(&mut self, def: &Def) -> Result<()> {
        if let Some(scheme) = self.declared.get(&def.name).cloned() {
            self.check_declared(def, &scheme)
        } else if self.inferred.contains_key(&def.name) {
            Ok(())
        } else {
            self.infer_word(&def.name)
        }
    }

    fn check_main(&self) -> Result<()> {
        let def = match self.defs.get("main") {
            Some(d) => *d,
            None => return Ok(()),
        };
        let scheme = match self
            .declared
            .get("main")
            .or_else(|| self.inferred.get("main"))
        {
            Some(s) => s,
            None => return Ok(()),
        };
        if !scheme.ins.is_empty() || !scheme.outs.is_empty() {
            return Err(Error::new(
                format!(
                    "`main` must have the type `--` (it takes nothing and leaves nothing), but it has the type `{}`",
                    self.infer.show_effect(&scheme.ins, &scheme.outs)
                ),
                def.line,
                1,
            ));
        }
        Ok(())
    }

    /// The words of a package whose effects are limited (`allow` in `stackel.toml`) may only
    /// perform the effects that the package is allowed.
    fn check_ceilings(&self, program: &Program) -> Result<()> {
        let mut names: Vec<&String> = program.origins.keys().collect();
        names.sort();
        for name in names {
            let origin = &program.origins[name];
            let scheme = match self.declared.get(name).or_else(|| self.inferred.get(name)) {
                Some(s) => s,
                None => continue,
            };
            // Effect variables are not effects: the word only passes on those of its quotations.
            if let Some(effect) = scheme
                .effects
                .iter()
                .find(|e| !is_effect_var(e) && !origin.allow.contains(*e))
            {
                let line = self.defs.get(name.as_str()).map(|d| d.line).unwrap_or(0);
                let allowed = if origin.allow.is_empty() {
                    "no effects".to_string()
                } else {
                    format!("only {}", origin.allow.join(", "))
                };
                return Err(Error::new(
                    format!(
                        "the package `{}` may perform {} (see `allow` in stackel.toml), but the word `{}` performs the `{}` effect",
                        origin.package, allowed, name, effect
                    ),
                    line,
                    1,
                ));
            }
        }
        Ok(())
    }

    /// Checks a word against its declared signature.
    fn check_declared(&mut self, def: &Def, scheme: &Scheme) -> Result<()> {
        let rigid: Vec<Ty> = scheme
            .names
            .iter()
            .map(|n| self.infer.fresh_rigid(n))
            .collect();
        let ins: Vec<Ty> = scheme.ins.iter().map(|t| subst_params(t, &rigid)).collect();
        let outs: Vec<Ty> = scheme
            .outs
            .iter()
            .map(|t| subst_params(t, &rigid))
            .collect();
        let pos: Pos = (def.line, 1);

        let mut st = State {
            ins: ins.clone(),
            outs: ins.clone(),
            effects: BTreeMap::new(),
        };
        self.infer_nodes(&def.body, &mut st)?;

        let declared = self.infer.show_effect(&ins, &outs);
        if st.ins.len() > ins.len() {
            return Err(Error::new(
                format!(
                    "word `{}` takes {} more value(s) from the stack than its signature declares (`{}`)",
                    def.name,
                    st.ins.len() - ins.len(),
                    declared
                ),
                pos.0,
                pos.1,
            ));
        }
        if st.outs.len() != outs.len() {
            return Err(Error::new(
                format!(
                    "word `{}` leaves {} value(s) on the stack, but its signature declares {} (`{}`)",
                    def.name,
                    st.outs.len(),
                    outs.len(),
                    declared
                ),
                pos.0,
                pos.1,
            ));
        }
        for (want, got) in outs.iter().zip(st.outs.iter()) {
            self.infer
                .unify(want, got)
                .map_err(|m| type_error(&format!("the result of `{}`", def.name), pos, &m))?;
        }
        for (effect, (word, at)) in &st.effects {
            for concrete in self.infer.flatten_effects(&[effect.clone()]) {
                // An effect variable that nothing determined stands for no effect at all.
                if concrete.starts_with('?') {
                    continue;
                }
                if !scheme.effects.contains(&concrete) {
                    return Err(Error::new(
                        format!(
                            "type error in `{}`: this performs the `{}` effect, but word `{}` does not declare it (add `! {}` to its signature)",
                            word, concrete, def.name, concrete
                        ),
                        at.0,
                        at.1,
                    ));
                }
            }
        }
        Ok(())
    }

    /// Infers the type of a word without a signature and records its generalized scheme.
    fn infer_word(&mut self, name: &str) -> Result<()> {
        let def = match self.defs.get(name).copied() {
            Some(d) => d,
            None => return Ok(()),
        };
        self.in_progress.insert(name.to_string());
        let mut st = State::empty();
        // The local names of the word that is being checked are not visible in this one.
        let saved = std::mem::take(&mut self.locals);
        let result = self.infer_nodes(&def.body, &mut st);
        self.locals = saved;
        self.in_progress.remove(name);
        result?;
        let effects: Vec<String> = st.effects.keys().cloned().collect();
        let scheme = self
            .infer
            .generalize_with_effects(&st.ins, &st.outs, &effects);
        self.inferred.insert(name.to_string(), scheme);
        Ok(())
    }

    /// The instantiated stack effect and the effects of a word that is being called.
    fn word_type(&mut self, name: &str, pos: Pos) -> Result<(Vec<Ty>, Vec<Ty>, Vec<String>)> {
        let known = self
            .declared
            .get(name)
            .or_else(|| self.inferred.get(name))
            .or_else(|| self.builtin.get(name))
            .or_else(|| self.ctors.get(name).map(|(_, s)| s))
            .cloned();
        if let Some(scheme) = known {
            return Ok(self.infer.instantiate_with_effects(&scheme));
        }
        if self.in_progress.contains(name) {
            return Err(Error::new(
                format!("recursive word `{}` needs a type signature", name),
                pos.0,
                pos.1,
            ));
        }
        if self.defs.contains_key(name) {
            self.infer_word(name)?;
            let scheme = self
                .inferred
                .get(name)
                .cloned()
                .expect("the word was just inferred");
            return Ok(self.infer.instantiate_with_effects(&scheme));
        }
        Err(Error::new(format!("unknown word `{}`", name), pos.0, pos.1))
    }

    /// Applies a stack effect to the symbolic stack.
    fn compose(
        &mut self,
        st: &mut State,
        ins: &[Ty],
        outs: &[Ty],
        what: &str,
        pos: Pos,
    ) -> Result<()> {
        let n = ins.len();
        let m = st.outs.len();
        // `k` inputs are not on the symbolic stack: they come from below it.
        let k = n.saturating_sub(m);
        let have = n - k;
        for i in 0..have {
            let expected = &ins[k + i];
            let found = &st.outs[m - have + i];
            self.infer
                .unify(expected, found)
                .map_err(|msg| type_error(what, pos, &msg))?;
        }
        st.outs.truncate(m - have);
        if k > 0 {
            let mut new_ins: Vec<Ty> = ins[..k].to_vec();
            new_ins.extend(st.ins.drain(..));
            st.ins = new_ins;
        }
        st.outs.extend(outs.iter().cloned());
        Ok(())
    }

    /// `apply`: runs the quotation on top of the stack.
    fn apply_quotation(&mut self, st: &mut State, pos: Pos) -> Result<()> {
        let top = match st.outs.last() {
            Some(t) => self.infer.resolve(t),
            None => {
                return Err(type_error(
                    "`apply`",
                    pos,
                    "the quotation to apply is not known here; give this word a type signature",
                ))
            }
        };
        match top {
            Ty::Quote(ins, outs, effects) => {
                st.outs.pop();
                self.compose(st, &ins, &outs, "`apply`", pos)?;
                add_effects(st, &effects, "apply", pos);
                Ok(())
            }
            Ty::Var(_) => Err(type_error(
                "`apply`",
                pos,
                "the type of the quotation is not known; give this word a type signature",
            )),
            other => {
                let msg = format!("expected a quotation, found {}", self.infer.show(&other));
                Err(type_error("`apply`", pos, &msg))
            }
        }
    }

    /// The type of a local name (`let`), if `name` is one; inner bindings hide outer ones.
    fn local_type(&self, name: &str) -> Option<Ty> {
        self.locals
            .iter()
            .rev()
            .find(|(n, _)| n.as_str() == name)
            .map(|(_, t)| t.clone())
    }

    fn infer_nodes(&mut self, nodes: &[Node], st: &mut State) -> Result<()> {
        for node in nodes {
            match node {
                Node::Int(_) => st.outs.push(Ty::Int),
                Node::Float(_) => st.outs.push(Ty::Float),
                Node::Str(_) => st.outs.push(Ty::Str),
                Node::Quote(body) => {
                    let mut qs = State::empty();
                    self.infer_nodes(body, &mut qs)?;
                    let keys: Vec<String> = qs.effects.keys().cloned().collect();
                    let effects = self.infer.flatten_effects(&keys);
                    st.outs.push(Ty::Quote(qs.ins, qs.outs, effects));
                }
                Node::Word { name, line, col } => {
                    let pos: Pos = (*line, *col);
                    if let Some(ty) = self.local_type(name) {
                        st.outs.push(ty);
                    } else if name == "apply" {
                        self.apply_quotation(st, pos)?;
                    } else {
                        let (ins, outs, effects) = self.word_type(name, pos)?;
                        self.compose(st, &ins, &outs, &format!("`{}`", name), pos)?;
                        add_effects(st, &effects, name, pos);
                    }
                }
                Node::Match { arms, line, col } => {
                    let pos: Pos = (*line, *col);
                    let merged = self.infer_match(st, arms, pos)?;
                    *st = merged;
                }
                Node::If {
                    then_branch,
                    else_branch,
                    line,
                    col,
                } => {
                    let pos: Pos = (*line, *col);
                    self.compose(st, &[Ty::Bool], &[], "guard", pos)?;
                    let mut then_state = st.clone();
                    self.infer_nodes(then_branch, &mut then_state)?;
                    let mut else_state = st.clone();
                    self.infer_nodes(else_branch, &mut else_state)?;
                    let labels = Branches {
                        what: "guard",
                        pair: "the `then` and `else` branches",
                        left: "the `then` branch",
                        right: "the `else` branch",
                    };
                    *st = self.merge_branches(then_state, else_state, pos, &labels)?;
                }
                Node::Let {
                    names,
                    body,
                    line,
                    col,
                } => {
                    let pos: Pos = (*line, *col);
                    let vars: Vec<Ty> = names.iter().map(|_| self.infer.fresh(ANY)).collect();
                    self.compose(st, &vars, &[], "`let`", pos)?;
                    let base = self.locals.len();
                    for (name, ty) in names.iter().zip(vars) {
                        self.locals.push((name.clone(), ty));
                    }
                    let result = self.infer_nodes(body, st);
                    self.locals.truncate(base);
                    result?;
                }
            }
        }
        Ok(())
    }

    /// `match`: pops a data value, then checks every arm against the constructor's fields.
    /// The arms must cover every constructor once and must all have the same stack effect.
    fn infer_match(&mut self, st: &mut State, arms: &[Arm], pos: Pos) -> Result<State> {
        // The first constructor tells which data type is matched.
        let first = &arms[0];
        let ty_name = match self.ctors.get(&first.ctor) {
            Some((ty, _)) => ty.clone(),
            None => {
                return Err(Error::new(
                    format!("unknown constructor `{}`", first.ctor),
                    first.line,
                    first.col,
                ))
            }
        };
        let arity = self.adts.get(&ty_name).copied().unwrap_or(0);
        let args: Vec<Ty> = (0..arity).map(|_| self.infer.fresh(ANY)).collect();
        let scrutinee = Ty::Con(ty_name.clone(), args.clone());
        self.compose(st, &[scrutinee], &[], "`match`", pos)?;

        let all: Vec<String> = self.adt_ctors.get(&ty_name).cloned().unwrap_or_default();
        let mut seen: HashSet<&str> = HashSet::new();
        for arm in arms {
            match self.ctors.get(&arm.ctor) {
                Some((ty, _)) if *ty == ty_name => {}
                Some((ty, _)) => {
                    return Err(Error::new(
                        format!(
                            "constructor `{}` belongs to type `{}`, not `{}`",
                            arm.ctor, ty, ty_name
                        ),
                        arm.line,
                        arm.col,
                    ))
                }
                None => {
                    return Err(Error::new(
                        format!("unknown constructor `{}`", arm.ctor),
                        arm.line,
                        arm.col,
                    ))
                }
            }
            if !seen.insert(arm.ctor.as_str()) {
                return Err(Error::new(
                    format!("duplicate arm for the constructor `{}`", arm.ctor),
                    arm.line,
                    arm.col,
                ));
            }
        }
        let missing: Vec<&str> = all
            .iter()
            .map(|c| c.as_str())
            .filter(|c| !seen.contains(c))
            .collect();
        if !missing.is_empty() {
            return Err(Error::new(
                format!(
                    "`match` on type `{}` is not exhaustive: missing {}",
                    ty_name,
                    missing.join(", ")
                ),
                pos.0,
                pos.1,
            ));
        }

        let mut merged: Option<State> = None;
        for arm in arms {
            let (_, scheme) = self
                .ctors
                .get(&arm.ctor)
                .cloned()
                .expect("the constructor was checked above");
            let mut arm_state = st.clone();
            for field in &scheme.ins {
                arm_state.outs.push(subst_params(field, &args));
            }
            self.infer_nodes(&arm.body, &mut arm_state)?;
            merged = Some(match merged {
                None => arm_state,
                Some(previous) => {
                    // Earlier arms were already unified with the first one.
                    let left = format!("the arm for `{}`", arms[0].ctor);
                    let right = format!("the arm for `{}`", arm.ctor);
                    let labels = Branches {
                        what: "`match`",
                        pair: "the `match` arms",
                        left: &left,
                        right: &right,
                    };
                    self.merge_branches(previous, arm_state, pos, &labels)?
                }
            });
        }
        Ok(merged.expect("a match has at least one arm"))
    }

    /// Both branches of a guard (or all arms of a `match`) must have the same stack effect.
    fn merge_branches(
        &mut self,
        mut t: State,
        mut e: State,
        pos: Pos,
        labels: &Branches,
    ) -> Result<State> {
        // The branch that took fewer inputs leaves those values untouched: pad it.
        let (lt, le) = (t.ins.len(), e.ins.len());
        if lt < le {
            self.pad(&mut t, le - lt);
        } else if le < lt {
            self.pad(&mut e, lt - le);
        }
        if t.outs.len() != e.outs.len() {
            let msg = format!(
                "{} has the type `{}`, but {} has the type `{}`",
                labels.left,
                self.infer.show_effect(&t.ins, &t.outs),
                labels.right,
                self.infer.show_effect(&e.ins, &e.outs)
            );
            return Err(type_error(labels.what, pos, &msg));
        }
        let pairs = t
            .ins
            .iter()
            .zip(e.ins.iter())
            .chain(t.outs.iter().zip(e.outs.iter()));
        for (a, b) in pairs {
            self.infer.unify(a, b).map_err(|m| {
                type_error(
                    labels.what,
                    pos,
                    &format!("{} do not match: {}", labels.pair, m),
                )
            })?;
        }
        // Either branch may run, so the guard may perform the effects of both.
        for (name, origin) in e.effects {
            t.effects.entry(name).or_insert(origin);
        }
        Ok(t)
    }

    /// Adds `p` untouched values at the bottom of a state.
    fn pad(&mut self, st: &mut State, p: usize) {
        let fresh: Vec<Ty> = (0..p).map(|_| self.infer.fresh(ANY)).collect();
        let mut ins = fresh.clone();
        ins.extend(st.ins.drain(..));
        st.ins = ins;
        let mut outs = fresh;
        outs.extend(st.outs.drain(..));
        st.outs = outs;
    }
}
