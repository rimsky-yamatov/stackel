//! Unification of types, with constrained type variables.
//!
//! A type variable carries a *mask*: the set of basic types it may be
//! instantiated to (see `types::NUM`, `types::ORD`, ...). This is how the
//! overloaded builtins (`add`, `lt`, `show`, ...) are typed without type classes.

use std::collections::{BTreeSet, HashMap};

use crate::types::{
    describe_mask, effect_vars_in, is_effect_var, rename_effect_vars, subst_params, var_name,
    Scheme, Ty, ANY, BOOL, CON, FLOAT, INT, QUOTE, STR,
};

#[derive(Debug, Default)]
pub struct Infer {
    bindings: Vec<Option<Ty>>,
    masks: Vec<u8>,
    rigid_names: Vec<String>,
    /// Variables for sets of effects (`?e0`, `?e1`, ...): the effects each one stands for.
    effect_bindings: Vec<Option<Vec<String>>>,
}

impl Infer {
    pub fn new() -> Self {
        Infer::default()
    }

    /// A fresh unification variable that may stand for the types in `mask`.
    pub fn fresh(&mut self, mask: u8) -> Ty {
        let id = self.bindings.len();
        self.bindings.push(None);
        self.masks.push(mask);
        Ty::Var(id)
    }

    /// A fresh rigid variable (equal only to itself), named `name` in messages.
    pub fn fresh_rigid(&mut self, name: &str) -> Ty {
        let id = self.rigid_names.len();
        self.rigid_names.push(name.to_string());
        Ty::Rigid(id)
    }

    /// A fresh variable for a set of effects, named `?e0`, `?e1`, ...
    pub fn fresh_effect(&mut self) -> String {
        let id = self.effect_bindings.len();
        self.effect_bindings.push(None);
        format!("?e{}", id)
    }

    fn effect_var_id(name: &str) -> Option<usize> {
        name.strip_prefix("?e").and_then(|n| n.parse::<usize>().ok())
    }

    /// Expands the bound effect variables in `effects`. The result is the concrete names
    /// (sorted) and the variables that are still unbound (sorted).
    pub fn resolve_effects(&self, effects: &[String]) -> (Vec<String>, Vec<String>) {
        let mut names: BTreeSet<String> = BTreeSet::new();
        let mut vars: BTreeSet<String> = BTreeSet::new();
        let mut work: Vec<String> = effects.to_vec();
        while let Some(e) = work.pop() {
            match Self::effect_var_id(&e) {
                Some(id) => match self.effect_bindings.get(id).and_then(|b| b.as_ref()) {
                    Some(bound) => work.extend(bound.iter().cloned()),
                    None => {
                        vars.insert(e);
                    }
                },
                None => {
                    names.insert(e);
                }
            }
        }
        (names.into_iter().collect(), vars.into_iter().collect())
    }

    /// The effects as one sorted list: the concrete names, then the unbound variables.
    pub fn flatten_effects(&self, effects: &[String]) -> Vec<String> {
        let (mut names, vars) = self.resolve_effects(effects);
        names.extend(vars);
        names
    }

    fn bind_effect(&mut self, var: &str, to: Vec<String>) {
        if let Some(id) = Self::effect_var_id(var) {
            self.effect_bindings[id] = Some(to);
        }
    }

    /// Makes two sets of effects equal, binding effect variables where that is needed.
    pub fn unify_effects(&mut self, a: &[String], b: &[String]) -> Result<(), String> {
        let (na, va) = self.resolve_effects(a);
        let (nb, vb) = self.resolve_effects(b);
        if na == nb && va == vb {
            return Ok(());
        }
        // A variable that occurs on both sides cancels.
        let va2: Vec<String> = va.iter().filter(|v| !vb.contains(v)).cloned().collect();
        let vb2: Vec<String> = vb.iter().filter(|v| !va.contains(v)).cloned().collect();
        let only_a: Vec<String> = na.iter().filter(|n| !nb.contains(n)).cloned().collect();
        let only_b: Vec<String> = nb.iter().filter(|n| !na.contains(n)).cloned().collect();
        let mismatch = "the effects differ".to_string();
        match (va2.is_empty(), vb2.is_empty()) {
            (true, true) => {
                if only_a.is_empty() && only_b.is_empty() {
                    Ok(())
                } else {
                    Err(mismatch)
                }
            }
            (false, true) => {
                if !only_a.is_empty() {
                    return Err(mismatch);
                }
                self.bind_effect(&va2[0], only_b);
                for v in &va2[1..] {
                    self.bind_effect(v, Vec::new());
                }
                Ok(())
            }
            (true, false) => {
                if !only_b.is_empty() {
                    return Err(mismatch);
                }
                self.bind_effect(&vb2[0], only_a);
                for v in &vb2[1..] {
                    self.bind_effect(v, Vec::new());
                }
                Ok(())
            }
            (false, false) => {
                // Each side gets what only the other side has, and both share a new rest.
                let rest = self.fresh_effect();
                let mut for_a = only_b;
                for_a.push(rest.clone());
                let mut for_b = only_a;
                for_b.push(rest);
                self.bind_effect(&va2[0], for_a);
                self.bind_effect(&vb2[0], for_b);
                for v in va2[1..].iter().chain(vb2[1..].iter()) {
                    self.bind_effect(v, Vec::new());
                }
                Ok(())
            }
        }
    }

    /// Follows variable bindings at the top level only.
    fn shallow(&self, t: &Ty) -> Ty {
        let mut cur = t.clone();
        while let Ty::Var(i) = cur {
            match &self.bindings[i] {
                Some(b) => cur = b.clone(),
                None => break,
            }
        }
        cur
    }

    /// Follows variable bindings everywhere.
    pub fn resolve(&self, t: &Ty) -> Ty {
        match self.shallow(t) {
            Ty::Quote(ins, outs, effects) => Ty::Quote(
                ins.iter().map(|x| self.resolve(x)).collect(),
                outs.iter().map(|x| self.resolve(x)).collect(),
                self.flatten_effects(&effects),
            ),
            Ty::Con(name, args) => {
                Ty::Con(name, args.iter().map(|x| self.resolve(x)).collect())
            }
            other => other,
        }
    }

    fn occurs(&self, var: usize, t: &Ty) -> bool {
        match self.shallow(t) {
            Ty::Var(i) => i == var,
            Ty::Quote(ins, outs, _) => ins.iter().chain(outs.iter()).any(|x| self.occurs(var, x)),
            Ty::Con(_, args) => args.iter().any(|x| self.occurs(var, x)),
            _ => false,
        }
    }

    fn bind(&mut self, var: usize, t: Ty, var_is_expected: bool) -> Result<(), String> {
        let mask = self.masks[var];
        let allowed = match &t {
            Ty::Int => mask & INT != 0,
            Ty::Float => mask & FLOAT != 0,
            Ty::Str => mask & STR != 0,
            Ty::Bool => mask & BOOL != 0,
            Ty::Quote(..) => mask & QUOTE != 0,
            Ty::Con(..) => mask & CON != 0,
            // A rigid variable stands for an unknown type, so it only fits an unconstrained variable.
            Ty::Rigid(_) => mask == ANY,
            Ty::Var(_) | Ty::Param(_) => unreachable!("only concrete types are bound here"),
        };
        if !allowed {
            let (expected, found) = if var_is_expected {
                (describe_mask(mask), self.show(&t))
            } else {
                (self.show(&t), describe_mask(mask))
            };
            return Err(format!("expected {}, found {}", expected, found));
        }
        if matches!(t, Ty::Quote(..) | Ty::Con(..)) && self.occurs(var, &t) {
            return Err("this would make an infinite type".to_string());
        }
        self.bindings[var] = Some(t);
        Ok(())
    }

    /// Makes `expected` and `found` the same type, or explains why that is impossible.
    pub fn unify(&mut self, expected: &Ty, found: &Ty) -> Result<(), String> {
        let e = self.shallow(expected);
        let f = self.shallow(found);
        match (&e, &f) {
            (Ty::Var(x), Ty::Var(y)) => {
                if x == y {
                    return Ok(());
                }
                let merged = self.masks[*x] & self.masks[*y];
                if merged == 0 {
                    return Err(format!(
                        "expected {}, found {}",
                        describe_mask(self.masks[*x]),
                        describe_mask(self.masks[*y])
                    ));
                }
                self.masks[*y] = merged;
                self.bindings[*x] = Some(Ty::Var(*y));
                Ok(())
            }
            (Ty::Var(x), _) => self.bind(*x, f.clone(), true),
            (_, Ty::Var(y)) => self.bind(*y, e.clone(), false),
            (Ty::Int, Ty::Int) | (Ty::Float, Ty::Float) | (Ty::Str, Ty::Str) | (Ty::Bool, Ty::Bool) => {
                Ok(())
            }
            (Ty::Rigid(a), Ty::Rigid(b)) if a == b => Ok(()),
            (Ty::Quote(i1, o1, f1), Ty::Quote(i2, o2, f2)) => {
                if i1.len() != i2.len() || o1.len() != o2.len() {
                    return Err(format!("expected {}, found {}", self.show(&e), self.show(&f)));
                }
                for (a, b) in i1.iter().zip(i2.iter()) {
                    self.unify(a, b)?;
                }
                for (a, b) in o1.iter().zip(o2.iter()) {
                    self.unify(a, b)?;
                }
                if self.unify_effects(f1, f2).is_err() {
                    return Err(format!("expected {}, found {}", self.show(&e), self.show(&f)));
                }
                Ok(())
            }
            (Ty::Con(n1, a1), Ty::Con(n2, a2)) if n1 == n2 && a1.len() == a2.len() => {
                for (a, b) in a1.iter().zip(a2.iter()) {
                    self.unify(a, b)?;
                }
                Ok(())
            }
            _ => Err(format!("expected {}, found {}", self.show(&e), self.show(&f))),
        }
    }

    /// Instantiates a scheme with fresh variables.
    pub fn instantiate(&mut self, s: &Scheme) -> (Vec<Ty>, Vec<Ty>) {
        let (ins, outs, _) = self.instantiate_with_effects(s);
        (ins, outs)
    }

    /// Instantiates a scheme with fresh variables, also for its effect variables. The third
    /// result is the effects of the word.
    pub fn instantiate_with_effects(&mut self, s: &Scheme) -> (Vec<Ty>, Vec<Ty>, Vec<String>) {
        let vars: Vec<Ty> = s.masks.iter().map(|m| self.fresh(*m)).collect();
        let ins: Vec<Ty> = s.ins.iter().map(|t| subst_params(t, &vars)).collect();
        let outs: Vec<Ty> = s.outs.iter().map(|t| subst_params(t, &vars)).collect();
        let mut found: Vec<String> = Vec::new();
        for t in ins.iter().chain(outs.iter()) {
            effect_vars_in(t, &mut found);
        }
        for e in &s.effects {
            if is_effect_var(e) && !found.contains(e) {
                found.push(e.clone());
            }
        }
        if found.is_empty() {
            return (ins, outs, s.effects.clone());
        }
        let mut map: HashMap<String, String> = HashMap::new();
        for name in found {
            let fresh = self.fresh_effect();
            map.insert(name, fresh);
        }
        let ins: Vec<Ty> = ins.iter().map(|t| rename_effect_vars(t, &map)).collect();
        let outs: Vec<Ty> = outs.iter().map(|t| rename_effect_vars(t, &map)).collect();
        let effects: Vec<String> = s
            .effects
            .iter()
            .map(|e| map.get(e).cloned().unwrap_or_else(|| e.clone()))
            .collect();
        (ins, outs, effects)
    }

    /// Turns the free variables of a stack effect into scheme parameters.
    pub fn generalize(&self, ins: &[Ty], outs: &[Ty]) -> Scheme {
        self.generalize_with_effects(ins, outs, &[])
    }

    /// Like `generalize`, for a word that performs `effects`. Effect variables that are still
    /// unbound become effect variables of the scheme (`e0`, `e1`, ...). A variable that no type
    /// of the word mentions constrains nothing, and is dropped.
    pub fn generalize_with_effects(&self, ins: &[Ty], outs: &[Ty], effects: &[String]) -> Scheme {
        let mut map: HashMap<usize, usize> = HashMap::new();
        let mut masks: Vec<u8> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        let mut emap: HashMap<String, String> = HashMap::new();
        let gi: Vec<Ty> = ins
            .iter()
            .map(|t| self.gen_ty(t, &mut map, &mut masks, &mut names, &mut emap))
            .collect();
        let go: Vec<Ty> = outs
            .iter()
            .map(|t| self.gen_ty(t, &mut map, &mut masks, &mut names, &mut emap))
            .collect();
        let mut ge: Vec<String> = Vec::new();
        for e in self.flatten_effects(effects) {
            if e.starts_with('?') {
                if let Some(name) = emap.get(&e) {
                    ge.push(name.clone());
                }
            } else {
                ge.push(e);
            }
        }
        ge.sort();
        ge.dedup();
        Scheme {
            names,
            masks,
            ins: gi,
            outs: go,
            effects: ge,
        }
    }

    fn gen_ty(
        &self,
        t: &Ty,
        map: &mut HashMap<usize, usize>,
        masks: &mut Vec<u8>,
        names: &mut Vec<String>,
        emap: &mut HashMap<String, String>,
    ) -> Ty {
        match self.shallow(t) {
            Ty::Var(i) => {
                if let Some(idx) = map.get(&i) {
                    return Ty::Param(*idx);
                }
                let idx = masks.len();
                masks.push(self.masks[i]);
                names.push(var_name(idx));
                map.insert(i, idx);
                Ty::Param(idx)
            }
            Ty::Quote(ins, outs, effects) => {
                let gi: Vec<Ty> = ins
                    .iter()
                    .map(|x| self.gen_ty(x, map, masks, names, emap))
                    .collect();
                let go: Vec<Ty> = outs
                    .iter()
                    .map(|x| self.gen_ty(x, map, masks, names, emap))
                    .collect();
                let mut ge: Vec<String> = Vec::new();
                for e in self.flatten_effects(&effects) {
                    if e.starts_with('?') {
                        let next = emap.len();
                        let name = emap
                            .entry(e)
                            .or_insert_with(|| format!("e{}", next))
                            .clone();
                        ge.push(name);
                    } else {
                        ge.push(e);
                    }
                }
                ge.sort();
                ge.dedup();
                Ty::Quote(gi, go, ge)
            }
            Ty::Con(name, args) => {
                let ga: Vec<Ty> = args
                    .iter()
                    .map(|x| self.gen_ty(x, map, masks, names, emap))
                    .collect();
                Ty::Con(name, ga)
            }
            other => other,
        }
    }

    pub fn show(&self, t: &Ty) -> String {
        let mut names = HashMap::new();
        self.show_with(t, &mut names)
    }

    /// Renders a stack effect such as `Int Int -- Int`.
    pub fn show_effect(&self, ins: &[Ty], outs: &[Ty]) -> String {
        let mut names = HashMap::new();
        self.show_effect_with(ins, outs, &[], &mut names)
    }

    /// Renders a stack effect together with its effects, such as `Int -- Int ! IO`.
    pub fn show_scheme(&self, ins: &[Ty], outs: &[Ty], effects: &[String]) -> String {
        let mut names = HashMap::new();
        let effects = self.flatten_effects(effects);
        self.show_effect_with(ins, outs, &effects, &mut names)
    }

    fn show_with(&self, t: &Ty, names: &mut HashMap<usize, String>) -> String {
        match self.shallow(t) {
            Ty::Int => "Int".to_string(),
            Ty::Float => "Float".to_string(),
            Ty::Str => "String".to_string(),
            Ty::Bool => "Bool".to_string(),
            Ty::Var(i) => {
                let n = names.len();
                names.entry(i).or_insert_with(|| var_name(n)).clone()
            }
            Ty::Rigid(i) => self.rigid_names[i].clone(),
            Ty::Param(i) => var_name(i),
            Ty::Quote(ins, outs, effects) => {
                let effects = self.flatten_effects(&effects);
                format!("[{}]", self.show_effect_with(&ins, &outs, &effects, names))
            }
            Ty::Con(name, args) => {
                if args.is_empty() {
                    name
                } else {
                    let shown: Vec<String> =
                        args.iter().map(|x| self.show_with(x, names)).collect();
                    format!("({} {})", name, shown.join(" "))
                }
            }
        }
    }

    fn show_effect_with(
        &self,
        ins: &[Ty],
        outs: &[Ty],
        effects: &[String],
        names: &mut HashMap<usize, String>,
    ) -> String {
        let i: Vec<String> = ins.iter().map(|t| self.show_with(t, names)).collect();
        let o: Vec<String> = outs.iter().map(|t| self.show_with(t, names)).collect();
        let mut s = String::new();
        if !i.is_empty() {
            s.push_str(&i.join(" "));
            s.push(' ');
        }
        s.push_str("--");
        if !o.is_empty() {
            s.push(' ');
            s.push_str(&o.join(" "));
        }
        if !effects.is_empty() {
            let shown: Vec<&str> = effects.iter().map(|e| e.trim_start_matches('?')).collect();
            s.push_str(" ! ");
            s.push_str(&shown.join(" "));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BASIC, NUM, ORD};

    #[test]
    fn unify_binds_variables() {
        let mut i = Infer::new();
        let a = i.fresh(ANY);
        i.unify(&Ty::Int, &a).unwrap();
        assert_eq!(i.resolve(&a), Ty::Int);
        assert!(i.unify(&a, &Ty::Str).is_err());
    }

    #[test]
    fn concrete_mismatch() {
        let mut i = Infer::new();
        let e = i.unify(&Ty::Int, &Ty::Str).unwrap_err();
        assert_eq!(e, "expected Int, found String");
    }

    #[test]
    fn constraints_restrict_variables() {
        let mut i = Infer::new();
        let n = i.fresh(NUM);
        assert!(i.unify(&n, &Ty::Str).is_err());
        assert!(i.unify(&n, &Ty::Bool).is_err());
        assert!(i.unify(&n, &Ty::Float).is_ok());
        assert_eq!(i.resolve(&n), Ty::Float);
    }

    #[test]
    fn constraints_are_intersected_when_variables_merge() {
        let mut i = Infer::new();
        let n = i.fresh(NUM);
        let o = i.fresh(ORD);
        i.unify(&n, &o).unwrap();
        assert!(i.unify(&o, &Ty::Str).is_err());
        assert!(i.unify(&o, &Ty::Int).is_ok());
        let b = i.fresh(BASIC);
        let s = i.fresh(NUM);
        assert!(i.unify(&b, &s).is_ok());
    }

    #[test]
    fn rigid_variables() {
        let mut i = Infer::new();
        let r = i.fresh_rigid("a");
        assert!(i.unify(&r, &Ty::Int).is_err());
        assert!(i.unify(&r, &r).is_ok());
        let v = i.fresh(ANY);
        assert!(i.unify(&v, &r).is_ok());
        let n = i.fresh(NUM);
        assert!(i.unify(&n, &r).is_err());
        let other = i.fresh_rigid("b");
        assert!(i.unify(&r, &other).is_err());
    }

    #[test]
    fn quotation_types_unify_structurally() {
        let mut i = Infer::new();
        let a = i.fresh(ANY);
        let q1 = Ty::Quote(vec![a.clone()], vec![Ty::Int], vec![]);
        let q2 = Ty::Quote(vec![Ty::Str], vec![Ty::Int], vec![]);
        i.unify(&q1, &q2).unwrap();
        assert_eq!(i.resolve(&a), Ty::Str);
        let q3 = Ty::Quote(vec![], vec![Ty::Int], vec![]);
        assert!(i.unify(&q2, &q3).is_err());
    }

    #[test]
    fn quotation_effects_must_be_equal() {
        let mut i = Infer::new();
        let pure = Ty::Quote(vec![Ty::Str], vec![], vec![]);
        let io = Ty::Quote(vec![Ty::Str], vec![], vec!["IO".to_string()]);
        let e = i.unify(&pure, &io).unwrap_err();
        assert_eq!(e, "expected [String --], found [String -- ! IO]");
        assert!(i.unify(&io, &io).is_ok());
    }

    #[test]
    fn data_types_unify_structurally() {
        let mut i = Infer::new();
        let a = i.fresh(ANY);
        let opt_a = Ty::Con("Option".to_string(), vec![a.clone()]);
        let opt_int = Ty::Con("Option".to_string(), vec![Ty::Int]);
        i.unify(&opt_a, &opt_int).unwrap();
        assert_eq!(i.resolve(&a), Ty::Int);
        let color = Ty::Con("Color".to_string(), vec![]);
        assert_eq!(
            i.unify(&opt_int, &color).unwrap_err(),
            "expected (Option Int), found Color"
        );
        let n = i.fresh(NUM);
        assert_eq!(
            i.unify(&n, &color).unwrap_err(),
            "expected a number (Int or Float), found Color"
        );
        let any = i.fresh(ANY);
        assert!(i.unify(&any, &color).is_ok());
    }

    #[test]
    fn data_types_have_an_occurs_check() {
        let mut i = Infer::new();
        let v = i.fresh(ANY);
        let list_v = Ty::Con("List".to_string(), vec![v.clone()]);
        assert!(i.unify(&v, &list_v).is_err());
    }

    #[test]
    fn occurs_check() {
        let mut i = Infer::new();
        let v = i.fresh(ANY);
        let q = Ty::Quote(vec![v.clone()], vec![], vec![]);
        assert!(i.unify(&v, &q).is_err());
    }

    #[test]
    fn show_names_variables_in_order_of_appearance() {
        let mut i = Infer::new();
        let a = i.fresh(ANY);
        let b = i.fresh(ANY);
        assert_eq!(
            i.show_effect(&[b.clone(), a.clone()], &[a, b, Ty::Int]),
            "a b -- b a Int"
        );
        assert_eq!(i.show_effect(&[], &[]), "--");
        assert_eq!(i.show_effect(&[Ty::Int], &[]), "Int --");
        assert_eq!(
            i.show(&Ty::Quote(vec![Ty::Int], vec![Ty::Str], vec![])),
            "[Int -- String]"
        );
    }

    #[test]
    fn generalize_then_instantiate_gives_fresh_variables() {
        let mut i = Infer::new();
        let a = i.fresh(NUM);
        let s = i.generalize(&[a.clone()], &[a.clone(), a]);
        assert_eq!(s.masks, vec![NUM]);
        let (ins1, _) = i.instantiate(&s);
        let (ins2, _) = i.instantiate(&s);
        i.unify(&ins1[0], &Ty::Int).unwrap();
        i.unify(&ins2[0], &Ty::Float).unwrap();
    }

    #[test]
    fn effect_variables_unify_with_effect_sets() {
        let mut i = Infer::new();
        let e = i.fresh_effect();
        // an unbound variable takes the effects of the other side
        i.unify_effects(&[e.clone()], &["IO".to_string()]).unwrap();
        assert_eq!(i.flatten_effects(&[e.clone()]), vec!["IO".to_string()]);
        // now it stands for IO
        assert!(i.unify_effects(&[e.clone()], &[]).is_err());
        assert!(i.unify_effects(&[e], &["IO".to_string()]).is_ok());
    }

    #[test]
    fn two_open_effect_sets_share_a_rest() {
        let mut i = Infer::new();
        let a = i.fresh_effect();
        let b = i.fresh_effect();
        let left = vec![a, "IO".to_string()];
        let right = vec![b, "Fs".to_string()];
        i.unify_effects(&left, &right).unwrap();
        let rl = i.flatten_effects(&left);
        let rr = i.flatten_effects(&right);
        assert_eq!(rl, rr);
        assert!(rl.contains(&"IO".to_string()) && rl.contains(&"Fs".to_string()));
    }

    #[test]
    fn a_closed_set_cannot_absorb_other_effects() {
        let mut i = Infer::new();
        let a = i.fresh_effect();
        assert!(i
            .unify_effects(&[a, "IO".to_string()], &["Fs".to_string()])
            .is_err());
    }

    #[test]
    fn quotation_types_unify_their_effect_variables() {
        let mut i = Infer::new();
        let e = i.fresh_effect();
        let open = Ty::Quote(vec![Ty::Str], vec![], vec![e]);
        let io = Ty::Quote(vec![Ty::Str], vec![], vec!["IO".to_string()]);
        i.unify(&open, &io).unwrap();
        assert_eq!(i.resolve(&open), io);
    }

    #[test]
    fn schemes_are_instantiated_with_fresh_effect_variables() {
        let mut i = Infer::new();
        let s = Scheme {
            names: vec![],
            masks: vec![],
            ins: vec![Ty::Quote(vec![], vec![], vec!["e".to_string()])],
            outs: vec![],
            effects: vec!["e".to_string()],
        };
        let (ins1, _, eff1) = i.instantiate_with_effects(&s);
        let (_, _, eff2) = i.instantiate_with_effects(&s);
        assert_ne!(eff1, eff2);
        match &ins1[0] {
            Ty::Quote(_, _, effects) => assert_eq!(effects, &eff1),
            other => panic!("expected a quotation, found {:?}", other),
        }
    }

    #[test]
    fn generalization_keeps_the_effect_variables_that_types_mention() {
        let mut i = Infer::new();
        let e = i.fresh_effect();
        let q = Ty::Quote(vec![], vec![], vec![e.clone()]);
        let s = i.generalize_with_effects(&[q], &[], &[e]);
        assert_eq!(s.effects, vec!["e0".to_string()]);
        assert_eq!(s.ins[0], Ty::Quote(vec![], vec![], vec!["e0".to_string()]));
        let lost = i.fresh_effect();
        let s = i.generalize_with_effects(&[], &[], &[lost]);
        assert!(s.effects.is_empty());
    }
}
