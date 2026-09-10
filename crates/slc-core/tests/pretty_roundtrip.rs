//! Pretty-printing is the core's external syntax: everything it writes must
//! read back as the same IR.

use slc_core::command::Command;
use slc_core::coterm::{CoCaseBranch, CoTerm};
use slc_core::parse::{parse_command, parse_coterm, parse_term, parse_type};
use slc_core::term::Term;
use slc_core::types::{Base, Type};

fn var(x: &str) -> Term {
    Term::Var(x.into())
}

fn cut(t: Term, e: CoTerm) -> Command {
    Command::Cut(t, e)
}

fn every_term() -> Vec<Term> {
    vec![
        var("x"),
        var("$int_42"),
        var("$str_\"a b\""),
        Term::Lam("x".into(), Box::new(var("x"))),
        Term::Mu("k".into(), Box::new(cut(var("v"), CoTerm::Covar("k".into())))),
        Term::CoAbs("k".into(), Box::new(var("v"))),
        Term::Pair(Box::new(var("a")), Box::new(var("b"))),
        Term::Inl(Box::new(var("a"))),
        Term::Inr(Box::new(var("b"))),
        Term::Tag("Color::Red".into(), Box::new(var("$unit"))),
        Term::Co(Box::new(CoTerm::CoCase(vec![
            CoCaseBranch {
                label: "Color::Red".into(),
                binders: vec!["x".into()],
                body: Box::new(cut(var("$int_0"), CoTerm::Covar("return".into()))),
            },
            CoCaseBranch {
                label: "Color::Green".into(),
                binders: vec!["y".into()],
                body: Box::new(cut(var("y"), CoTerm::Covar("return".into()))),
            },
        ]))),
        // A declaration-shaped term: binders of both kinds, nested.
        Term::Lam(
            "x".into(),
            Box::new(Term::CoAbs(
                "k".into(),
                Box::new(Term::Mu(
                    "__call".into(),
                    Box::new(cut(
                        var("k"),
                        CoTerm::CoLam(
                            "__f".into(),
                            Box::new(cut(var("x"), CoTerm::Covar("__call".into()))),
                        ),
                    )),
                )),
            )),
        ),
    ]
}

fn every_coterm() -> Vec<CoTerm> {
    vec![
        CoTerm::Covar("k".into()),
        CoTerm::CoLam("x".into(), Box::new(cut(var("x"), CoTerm::Covar("k".into())))),
        CoTerm::MuTilde("x".into(), Box::new(cut(var("x"), CoTerm::Covar("k".into())))),
        CoTerm::Par(Box::new(CoTerm::Covar("a".into())), Box::new(CoTerm::Covar("b".into()))),
        CoTerm::Prj(0),
        CoTerm::Prj(1),
        CoTerm::CoCase(vec![CoCaseBranch {
            label: "Reading::Measured".into(),
            binders: vec!["value".into()],
            body: Box::new(cut(var("value"), CoTerm::Covar("k".into()))),
        }]),
    ]
}

fn every_command() -> Vec<Command> {
    vec![
        cut(var("v"), CoTerm::Covar("k".into())),
        cut(
            Term::Lam("x".into(), Box::new(var("x"))),
            CoTerm::MuTilde("y".into(), Box::new(cut(var("y"), CoTerm::Covar("k".into())))),
        ),
        Command::Command("x".into(), var("t")),
        Command::Activate(var("k"), var("v")),
    ]
}

fn every_type() -> Vec<Type> {
    vec![
        Type::Var(3),
        Type::Pos(Base::I32),
        Type::Neg(Base::I32),
        Type::Pos(Base::Str),
        Type::Neg(Base::Char),
        Type::One,
        Type::Bottom,
        Type::Tensor(Box::new(Type::Pos(Base::I32)), Box::new(Type::Pos(Base::Bool))),
        Type::Par(Box::new(Type::Neg(Base::I32)), Box::new(Type::Neg(Base::Bool))),
        Type::With(Box::new(Type::Neg(Base::I32)), Box::new(Type::Neg(Base::Bool))),
        Type::Sum(Box::new(Type::Pos(Base::I32)), Box::new(Type::Pos(Base::Bool))),
        Type::Bang(Box::new(Type::Pos(Base::I32))),
        Type::List(Box::new(Type::Pos(Base::I32))),
        Type::arrow(Type::Pos(Base::I32), Type::Neg(Base::Bool)),
        Type::Dual(Box::new(Type::Named("Color".into()))),
        Type::Named("Color".into()),
    ]
}

#[test]
fn every_term_round_trips() {
    for term in every_term() {
        let printed = term.to_string();
        assert_eq!(parse_term(&printed), Ok(term.clone()), "printed: {printed}");
    }
}

#[test]
fn every_coterm_round_trips() {
    for coterm in every_coterm() {
        let printed = coterm.to_string();
        assert_eq!(parse_coterm(&printed), Ok(coterm.clone()), "printed: {printed}");
    }
}

#[test]
fn every_command_round_trips() {
    for command in every_command() {
        let printed = command.to_string();
        assert_eq!(parse_command(&printed), Ok(command.clone()), "printed: {printed}");
    }
}

#[test]
fn every_type_round_trips() {
    for ty in every_type() {
        let printed = ty.to_string();
        assert_eq!(parse_type(&printed), Ok(ty.clone()), "printed: {printed}");
        // Polarity annotations survive the round trip in both directions.
        let dual = ty.dual();
        assert_eq!(parse_type(&dual.to_string()), Ok(dual), "printed: {printed}");
    }
}
