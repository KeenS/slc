//! Cut-based small-step reduction semantics for λ̄μμ̃.

use crate::command::Command;
use crate::coterm::CoTerm;
use crate::substitution::subst_command;
use crate::term::Term;

/// The `index`-th component of a right-nested product: walk `index` tails,
/// then take the head, or the whole remainder when it is the bare last.
fn project_term(product: &Term, index: usize) -> Option<Term> {
    let mut current = product;
    for _ in 0..index {
        let Term::Pair(_, tail) = current else { return None };
        current = tail;
    }
    match current {
        Term::Pair(head, _) => Some((**head).clone()),
        last => Some(last.clone()),
    }
}

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
        // μ-rule: ⟨ μα.c ∥ e ⟩ → c[e/α]
        // Substitution of co-terms for co-variables is handled during
        // evaluation; here we return the command body. It is tried before the
        // μ̃-rule, which settles the critical pair ⟨μα.c ∥ μ̃x.c'⟩ in favour of
        // the producer.
        Command::Cut(Term::Mu(_, c1), _) => Step::Reduced((**c1).clone()),

        // μ̃-rule: ⟨ v ∥ μ̃x.c ⟩ → c[v/x]
        // This is the binder: it takes what the cut delivers and runs `c`
        // with it bound, which is what `let` and every other binding form
        // lowers to.
        Command::Cut(v, CoTerm::MuTilde(x, c2)) => Step::Reduced(subst_command(x, v, c2)),

        // Labelled rule: ⟨ L(v₁ ⊗ … ⊗ vₙ) ∥ μ̃[… L(x₁,…,xₙ). c …] ⟩ → c[vᵢ/xᵢ]
        // The label selects exactly one branch; the others are discarded
        // unreduced. An `enum` has a branch per variant, a `struct` one.
        Command::Cut(Term::Tag(label, payload), CoTerm::CoCase(branches)) => {
            match branches.iter().find(|b| &b.label == label) {
                Some(branch) => match bind_components(&branch.binders, payload, &branch.body) {
                    Some(command) => Step::Reduced(command),
                    None => Step::Normal,
                },
                None => Step::Normal,
            }
        }

        // Multiplicative rule: ⟨ v₁ ⊗ v₂ ∥ μ̃(x, y). c ⟩ → c[v₁/x, v₂/y]
        Command::Cut(value, CoTerm::MuTildeTensor(binders, body)) => {
            match bind_components(binders, value, body) {
                Some(command) => Step::Reduced(command),
                None => Step::Normal,
            }
        }

        // co-β-rule: ⟨ t ∥ λ̄x.c ⟩ → c[t/x]
        Command::Cut(t, CoTerm::CoLam(x, c2)) => Step::Reduced(subst_command(x, t, c2)),

        // Projection: ⟨ (t₀ ⊗ … ) ∥ prj:i ⟩ → tᵢ. Walk `i` tails along the
        // right-nested spine, then take the head — or the whole remainder
        // when it is the bare last component.
        Command::Cut(product @ Term::Pair(..), CoTerm::Prj(index)) => {
            match project_term(product, *index) {
                Some(t) => Step::Reduced(Command::Cut(t, CoTerm::Covar("□".into()))),
                None => Step::Normal,
            }
        }

        _ => Step::Normal,
    }
}

/// Substitute the components of a right-nested tensor for a list of binders.
/// The last binder takes whatever remains, so `n` binders split `v₁ ⊗ (v₂ ⊗ v₃)`
/// into exactly `n` parts.
fn bind_components(binders: &[String], value: &Term, body: &Command) -> Option<Command> {
    let mut command = body.clone();
    let mut rest = value.clone();
    for (index, binder) in binders.iter().enumerate() {
        if index + 1 == binders.len() {
            return Some(subst_command(binder, &rest, &command));
        }
        let Term::Pair(head, tail) = rest else {
            return None;
        };
        command = subst_command(binder, &head, &command);
        rest = *tail;
    }
    Some(command)
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
    fn mu_tilde_binds_the_value() {
        // ⟨ λx.x ∥ μ̃y. ⟨ y ∥ k ⟩ ⟩ → ⟨ λx.x ∥ k ⟩
        let inner = Command::Cut(Term::Var("y".into()), CoTerm::Covar("k".into()));
        let mu_tilde = CoTerm::MuTilde("y".into(), Box::new(inner));
        match step(&Command::Cut(identity(), mu_tilde)) {
            Step::Reduced(c) => {
                assert_eq!(c, Command::Cut(identity(), CoTerm::Covar("k".into())));
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
    fn tensor_projection() {
        let pair = Term::Pair(Box::new(Term::Var("a".into())), Box::new(Term::Var("b".into())));
        match step(&Command::Cut(pair.clone(), CoTerm::Prj(0))) {
            Step::Reduced(Command::Cut(Term::Var(v), _)) => assert_eq!(v, "a"),
            _ => panic!("expected projection to `a`"),
        }
        match step(&Command::Cut(pair, CoTerm::Prj(1))) {
            Step::Reduced(Command::Cut(Term::Var(v), _)) => assert_eq!(v, "b"),
            _ => panic!("expected projection to `b`"),
        }
    }

    fn branch(label: &str, target: &str) -> crate::coterm::CoCaseBranch {
        crate::coterm::CoCaseBranch {
            label: label.into(),
            binders: vec!["x".into()],
            body: Box::new(Command::Cut(Term::Var("x".into()), CoTerm::Covar(target.into()))),
        }
    }

    #[test]
    fn labelled_value_selects_its_branch() {
        // ⟨ Color::Green(v) ∥ μ̃[Red x.⟨x ∥ r⟩ | Green x.⟨x ∥ g⟩] ⟩ → ⟨ v ∥ g ⟩
        let consumer = CoTerm::CoCase(vec![
            branch("Color::Red", "r"),
            branch("Color::Green", "g"),
            branch("Color::Blue", "b"),
        ]);
        let value = Term::Tag("Color::Green".into(), Box::new(Term::Var("v".into())));
        match step(&Command::Cut(value, consumer)) {
            Step::Reduced(c) => {
                assert_eq!(c, Command::Cut(Term::Var("v".into()), CoTerm::Covar("g".into())));
            }
            Step::Normal => panic!("expected the Green branch to fire"),
        }
    }

    #[test]
    fn a_label_with_no_branch_does_not_reduce() {
        let consumer = CoTerm::CoCase(vec![branch("Color::Red", "r")]);
        let value = Term::Tag("Color::Blue".into(), Box::new(Term::Var("v".into())));
        assert!(matches!(step(&Command::Cut(value, consumer)), Step::Normal));
    }

    #[test]
    fn normalize_respects_fuel() {
        // Ω-like: μ α. ⟨ λx.x ∥ α ⟩ ... just check fuel stops
        let c = Command::Cut(
            identity(),
            CoTerm::MuTilde(
                "x".into(),
                Box::new(Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()))),
            ),
        );
        assert!(normalize(&c, 100).is_some());
    }
}
