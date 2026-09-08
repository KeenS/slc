//! Inline snapshot tests for diagnostics and pretty-printed IR.
//!
//! `insta` is unavailable in the offline build environment, so snapshots are
//! represented as explicit expected strings. Like insta snapshots, a change
//! must be reviewed and copied into the expected value.

use slc_core::command::Command;
use slc_core::coterm::CoTerm;
use slc_core::term::Term;
use slc_core::types::{Base, Type};
use slc_core::typing::{CoTermContext, TermContext, TypeError, infer_command};

fn pos_i32() -> Type {
    Type::Pos(Base::I32)
}

#[test]
fn snapshot_lambda_term() {
    let term = Term::Lam("x".into(), Box::new(Term::Var("x".into())));
    assert_eq!(term.to_string(), "λx. x");
}

#[test]
fn snapshot_cut_command() {
    let command = Command::Cut(
        Term::Var("v".into()),
        CoTerm::CoLam(
            "x".into(),
            Box::new(Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()))),
        ),
    );
    assert_eq!(command.to_string(), "⟨v ∥ λ̄x. ⟨x ∥ k⟩⟩");
}

#[test]
fn snapshot_mu_term() {
    let term = Term::Mu(
        "k".into(),
        Box::new(Command::Cut(Term::Var("v".into()), CoTerm::Covar("k".into()))),
    );
    assert_eq!(term.to_string(), "μk. ⟨v ∥ k⟩");
}

#[test]
fn snapshot_type_mismatch_diagnostic() {
    let mut gamma = TermContext::new();
    let mut delta = CoTermContext::new();
    gamma.insert("x".into(), pos_i32());
    delta.insert("k".into(), Type::Neg(Base::Bool));
    let command = Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()));
    let error = infer_command(&command, &mut gamma, &mut delta).unwrap_err();
    assert_eq!(
        error,
        TypeError::Mismatch { expected: Type::Neg(Base::I32), actual: Type::Neg(Base::Bool) }
    );
    assert_eq!(error.to_string(), "type mismatch: expected -i32, got -bool");
}

#[test]
fn snapshot_unbound_diagnostic() {
    let mut gamma = TermContext::new();
    let mut delta = CoTermContext::new();
    let error = infer_command(
        &Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into())),
        &mut gamma,
        &mut delta,
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "unbound variable: x");
}
