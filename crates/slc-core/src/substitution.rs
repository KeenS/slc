//! α-equivalence, fresh variable generation, and capture-avoiding substitution.

use crate::command::Command;
use crate::coterm::{CoCaseBranch, CoTerm};
use crate::term::{CoMatchBranch, Term};
use std::borrow::Cow;
use std::collections::HashSet;

/// Generate a fresh variable name not in `used`.
pub fn fresh(base: &str, used: &mut HashSet<String>) -> String {
    let mut n = 0;
    loop {
        let candidate = if n == 0 { base.to_string() } else { format!("{base}{n}") };
        if !used.contains(&candidate) {
            used.insert(candidate.clone());
            return candidate;
        }
        n += 1;
    }
}

/// Collect the free variables of a term.
pub fn free_vars_term(t: &Term) -> HashSet<String> {
    let mut out = HashSet::new();
    go_term(t, &mut out);
    out
}

/// Collect the free variables of a co-term. Term variables and covariables
/// share one namespace, so both kinds are included.
pub fn free_vars_coterm(e: &CoTerm) -> HashSet<String> {
    let mut out = HashSet::new();
    go_coterm(e, &mut out);
    out
}

/// Collect the free variables of a command.
pub fn free_vars_command(c: &Command) -> HashSet<String> {
    let mut out = HashSet::new();
    go_command(c, &mut out);
    out
}

fn go_term(t: &Term, out: &mut HashSet<String>) {
    match t {
        Term::Var(x) => {
            out.insert(x.clone());
        }
        Term::Lam(x, body) => {
            let mut inner = HashSet::new();
            go_term(body, &mut inner);
            inner.remove(x);
            out.extend(inner);
        }
        Term::Mu(a, c) => {
            let mut inner = HashSet::new();
            go_command(c, &mut inner);
            inner.remove(a);
            out.extend(inner);
        }
        Term::Tuple(items) => {
            for item in items {
                go_term(item, out);
            }
        }
        Term::Tag(_, t) => go_term(t, out),
        Term::CoMatch { branches, .. } => {
            for branch in branches {
                let mut inner = HashSet::new();
                go_command(&branch.body, &mut inner);
                inner.remove(&branch.binder);
                out.extend(inner);
            }
        }
        Term::Co(e) => go_coterm(e, out),
    }
}

fn go_coterm(e: &CoTerm, out: &mut HashSet<String>) {
    match e {
        CoTerm::Covar(x) => {
            out.insert(x.clone());
        }
        CoTerm::App(v, e) => {
            go_term(v, out);
            go_coterm(e, out);
        }
        CoTerm::MuTilde(x, c) => {
            let mut inner = HashSet::new();
            go_command(c, &mut inner);
            inner.remove(x);
            out.extend(inner);
        }
        CoTerm::Prj(_) => {}
        CoTerm::CoCase { branches, .. } => {
            for branch in branches {
                let mut inner = HashSet::new();
                go_command(&branch.body, &mut inner);
                for binder in &branch.binders {
                    inner.remove(binder);
                }
                out.extend(inner);
            }
        }
        CoTerm::MuTildeTensor(binders, c) => {
            let mut inner = HashSet::new();
            go_command(c, &mut inner);
            for binder in binders {
                inner.remove(binder);
            }
            out.extend(inner);
        }
        CoTerm::Dtor(_, e) => go_coterm(e, out),
    }
}

fn go_command(c: &Command, out: &mut HashSet<String>) {
    match c {
        Command::Cut(t, e) => {
            go_term(t, out);
            go_coterm(e, out);
        }
    }
}

/// α-equivalence for terms.
pub fn alpha_eq_term(a: &Term, b: &Term) -> bool {
    alpha_term(a, b, &mut Vec::new(), &mut Vec::new())
}

fn var_eq(x: &str, y: &str, xs: &[String], ys: &[String]) -> bool {
    // Binders are pushed outer-to-inner. `position` would see the outer
    // binding, so a shadowed use would not match the binder that actually
    // scopes over it. Term variables and covariables share this stack.
    match (xs.iter().rposition(|p| p == x), ys.iter().rposition(|p| p == y)) {
        (Some(i), Some(j)) => i == j,
        (None, None) => x == y,
        _ => false,
    }
}

fn alpha_term(a: &Term, b: &Term, xs: &mut Vec<String>, ys: &mut Vec<String>) -> bool {
    match (a, b) {
        (Term::Var(x), Term::Var(y)) => var_eq(x, y, xs, ys),
        (Term::Lam(x, t1), Term::Lam(y, t2)) => {
            xs.push(x.clone());
            ys.push(y.clone());
            let r = alpha_term(t1, t2, xs, ys);
            xs.pop();
            ys.pop();
            r
        }
        (Term::Mu(x, c1), Term::Mu(y, c2)) => {
            xs.push(x.clone());
            ys.push(y.clone());
            let r = alpha_command(c1, c2, xs, ys);
            xs.pop();
            ys.pop();
            r
        }
        (Term::Tuple(a), Term::Tuple(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| alpha_term(a, b, xs, ys))
        }
        (Term::Tag(l1, t1), Term::Tag(l2, t2)) => l1 == l2 && alpha_term(t1, t2, xs, ys),
        (
            Term::CoMatch { owner: owner1, branches: b1 },
            Term::CoMatch { owner: owner2, branches: b2 },
        ) => {
            owner1 == owner2
                && b1.len() == b2.len()
                && b1.iter().zip(b2).all(|(l, r)| {
                    if l.label != r.label {
                        return false;
                    }
                    xs.push(l.binder.clone());
                    ys.push(r.binder.clone());
                    let eq = alpha_command(&l.body, &r.body, xs, ys);
                    xs.pop();
                    ys.pop();
                    eq
                })
        }
        (Term::Co(e1), Term::Co(e2)) => alpha_coterm(e1, e2, xs, ys),
        _ => false,
    }
}

fn alpha_coterm(a: &CoTerm, b: &CoTerm, xs: &mut Vec<String>, ys: &mut Vec<String>) -> bool {
    match (a, b) {
        (CoTerm::Covar(x), CoTerm::Covar(y)) => var_eq(x, y, xs, ys),
        (CoTerm::App(v1, e1), CoTerm::App(v2, e2)) => {
            alpha_term(v1, v2, xs, ys) && alpha_coterm(e1, e2, xs, ys)
        }
        (CoTerm::MuTilde(x, c1), CoTerm::MuTilde(y, c2)) => {
            xs.push(x.clone());
            ys.push(y.clone());
            let r = alpha_command(c1, c2, xs, ys);
            xs.pop();
            ys.pop();
            r
        }
        (CoTerm::MuTildeTensor(b1, c1), CoTerm::MuTildeTensor(b2, c2)) => {
            if b1.len() != b2.len() {
                return false;
            }
            xs.extend(b1.iter().cloned());
            ys.extend(b2.iter().cloned());
            let eq = alpha_command(c1, c2, xs, ys);
            xs.truncate(xs.len() - b1.len());
            ys.truncate(ys.len() - b2.len());
            eq
        }
        (CoTerm::Prj(i), CoTerm::Prj(j)) => i == j,
        (CoTerm::Dtor(l1, e1), CoTerm::Dtor(l2, e2)) => l1 == l2 && alpha_coterm(e1, e2, xs, ys),
        (
            CoTerm::CoCase { owner: owner1, branches: b1 },
            CoTerm::CoCase { owner: owner2, branches: b2 },
        ) => {
            owner1 == owner2
                && b1.len() == b2.len()
                && b1.iter().zip(b2).all(|(l, r)| {
                    if l.label != r.label || l.binders.len() != r.binders.len() {
                        return false;
                    }
                    xs.extend(l.binders.iter().cloned());
                    ys.extend(r.binders.iter().cloned());
                    let eq = alpha_command(&l.body, &r.body, xs, ys);
                    xs.truncate(xs.len() - l.binders.len());
                    ys.truncate(ys.len() - r.binders.len());
                    eq
                })
        }
        _ => false,
    }
}

fn alpha_command(a: &Command, b: &Command, xs: &mut Vec<String>, ys: &mut Vec<String>) -> bool {
    match (a, b) {
        (Command::Cut(t1, e1), Command::Cut(t2, e2)) => {
            alpha_term(t1, t2, xs, ys) && alpha_coterm(e1, e2, xs, ys)
        }
    }
}

/// Every name in `term`, bound or free, so a fresh binder cannot collide.
fn occupy_term(term: &Term, used: &mut HashSet<String>) {
    match term {
        Term::Var(y) => {
            used.insert(y.clone());
        }
        Term::Lam(y, body) => {
            used.insert(y.clone());
            occupy_term(body, used);
        }
        Term::Mu(y, body) => {
            used.insert(y.clone());
            occupy_command(body, used);
        }
        Term::Tuple(items) => {
            for item in items {
                occupy_term(item, used);
            }
        }
        Term::Tag(_, t) => occupy_term(t, used),
        Term::CoMatch { branches, .. } => {
            for branch in branches {
                used.insert(branch.binder.clone());
                occupy_command(&branch.body, used);
            }
        }
        Term::Co(e) => occupy_coterm(e, used),
    }
}

fn occupy_coterm(e: &CoTerm, used: &mut HashSet<String>) {
    match e {
        CoTerm::Covar(y) => {
            used.insert(y.clone());
        }
        CoTerm::App(v, tail) => {
            occupy_term(v, used);
            occupy_coterm(tail, used);
        }
        CoTerm::MuTilde(y, c) => {
            used.insert(y.clone());
            occupy_command(c, used);
        }
        CoTerm::Prj(_) => {}
        CoTerm::CoCase { branches, .. } => {
            for branch in branches {
                used.extend(branch.binders.iter().cloned());
                occupy_command(&branch.body, used);
            }
        }
        CoTerm::MuTildeTensor(binders, c) => {
            used.extend(binders.iter().cloned());
            occupy_command(c, used);
        }
        CoTerm::Dtor(_, e) => occupy_coterm(e, used),
    }
}

fn occupy_command(c: &Command, used: &mut HashSet<String>) {
    match c {
        Command::Cut(t, e) => {
            occupy_term(t, used);
            occupy_coterm(e, used);
        }
    }
}

/// Rename free occurrences of `old` to `new`. `new` must not occur in `body`.
fn rename_term(term: &Term, old: &str, new: &str) -> Term {
    match term {
        Term::Var(y) if y == old => Term::Var(new.to_string()),
        Term::Var(y) => Term::Var(y.clone()),
        Term::Lam(y, _) if y == old => term.clone(),
        Term::Lam(y, body) => Term::Lam(y.clone(), Box::new(rename_term(body, old, new))),
        Term::Mu(y, _) if y == old => term.clone(),
        Term::Mu(y, body) => Term::Mu(y.clone(), Box::new(rename_command(body, old, new))),
        Term::Tuple(items) => {
            Term::Tuple(items.iter().map(|item| rename_term(item, old, new)).collect())
        }
        Term::Tag(label, t) => Term::Tag(label.clone(), Box::new(rename_term(t, old, new))),
        Term::CoMatch { owner, branches } => Term::CoMatch {
            owner: owner.clone(),
            branches: branches
                .iter()
                .map(|branch| {
                    if branch.binder == old {
                        branch.clone()
                    } else {
                        CoMatchBranch {
                            label: branch.label.clone(),
                            binder: branch.binder.clone(),
                            body: Box::new(rename_command(&branch.body, old, new)),
                        }
                    }
                })
                .collect(),
        },
        Term::Co(e) => Term::Co(Box::new(rename_coterm(e, old, new))),
    }
}

fn rename_coterm(e: &CoTerm, old: &str, new: &str) -> CoTerm {
    match e {
        CoTerm::Covar(y) if y == old => CoTerm::Covar(new.to_string()),
        CoTerm::Covar(y) => CoTerm::Covar(y.clone()),
        CoTerm::App(v, tail) => {
            CoTerm::App(rename_term(v, old, new), Box::new(rename_coterm(tail, old, new)))
        }
        CoTerm::MuTilde(y, _) if y == old => e.clone(),
        CoTerm::MuTilde(y, c) => CoTerm::MuTilde(y.clone(), Box::new(rename_command(c, old, new))),
        CoTerm::Prj(_) => e.clone(),
        CoTerm::CoCase { owner, branches } => CoTerm::CoCase {
            owner: owner.clone(),
            branches: branches
                .iter()
                .map(|branch| {
                    if branch.binders.iter().any(|binder| binder == old) {
                        branch.clone()
                    } else {
                        CoCaseBranch {
                            label: branch.label.clone(),
                            binders: branch.binders.clone(),
                            body: Box::new(rename_command(&branch.body, old, new)),
                        }
                    }
                })
                .collect(),
        },
        CoTerm::MuTildeTensor(binders, _) if binders.iter().any(|binder| binder == old) => {
            e.clone()
        }
        CoTerm::MuTildeTensor(binders, c) => {
            CoTerm::MuTildeTensor(binders.clone(), Box::new(rename_command(c, old, new)))
        }
        CoTerm::Dtor(label, e) => CoTerm::Dtor(label.clone(), Box::new(rename_coterm(e, old, new))),
    }
}

fn rename_command(command: &Command, old: &str, new: &str) -> Command {
    match command {
        Command::Cut(t, e) => Command::Cut(rename_term(t, old, new), rename_coterm(e, old, new)),
    }
}

/// What is being substituted, and the free names that must not be captured.
struct Subst<'a> {
    name: &'a str,
    free: &'a HashSet<String>,
    replacement: Rep<'a>,
}

enum Rep<'a> {
    Term(&'a Term),
    CoTerm(&'a CoTerm),
}

impl<'a> Subst<'a> {
    fn term(&self, term: &Term) -> Term {
        match term {
            Term::Var(y) => match self.replacement {
                Rep::Term(replacement) if y == self.name => replacement.clone(),
                _ => Term::Var(y.clone()),
            },
            Term::Lam(y, body) => self.under_term(y, body, term, Term::Lam),
            Term::Mu(y, body) => self.under_command(y, body, term, Term::Mu),
            Term::Tuple(items) => Term::Tuple(items.iter().map(|item| self.term(item)).collect()),
            Term::Tag(label, t) => Term::Tag(label.clone(), Box::new(self.term(t))),
            Term::CoMatch { owner, branches } => Term::CoMatch {
                owner: owner.clone(),
                branches: branches
                    .iter()
                    .map(|branch| {
                        if branch.binder == self.name {
                            return branch.clone();
                        }
                        let (binder, body) = escape(
                            self.name,
                            self.free,
                            &branch.binder,
                            branch.body.as_ref(),
                            occupy_command,
                            |body, new_name| rename_command(body, &branch.binder, new_name),
                        );
                        CoMatchBranch {
                            label: branch.label.clone(),
                            binder,
                            body: Box::new(self.command(&body)),
                        }
                    })
                    .collect(),
            },
            Term::Co(e) => Term::Co(Box::new(self.coterm(e))),
        }
    }

    fn command(&self, command: &Command) -> Command {
        match command {
            Command::Cut(t, e) => Command::Cut(self.term(t), self.coterm(e)),
        }
    }

    fn coterm(&self, e: &CoTerm) -> CoTerm {
        match e {
            CoTerm::Covar(y) => match self.replacement {
                Rep::CoTerm(replacement) if y == self.name => replacement.clone(),
                _ => CoTerm::Covar(y.clone()),
            },
            CoTerm::App(v, tail) => CoTerm::App(self.term(v), Box::new(self.coterm(tail))),
            CoTerm::MuTilde(y, body) => self.under_coterm_command(y, body, e, CoTerm::MuTilde),
            CoTerm::Prj(_) => e.clone(),
            CoTerm::CoCase { owner, branches } => CoTerm::CoCase {
                owner: owner.clone(),
                branches: branches.iter().map(|branch| self.branch(branch)).collect(),
            },
            CoTerm::MuTildeTensor(binders, body) => {
                if binders.iter().any(|binder| binder == self.name) {
                    return e.clone();
                }
                let (binders, body) = refresh_binders(self.name, self.free, binders, body);
                CoTerm::MuTildeTensor(binders, Box::new(self.command(&body)))
            }
            CoTerm::Dtor(label, tail) => CoTerm::Dtor(label.clone(), Box::new(self.coterm(tail))),
        }
    }

    /// A term binder whose body is a term: `λ`.
    fn under_term(
        &self,
        binder: &str,
        body: &Term,
        original: &Term,
        build: fn(String, Box<Term>) -> Term,
    ) -> Term {
        if binder == self.name {
            return original.clone();
        }
        let (binder, body) =
            escape(self.name, self.free, binder, body, occupy_term, |body, new| {
                rename_term(body, binder, new)
            });
        build(binder, Box::new(self.term(&body)))
    }

    /// A binder whose body is a command, building a term: `μ`.
    fn under_command(
        &self,
        binder: &str,
        body: &Command,
        original: &Term,
        build: fn(String, Box<Command>) -> Term,
    ) -> Term {
        if binder == self.name {
            return original.clone();
        }
        let (binder, body) =
            escape(self.name, self.free, binder, body, occupy_command, |body, new| {
                rename_command(body, binder, new)
            });
        build(binder, Box::new(self.command(&body)))
    }

    /// A binder whose body is a command, building a co-term: `μ̃`.
    fn under_coterm_command(
        &self,
        binder: &str,
        body: &Command,
        original: &CoTerm,
        build: fn(String, Box<Command>) -> CoTerm,
    ) -> CoTerm {
        if binder == self.name {
            return original.clone();
        }
        let (binder, body) =
            escape(self.name, self.free, binder, body, occupy_command, |body, new| {
                rename_command(body, binder, new)
            });
        build(binder, Box::new(self.command(&body)))
    }

    fn branch(&self, branch: &CoCaseBranch) -> CoCaseBranch {
        if branch.binders.iter().any(|binder| binder == self.name) {
            return branch.clone();
        }
        let (binders, body) = refresh_binders(self.name, self.free, &branch.binders, &branch.body);
        CoCaseBranch { label: branch.label.clone(), binders, body: Box::new(self.command(&body)) }
    }
}

/// Rename `binder` when it occurs free in the replacement. Term binders and
/// covariable binders share a namespace, so either kind captures either kind
/// of free name. `new` is chosen not to occur in the body or the replacement.
fn escape<'a, T: Clone>(
    subst_name: &str,
    free: &HashSet<String>,
    binder: &str,
    body: &'a T,
    occupy: impl FnOnce(&T, &mut HashSet<String>),
    rename: impl FnOnce(&T, &str) -> T,
) -> (String, Cow<'a, T>) {
    if !free.contains(binder) {
        return (binder.to_string(), Cow::Borrowed(body));
    }
    let mut used = free.clone();
    used.insert(subst_name.to_string());
    occupy(body, &mut used);
    let new_name = fresh(binder, &mut used);
    let renamed = rename(body, &new_name);
    (new_name, Cow::Owned(renamed))
}

/// Same as [`escape`], for a binder list. The innermost duplicate is the one
/// that binds, so only that occurrence is renamed.
fn refresh_binders<'a>(
    subst_name: &str,
    free: &HashSet<String>,
    binders: &'a [String],
    body: &'a Command,
) -> (Vec<String>, Cow<'a, Command>) {
    if !binders.iter().any(|binder| free.contains(binder)) {
        return (binders.to_vec(), Cow::Borrowed(body));
    }
    let mut used = free.clone();
    used.insert(subst_name.to_string());
    occupy_command(body, &mut used);
    for binder in binders {
        used.insert(binder.clone());
    }
    let mut body = body.clone();
    let mut seen = HashSet::new();
    let mut names = binders.to_vec();
    for index in (0..names.len()).rev() {
        let binder = names[index].clone();
        if !seen.insert(binder.clone()) || !free.contains(&binder) {
            continue;
        }
        let new_name = fresh(&binder, &mut used);
        body = rename_command(&body, &binder, &new_name);
        names[index] = new_name;
    }
    (names, Cow::Owned(body))
}

/// Substitute `replacement` for variable `x` in `term`, avoiding capture.
pub fn subst_term(x: &str, replacement: &Term, term: &Term) -> Term {
    let free = free_vars_term(replacement);
    Subst { name: x, free: &free, replacement: Rep::Term(replacement) }.term(term)
}

fn subst_with(
    name: &str,
    free: &HashSet<String>,
    replacement: Rep<'_>,
    command: &Command,
) -> Command {
    Subst { name, free, replacement }.command(command)
}

/// Substitute `replacement` for variable `x` in a command.
pub fn subst_command(x: &str, replacement: &Term, command: &Command) -> Command {
    let free = free_vars_term(replacement);
    subst_with(x, &free, Rep::Term(replacement), command)
}

/// Substitute `replacement` for variable `x` in a co-term.
pub fn subst_coterm(x: &str, replacement: &Term, e: &CoTerm) -> CoTerm {
    let free = free_vars_term(replacement);
    Subst { name: x, free: &free, replacement: Rep::Term(replacement) }.coterm(e)
}

/// Substitute co-term `replacement` for co-variable `a` in a command.
///
/// The mirror of [`subst_command`]: values and continuations share one
/// namespace, so any binder of the same name shadows `a` and stops the
/// descent. A term binder also captures a free covariable of that name,
/// and the reverse.
pub fn subst_covar_command(a: &str, replacement: &CoTerm, command: &Command) -> Command {
    let free = free_vars_coterm(replacement);
    subst_with(a, &free, Rep::CoTerm(replacement), command)
}

/// Substitute co-term `replacement` for co-variable `a` in a term.
pub fn subst_covar_term(a: &str, replacement: &CoTerm, term: &Term) -> Term {
    let free = free_vars_coterm(replacement);
    Subst { name: a, free: &free, replacement: Rep::CoTerm(replacement) }.term(term)
}

/// Substitute co-term `replacement` for co-variable `a` in a co-term.
pub fn subst_covar_coterm(a: &str, replacement: &CoTerm, e: &CoTerm) -> CoTerm {
    let free = free_vars_coterm(replacement);
    Subst { name: a, free: &free, replacement: Rep::CoTerm(replacement) }.coterm(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_avoids_used() {
        let mut used = HashSet::new();
        used.insert("x".to_string());
        let f = fresh("x", &mut used);
        assert_ne!(f, "x");
        assert!(used.contains(&f));
    }

    #[test]
    fn alpha_eq_simple() {
        let a = Term::Lam("x".into(), Box::new(Term::Var("x".into())));
        let b = Term::Lam("y".into(), Box::new(Term::Var("y".into())));
        assert!(alpha_eq_term(&a, &b));
    }

    #[test]
    fn alpha_neq_different_body() {
        let a = Term::Lam("x".into(), Box::new(Term::Var("x".into())));
        let b = Term::Lam("y".into(), Box::new(Term::Var("z".into())));
        assert!(!alpha_eq_term(&a, &b));
    }

    #[test]
    fn subst_avoids_binder() {
        // (λx. x) with y := z  =>  λx. x   (x is bound, untouched)
        let body = Term::Lam("x".into(), Box::new(Term::Var("x".into())));
        let sub = Term::Var("z".into());
        let r = subst_term("y", &sub, &body);
        assert!(alpha_eq_term(&r, &body));
    }

    #[test]
    fn subst_replaces_free() {
        // (λx. y) with y := z  =>  λx. z
        let body = Term::Lam("x".into(), Box::new(Term::Var("y".into())));
        let sub = Term::Var("z".into());
        let expected = Term::Lam("x".into(), Box::new(Term::Var("z".into())));
        let r = subst_term("y", &sub, &body);
        assert!(alpha_eq_term(&r, &expected));
    }

    #[test]
    fn subst_renames_a_binder_that_would_capture() {
        // λy. x  with x := y  =>  λy1. y, not λy. y
        let body = Term::Lam("y".into(), Box::new(Term::Var("x".into())));
        let got = subst_term("x", &Term::Var("y".into()), &body);
        let Term::Lam(binder, inner) = got else { panic!("expected a lambda") };
        assert_ne!(binder, "y");
        assert_eq!(*inner, Term::Var("y".into()));
    }

    #[test]
    fn a_term_binder_does_not_capture_a_free_covariable() {
        // λy. μk. ⟨z ∥ a⟩ with a := y  => the binder is not y, and y stays free
        let body = Term::Lam(
            "y".into(),
            Box::new(Term::Mu(
                "k".into(),
                Box::new(Command::Cut(Term::Var("z".into()), CoTerm::Covar("a".into()))),
            )),
        );
        let got = subst_covar_term("a", &CoTerm::Covar("y".into()), &body);
        let Term::Lam(binder, inner) = got else { panic!("expected a lambda") };
        assert_ne!(binder, "y");
        let Term::Mu(_, command) = *inner else { panic!("expected μ") };
        let Command::Cut(_, CoTerm::Covar(name)) = *command else { panic!("expected a covar") };
        assert_eq!(name, "y");
    }

    #[test]
    fn alpha_uses_the_innermost_binder() {
        let shadowed =
            Term::Lam("x".into(), Box::new(Term::Lam("x".into(), Box::new(Term::Var("x".into())))));
        let inner =
            Term::Lam("a".into(), Box::new(Term::Lam("b".into(), Box::new(Term::Var("b".into())))));
        let outer =
            Term::Lam("a".into(), Box::new(Term::Lam("b".into(), Box::new(Term::Var("a".into())))));
        assert!(alpha_eq_term(&shadowed, &inner));
        assert!(!alpha_eq_term(&shadowed, &outer));
    }
}
