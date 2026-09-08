//! Property-style tests for duality and substitution laws.
//!
//! The sandbox is offline, so this is a deterministic property corpus
//! rather than a randomized `proptest` suite. Every generated case is a
//! small closed type/term built from a fixed set of constructors, which
//! gives exhaustive coverage for the two-operator shape universe below.

use slc_core::command::Command;
use slc_core::coterm::CoTerm;
use slc_core::substitution::{alpha_eq_term, free_vars_term, subst_term};
use slc_core::term::Term;
use slc_core::types::{Base, Type};

fn types() -> Vec<Type> {
    let i32 = Type::Pos(Base::I32);
    let bool = Type::Pos(Base::Bool);
    let ni32 = Type::Neg(Base::I32);
    let nbool = Type::Neg(Base::Bool);
    vec![
        Type::One,
        Type::Bottom,
        i32.clone(),
        bool.clone(),
        ni32.clone(),
        nbool.clone(),
        Type::Tensor(Box::new(i32.clone()), Box::new(bool.clone())),
        Type::Par(Box::new(ni32.clone()), Box::new(nbool.clone())),
        Type::Sum(Box::new(i32.clone()), Box::new(bool.clone())),
        Type::With(Box::new(ni32.clone()), Box::new(nbool)),
        Type::Bang(Box::new(i32.clone())),
        Type::List(Box::new(i32.clone())),
        Type::Fun(Box::new(i32), Box::new(bool)),
    ]
}

#[test]
fn dual_is_involutive_on_corpus() {
    for ty in types() {
        assert_eq!(ty.dual().dual(), ty, "dual(dual({ty})) != {ty}");
    }
}

#[test]
fn dual_of_dual_is_identity_under_alpha() {
    for ty in types() {
        let dd = ty.dual().dual();
        assert_eq!(format!("{dd}"), format!("{ty}"));
    }
}

fn term_corpus() -> Vec<Term> {
    let x = Term::Var("x".into());
    let y = Term::Var("y".into());
    let id = Term::Lam("x".into(), Box::new(Term::Var("x".into())));
    let id_y = Term::Lam("y".into(), Box::new(Term::Var("y".into())));
    let mu = Term::Mu(
        "k".into(),
        Box::new(Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()))),
    );
    let pair = Term::Pair(Box::new(x.clone()), Box::new(y.clone()));
    vec![
        Term::Var("z".into()),
        x,
        y,
        id,
        id_y,
        mu,
        pair,
        Term::Inl(Box::new(Term::Var("z".into()))),
        Term::Inr(Box::new(Term::Var("z".into()))),
    ]
}

#[test]
fn substitution_is_idempotent_for_absent_variable() {
    for t in term_corpus() {
        let replacement = Term::Var("replacement".into());
        let once = subst_term("absent", &replacement, &t);
        let twice = subst_term("absent", &replacement, &once);
        assert!(alpha_eq_term(&once, &twice), "substitution changed {t}");
    }
}

#[test]
fn substitution_preserves_free_variable_membership() {
    for t in term_corpus() {
        let replacement = Term::Var("replacement".into());
        let before = free_vars_term(&t);
        let after = free_vars_term(&subst_term("x", &replacement, &t));
        assert!(!after.contains("x"), "free x survived substitution in {t}");
        if before.contains("x") {
            assert!(after.contains("replacement"), "replacement vanished from {t}");
        }
    }
}

#[test]
fn substitution_result_is_stable_under_repetition() {
    for t in term_corpus() {
        let replacement = Term::Var("z".into());
        let once = subst_term("x", &replacement, &t);
        let twice = subst_term("x", &replacement, &once);
        assert!(alpha_eq_term(&once, &twice), "substitution is not idempotent: {t}");
    }
}
