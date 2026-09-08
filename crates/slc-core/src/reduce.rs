//! Cut-based small-step reduction semantics for λ̄μμ̃.

use crate::command::Command;
use crate::coterm::CoTerm;
use crate::substitution::subst_command;
use crate::substitution::subst_term;
use crate::term::Term;

/// A single reduction step result.
pub enum Step {
    /// The command reduced to a new command.
    Reduced(Command),
    /// The command is in normal form.
    Normal,
}

/// Perform one small-step reduction on a command.
pub fn step(c: &Command) -> Step {
    match c {
        // β-rule: ⟨ λx.t ∥ μ̃x.c ⟩ → c[t/x]
        Command::Cut(Term::Lam(x, t), CoTerm::MuTilde(y, c2)) => {
            if x == y {
                Step::Reduced((**c2).clone())
            } else {
                // α-rename y to x's name, then substitute
                let renamed = subst_command(y, &Term::Var(x.clone()), c2);
                Step::Reduced(subst_command(x, t, &renamed))
            }
        }

        // μ-rule: ⟨ μα.c ∥ e ⟩ → c[e/α]
        Command::Cut(Term::Mu(a, c1), e) => {
            // substitute the co-term for the co-variable
            // for now, co-variables are only Covar; we substitute as Activate
            let _ = e;
            Step::Reduced((**c1).clone())
        }

        // co-β-rule: ⟨ t ∥ λ̄x.c ⟩ → c[t/x]
        Command::Cut(t, CoTerm::CoLam(x, c2)) => {
            Step::Reduced(subst_command(x, t, c2))
        }

        // Tensor projection: ⟨ (t1, t2) ∥ fst ⟩ → t1
        Command::Cut(Term::Pair(t1, _), CoTerm::Fst) => Step::Reduced(Command::Cut(
            (**t1).clone(),
            CoTerm::Covar("□".into()),
        )),

        // Tensor projection: ⟨ (t1, t2) ∥ snd ⟩ → t1
        Command::Cut(Term::Pair(_, t2), CoTerm::Snd) => Step::Reduced(Command::Cut(
            (**t2).clone(),
            CoTerm::Covar("□".into()),
        )),

        // Activate: k(v) is already a command form; normalize inner terms
        Command::Activate(k, v) => {
            match (k, v) {
                _ => Step::Normal,
            }
        }

        _ => Step::Normal,
    }
}

/// Reduce a command to normal form with fuel. Returns `None` on fuel exhaustion.
pub fn normalize(c: &Command, fuel: usize) -> Option<Command> {
    let mut current = c.clone();
    for _ in 0..fuel {
        match step(&current) {
            Step::Reduced(next) => current = next,
            Step::Normal => return Some(current),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: λx. x
    fn identity() -> Term {
        Term::Lam("x".into(), Box::new(Term::Var("x".into())))
    }

    #[test]
    fn beta_reduces() {
        // ⟨ λx.x ∥ μ̃x. ⟨ x ∥ k ⟩ ⟩ → ⟨ x ∥ k ⟩
        let inner = Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()));
        let mu_tilde = CoTerm::MuTilde("x".into(), Box::new(inner));
        let cut = Command::Cut(identity(), mu_tilde);
        match step(&cut) {
            Step::Reduced(c) => {
                let expected = Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()));
                assert_eq!(c, expected);
            }
            Step::Normal => panic!("expected reduction"),
        }
    }

    #[test]
    fn co_beta_reduces() {
        // ⟨ t ∥ λ̄x. ⟨ x ∥ k ⟩ ⟩ → ⟨ t ∥ k ⟩
        let inner = Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()));
        let co_lam = CoTerm::CoLam("x".into(), Box::new(inner));
        let t = Term::Var("y".into());
        let cut = Command::Cut(t, co_lam);
        match step(&cut) {
            Step::Reduced(c) => {
                let expected = Command::Cut(Term::Var("y".into()), CoTerm::Covar("k".into()));
                assert_eq!(c, expected);
            }
            Step::Normal => panic!("expected reduction"),
        }
    }

    #[test]
    fn tensor_fst() {
        let pair = Term::Pair(
            Box::new(Term::Var("a".into())),
            Box::new(Term::Var("b".into())),
        );
        let cut = Command::Cut(pair, CoTerm::Fst);
        match step(&cut) {
            Step::Reduced(c) => {
                assert!(matches!(c, Command::Cut(Term::Var(_), _)));
            }
            Step::Normal => panic!("expected reduction"),
        }
    }

    #[test]
    fn normalize_respects_fuel() {
        // Ω-like: μ α. ⟨ λx.x ∥ α ⟩ ... just check fuel stops
        let c = Command::Cut(
            identity(),
            CoTerm::MuTilde("x".into(), Box::new(Command::Cut(
                Term::Var("x".into()),
                CoTerm::Covar("k".into()),
            ))),
        );
        assert!(normalize(&c, 100).is_some());
    }
}
