//! α-equivalence, fresh variable generation, and capture-avoiding substitution.

use crate::command::Command;
use crate::coterm::{CoCaseBranch, CoTerm};
use crate::term::{CoMatchBranch, Term};
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
        Term::Pair(t1, t2) => {
            go_term(t1, out);
            go_term(t2, out);
        }
        Term::Tag(_, t) => go_term(t, out),
        Term::CoMatch(branches) => {
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
        CoTerm::CoCase(branches) => {
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
    match (xs.iter().position(|p| p == x), ys.iter().position(|p| p == y)) {
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
        (Term::Pair(a1, a2), Term::Pair(b1, b2)) => {
            alpha_term(a1, b1, xs, ys) && alpha_term(a2, b2, xs, ys)
        }
        (Term::Tag(l1, t1), Term::Tag(l2, t2)) => l1 == l2 && alpha_term(t1, t2, xs, ys),
        (Term::CoMatch(b1), Term::CoMatch(b2)) => {
            b1.len() == b2.len()
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
        (CoTerm::CoCase(b1), CoTerm::CoCase(b2)) => {
            b1.len() == b2.len()
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

/// Substitute `replacement` for variable `x` in `term`, avoiding capture.
pub fn subst_term(x: &str, replacement: &Term, term: &Term) -> Term {
    match term {
        Term::Var(y) if y == x => replacement.clone(),
        Term::Var(y) => Term::Var(y.clone()),
        Term::Lam(y, body) => {
            if y == x {
                term.clone()
            } else {
                Term::Lam(y.clone(), Box::new(subst_term(x, replacement, body)))
            }
        }
        Term::Mu(a, c) => {
            if a == x {
                term.clone()
            } else {
                Term::Mu(a.clone(), Box::new(subst_command(x, replacement, c)))
            }
        }
        Term::Pair(t1, t2) => Term::Pair(
            Box::new(subst_term(x, replacement, t1)),
            Box::new(subst_term(x, replacement, t2)),
        ),
        Term::Tag(label, t) => Term::Tag(label.clone(), Box::new(subst_term(x, replacement, t))),
        Term::CoMatch(branches) => Term::CoMatch(
            branches
                .iter()
                .map(|branch| {
                    if branch.binder == x {
                        branch.clone()
                    } else {
                        CoMatchBranch {
                            label: branch.label.clone(),
                            binder: branch.binder.clone(),
                            body: Box::new(subst_command(x, replacement, &branch.body)),
                        }
                    }
                })
                .collect(),
        ),
        Term::Co(e) => Term::Co(Box::new(subst_coterm(x, replacement, e))),
    }
}

/// Substitute `replacement` for variable `x` in a command.
pub fn subst_command(x: &str, replacement: &Term, command: &Command) -> Command {
    match command {
        Command::Cut(t, e) => {
            Command::Cut(subst_term(x, replacement, t), subst_coterm(x, replacement, e))
        }
    }
}

/// Substitute `replacement` for variable `x` in a co-term.
pub fn subst_coterm(x: &str, replacement: &Term, e: &CoTerm) -> CoTerm {
    match e {
        CoTerm::Covar(y) => {
            if y == x {
                CoTerm::Covar(y.clone())
            } else {
                e.clone()
            }
        }
        CoTerm::App(v, tail) => {
            CoTerm::App(subst_term(x, replacement, v), Box::new(subst_coterm(x, replacement, tail)))
        }
        CoTerm::MuTilde(y, c) => {
            if y == x {
                e.clone()
            } else {
                CoTerm::MuTilde(y.clone(), Box::new(subst_command(x, replacement, c)))
            }
        }
        CoTerm::Prj(_) => e.clone(),
        CoTerm::CoCase(branches) => CoTerm::CoCase(
            branches
                .iter()
                .map(|branch| {
                    if branch.binders.iter().any(|binder| binder == x) {
                        branch.clone()
                    } else {
                        CoCaseBranch {
                            label: branch.label.clone(),
                            binders: branch.binders.clone(),
                            body: Box::new(subst_command(x, replacement, &branch.body)),
                        }
                    }
                })
                .collect(),
        ),
        CoTerm::MuTildeTensor(binders, c) => {
            if binders.iter().any(|binder| binder == x) {
                e.clone()
            } else {
                CoTerm::MuTildeTensor(binders.clone(), Box::new(subst_command(x, replacement, c)))
            }
        }
        CoTerm::Dtor(label, e) => {
            CoTerm::Dtor(label.clone(), Box::new(subst_coterm(x, replacement, e)))
        }
    }
}

/// Substitute co-term `replacement` for co-variable `a` in a command.
///
/// The mirror of [`subst_command`]: values and continuations share one
/// namespace, so any binder of the same name shadows `a` and stops the
/// descent.
pub fn subst_covar_command(a: &str, replacement: &CoTerm, command: &Command) -> Command {
    match command {
        Command::Cut(t, e) => {
            Command::Cut(subst_covar_term(a, replacement, t), subst_covar_coterm(a, replacement, e))
        }
    }
}

/// Substitute co-term `replacement` for co-variable `a` in a term.
pub fn subst_covar_term(a: &str, replacement: &CoTerm, term: &Term) -> Term {
    match term {
        Term::Var(_) => term.clone(),
        Term::Lam(y, body) => {
            if y == a {
                term.clone()
            } else {
                Term::Lam(y.clone(), Box::new(subst_covar_term(a, replacement, body)))
            }
        }
        Term::Mu(b, c) => {
            if b == a {
                term.clone()
            } else {
                Term::Mu(b.clone(), Box::new(subst_covar_command(a, replacement, c)))
            }
        }
        Term::Pair(t1, t2) => Term::Pair(
            Box::new(subst_covar_term(a, replacement, t1)),
            Box::new(subst_covar_term(a, replacement, t2)),
        ),
        Term::Tag(label, t) => {
            Term::Tag(label.clone(), Box::new(subst_covar_term(a, replacement, t)))
        }
        Term::CoMatch(branches) => Term::CoMatch(
            branches
                .iter()
                .map(|branch| {
                    if branch.binder == a {
                        branch.clone()
                    } else {
                        CoMatchBranch {
                            label: branch.label.clone(),
                            binder: branch.binder.clone(),
                            body: Box::new(subst_covar_command(a, replacement, &branch.body)),
                        }
                    }
                })
                .collect(),
        ),
        Term::Co(e) => Term::Co(Box::new(subst_covar_coterm(a, replacement, e))),
    }
}

/// Substitute co-term `replacement` for co-variable `a` in a co-term.
pub fn subst_covar_coterm(a: &str, replacement: &CoTerm, e: &CoTerm) -> CoTerm {
    match e {
        CoTerm::Covar(y) => {
            if y == a {
                replacement.clone()
            } else {
                e.clone()
            }
        }
        CoTerm::App(v, tail) => CoTerm::App(
            subst_covar_term(a, replacement, v),
            Box::new(subst_covar_coterm(a, replacement, tail)),
        ),
        CoTerm::MuTilde(y, c) => {
            if y == a {
                e.clone()
            } else {
                CoTerm::MuTilde(y.clone(), Box::new(subst_covar_command(a, replacement, c)))
            }
        }
        CoTerm::Prj(_) => e.clone(),
        CoTerm::CoCase(branches) => CoTerm::CoCase(
            branches
                .iter()
                .map(|branch| {
                    if branch.binders.iter().any(|binder| binder == a) {
                        branch.clone()
                    } else {
                        CoCaseBranch {
                            label: branch.label.clone(),
                            binders: branch.binders.clone(),
                            body: Box::new(subst_covar_command(a, replacement, &branch.body)),
                        }
                    }
                })
                .collect(),
        ),
        CoTerm::MuTildeTensor(binders, c) => {
            if binders.iter().any(|binder| binder == a) {
                e.clone()
            } else {
                CoTerm::MuTildeTensor(
                    binders.clone(),
                    Box::new(subst_covar_command(a, replacement, c)),
                )
            }
        }
        CoTerm::Dtor(label, e) => {
            CoTerm::Dtor(label.clone(), Box::new(subst_covar_coterm(a, replacement, e)))
        }
    }
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
}
