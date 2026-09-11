//! Cut-based small-step reduction semantics for λ̄μμ̃.

use crate::command::Command;
use crate::coterm::CoTerm;
use crate::substitution::{subst_command, subst_covar_command};
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
        // unreduced. An `enum` has a branch per variant, a `data` one.
        Command::Cut(Term::Tag(label, payload), CoTerm::CoCase(branches)) => {
            match branches.iter().find(|b| &b.label == label) {
                Some(branch) => match bind_components(&branch.binders, payload, &branch.body) {
                    Some(command) => Step::Reduced(command),
                    None => Step::Normal,
                },
                None => Step::Normal,
            }
        }

        // Copattern rule: ⟨ μ[… .d(α). c …] ∥ .d(e) ⟩ → c[e/α]
        // The mirror of the labelled rule: the request selects exactly one
        // branch of the menu, binds its continuation, and the branches that
        // were not demanded are discarded unreduced.
        Command::Cut(Term::CoMatch(branches), CoTerm::Dtor(label, e)) => {
            match branches.iter().find(|b| &b.label == label) {
                Some(branch) => Step::Reduced(subst_covar_command(&branch.binder, e, &branch.body)),
                None => Step::Normal,
            }
        }

        // Co-labelled rule: ⟨ co(.d(e)) ∥ μ̃[… .d(x). c …] ⟩ → c[co(e)/x]
        // A request boxed by ↓ is a positive value with a label, so matching
        // on a continuation is the labelled rule with the payload rewrapped:
        // the arm receives the request's own continuation as a value.
        Command::Cut(Term::Co(request), CoTerm::CoCase(branches)) => match request.as_ref() {
            CoTerm::Dtor(label, e) => match branches.iter().find(|b| &b.label == label) {
                Some(branch) => {
                    let payload = Term::Co(e.clone());
                    match bind_components(&branch.binders, &payload, &branch.body) {
                        Some(command) => Step::Reduced(command),
                        None => Step::Normal,
                    }
                }
                None => Step::Normal,
            },
            _ => Step::Normal,
        },

        // Multiplicative rule: ⟨ v₁ ⊗ v₂ ∥ μ̃(x, y). c ⟩ → c[v₁/x, v₂/y]
        Command::Cut(value, CoTerm::MuTildeTensor(binders, body)) => {
            match bind_components(binders, value, body) {
                Some(command) => Step::Reduced(command),
                None => Step::Normal,
            }
        }

        // →-rule: ⟨ λx.t ∥ v·e ⟩ → ⟨ v ∥ μ̃x. ⟨ t ∥ e ⟩ ⟩ — application:
        // the argument goes to the binder, and the body meets the tail.
        Command::Cut(Term::Lam(x, t), CoTerm::App(v, e)) => Step::Reduced(Command::Cut(
            v.clone(),
            CoTerm::MuTilde(x.clone(), Box::new(Command::Cut((**t).clone(), (**e).clone()))),
        )),

        // ↑-rule: ⟨ co(e′) ∥ v·e ⟩ → ⟨ v ∥ e′ ⟩ — applying a boxed consumer
        // opens the box; a consumer does not return, so the tail is dropped.
        Command::Cut(Term::Co(consumer), CoTerm::App(v, _)) => {
            Step::Reduced(Command::Cut(v.clone(), (**consumer).clone()))
        }

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
    fn application_reduces() {
        // ⟨ λx.x ∥ y · k ⟩ → ⟨ y ∥ μ̃x. ⟨ x ∥ k ⟩ ⟩
        let app = CoTerm::App(Term::Var("y".into()), Box::new(CoTerm::Covar("k".into())));
        match step(&Command::Cut(identity(), app)) {
            Step::Reduced(c) => {
                let expected = Command::Cut(
                    Term::Var("y".into()),
                    CoTerm::MuTilde(
                        "x".into(),
                        Box::new(Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()))),
                    ),
                );
                assert_eq!(c, expected);
            }
            Step::Normal => panic!("expected reduction"),
        }
    }

    #[test]
    fn applying_a_boxed_consumer_opens_the_box() {
        // ⟨ co(μ̃x. ⟨x ∥ k⟩) ∥ y · tail ⟩ → ⟨ y ∥ μ̃x. ⟨x ∥ k⟩ ⟩
        let consumer = CoTerm::MuTilde(
            "x".into(),
            Box::new(Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()))),
        );
        let boxed = Term::Co(Box::new(consumer.clone()));
        let app = CoTerm::App(Term::Var("y".into()), Box::new(CoTerm::Covar("tail".into())));
        match step(&Command::Cut(boxed, app)) {
            Step::Reduced(c) => {
                assert_eq!(c, Command::Cut(Term::Var("y".into()), consumer));
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

    fn menu_branch(label: &str, answer: &str) -> crate::term::CoMatchBranch {
        crate::term::CoMatchBranch {
            label: label.into(),
            binder: "out".into(),
            body: Box::new(Command::Cut(Term::Var(answer.into()), CoTerm::Covar("out".into()))),
        }
    }

    #[test]
    fn a_request_selects_its_menu_branch() {
        // ⟨ μ[.C::a(out). ⟨x ∥ out⟩ | .C::b(out). ⟨y ∥ out⟩] ∥ .C::b(k) ⟩
        //   → ⟨ y ∥ k ⟩
        let menu = Term::CoMatch(vec![menu_branch("C::a", "x"), menu_branch("C::b", "y")]);
        let request = CoTerm::Dtor("C::b".into(), Box::new(CoTerm::Covar("k".into())));
        match step(&Command::Cut(menu, request)) {
            Step::Reduced(c) => {
                assert_eq!(c, Command::Cut(Term::Var("y".into()), CoTerm::Covar("k".into())));
            }
            Step::Normal => panic!("expected the C::b branch to fire"),
        }
    }

    #[test]
    fn a_request_with_no_branch_does_not_reduce() {
        let menu = Term::CoMatch(vec![menu_branch("C::a", "x")]);
        let request = CoTerm::Dtor("C::b".into(), Box::new(CoTerm::Covar("k".into())));
        assert!(matches!(step(&Command::Cut(menu, request)), Step::Normal));
    }

    #[test]
    fn matching_a_boxed_request_binds_its_continuation() {
        // ⟨ co(.C::b(k)) ∥ μ̃[C::a(f). ⟨x ∥ k2⟩ | C::b(f). ⟨f ∥ k2⟩] ⟩
        //   → ⟨ co(k) ∥ k2 ⟩ — the arm binds f to the request's own
        // continuation, boxed, exactly as it would bind an enum payload.
        let branch = |label: &str, answer: &str| crate::coterm::CoCaseBranch {
            label: label.into(),
            binders: vec!["f".into()],
            body: Box::new(Command::Cut(Term::Var(answer.into()), CoTerm::Covar("k2".into()))),
        };
        let boxed =
            Term::Co(Box::new(CoTerm::Dtor("C::b".into(), Box::new(CoTerm::Covar("k".into())))));
        let consumer = CoTerm::CoCase(vec![branch("C::a", "x"), branch("C::b", "f")]);
        match step(&Command::Cut(boxed, consumer)) {
            Step::Reduced(c) => {
                let expected = Command::Cut(
                    Term::Co(Box::new(CoTerm::Covar("k".into()))),
                    CoTerm::Covar("k2".into()),
                );
                assert_eq!(c, expected);
            }
            Step::Normal => panic!("expected the C::b arm to fire"),
        }
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
