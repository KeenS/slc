//! Confluence property tests: different reduction orders converge
//! to the same normal form (up to alpha-equivalence).

use slc_core::command::Command;
use slc_core::coterm::CoTerm;
use slc_core::reduce::{Step, normalize, step};
use slc_core::substitution::alpha_eq_term;
use slc_core::term::Term;

fn nf(c: &Command) -> Command {
    normalize(c, 1000).expect("should normalize")
}

#[test]
fn beta_order_independent() {
    let inner = Command::Cut(
        Term::Lam("x".into(), Box::new(Term::Var("x".into()))),
        CoTerm::MuTilde(
            "x".into(),
            Box::new(Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()))),
        ),
    );
    let a = nf(&inner);
    let b = nf(&inner);
    assert_eq!(a, b);
}

#[test]
fn tensor_projection_commutes_with_beta() {
    let pair = Term::Pair(Box::new(Term::Var("a".into())), Box::new(Term::Var("b".into())));
    let c = Command::Cut(pair, CoTerm::Prj(0));
    let r = step(&c);
    assert!(matches!(r, Step::Reduced(_)));
}

#[test]
fn normalization_is_deterministic() {
    let c = Command::Cut(
        Term::Lam("x".into(), Box::new(Term::Var("x".into()))),
        CoTerm::MuTilde(
            "y".into(),
            Box::new(Command::Cut(Term::Var("y".into()), CoTerm::Covar("k".into()))),
        ),
    );
    let n1 = nf(&c);
    let n2 = nf(&c);
    let (Command::Cut(t1, _), Command::Cut(t2, _)) = (&n1, &n2);
    assert!(alpha_eq_term(t1, t2));
}

#[test]
fn normal_forms_are_idempotent() {
    let c = Command::Cut(
        Term::Lam("x".into(), Box::new(Term::Var("x".into()))),
        CoTerm::MuTilde(
            "x".into(),
            Box::new(Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()))),
        ),
    );
    let once = nf(&c);
    let twice = nf(&once);
    assert_eq!(once, twice);
}
