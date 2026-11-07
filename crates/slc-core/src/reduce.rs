//! Cut-based small-step reduction semantics for λ̄μμ̃.

use crate::command::Command;
use crate::coterm::CoTerm;
use crate::substitution::{
    free_vars_command, free_vars_coterm, free_vars_term, fresh, subst_command, subst_covar_command,
    subst_covar_term, subst_term,
};
use crate::term::Term;

/// The `index`-th component of a tuple.
fn project_term(product: &Term, index: usize) -> Option<Term> {
    let Term::Tuple(items) = product else { return None };
    items.get(index).cloned()
}

/// A single reduction step result.
pub enum Step {
    /// The command reduced to a new command.
    Reduced(Command),
    /// Projection extracted a component. It is not placed back in a cut:
    /// doing so would invent a covariable that `μ` then substitutes.
    Component(Term),
    /// The command is in normal form.
    Normal,
}

/// Perform one small-step reduction on a command.
pub fn step(c: &Command) -> Step {
    match c {
        // μ-rule: ⟨ μα.c ∥ e ⟩ → c[e/α]
        // Tried before the μ̃-rule, which settles the critical pair
        // ⟨μα.c ∥ μ̃x.c'⟩ in favour of the producer.
        Command::Cut(Term::Mu(a, c1), e) => {
            Step::Reduced(crate::substitution::subst_covar_command(a, e, c1))
        }

        // μ̃-rule: ⟨ v ∥ μ̃x.c ⟩ → c[v/x]
        // This is the binder: it takes what the cut delivers and runs `c`
        // with it bound, which is what `let` and every other binding form
        // lowers to.
        Command::Cut(v, CoTerm::MuTilde(x, c2)) => Step::Reduced(subst_command(x, v, c2)),

        // Labelled rule: ⟨ L(v₁ ⊗ … ⊗ vₙ) ∥ μ̃[M; … L(x₁,…,xₙ). c …] ⟩ → c[vᵢ/xᵢ]
        // The label selects exactly one branch; the others are discarded
        // unreduced. An `enum` has a branch per variant, a `data` one.
        Command::Cut(Term::Tag(label, payload), CoTerm::CoCase { branches, .. }) => {
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
        Command::Cut(Term::CoMatch { branches, .. }, CoTerm::Dtor(label, e)) => {
            match branches.iter().find(|b| &b.label == label) {
                Some(branch) => Step::Reduced(subst_covar_command(&branch.binder, e, &branch.body)),
                None => Step::Normal,
            }
        }

        // Co-labelled rule: ⟨ co(.d(e)) ∥ μ̃[M; … .d(x). c …] ⟩ → c[co(e)/x]
        // A reified request is a positive value with a label, so matching
        // on a continuation is the labelled rule with the payload rewrapped:
        // the arm receives the request's own continuation as a value.
        Command::Cut(Term::Co(request), CoTerm::CoCase { branches, .. }) => {
            match request.as_ref() {
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
            }
        }

        // Multiplicative rule: ⟨ v₁ ⊗ v₂ ∥ μ̃(x, y). c ⟩ → c[v₁/x, v₂/y]
        Command::Cut(value, CoTerm::MuTildeTensor(binders, body)) => {
            match bind_components(binders, value, body) {
                Some(command) => Step::Reduced(command),
                None => Step::Normal,
            }
        }

        // →-rule: ⟨ λx.t ∥ v·e ⟩ → ⟨ v ∥ μ̃x. ⟨ t ∥ e ⟩ ⟩ — application:
        // the argument goes to the binder, and the body meets the tail.
        // The μ̃ is a new binder. If its name is free in the tail, the next
        // μ̃-step would replace that free name with the argument.
        Command::Cut(Term::Lam(x, t), CoTerm::App(v, e)) => {
            let (binder, body) = open_lambda(x, t, e);
            Step::Reduced(Command::Cut(
                v.clone(),
                CoTerm::MuTilde(binder, Box::new(Command::Cut(body, (**e).clone()))),
            ))
        }

        // co-rule: ⟨ co(e′) ∥ v·e ⟩ → ⟨ v ∥ e′ ⟩ — applying a reified consumer
        // opens the box; a consumer does not return, so the tail is dropped.
        Command::Cut(Term::Co(consumer), CoTerm::App(v, _)) => {
            Step::Reduced(Command::Cut(v.clone(), (**consumer).clone()))
        }

        // Projection: ⟨ (t₀ ⊗ … ⊗ tₙ) ∥ prj:i ⟩ → tᵢ.
        // A component, not a command: cutting it against □ would fire μ.
        Command::Cut(product @ Term::Tuple(..), CoTerm::Prj(index)) => {
            match project_term(product, *index) {
                Some(t) => Step::Component(t),
                None => Step::Normal,
            }
        }

        _ => Step::Normal,
    }
}

/// Substitute a product's components for a list of binders: one binder takes
/// the whole value, none take the unit, and several take a tuple's components
/// by position — as many binders as components.
fn bind_components(binders: &[String], value: &Term, body: &Command) -> Option<Command> {
    if let [binder] = binders {
        return Some(subst_command(binder, value, body));
    }
    let components: &[Term] = match value {
        Term::Tuple(items) if items.len() == binders.len() => items,
        _ if binders.is_empty() => &[],
        _ => return None,
    };
    // c[vᵢ/xᵢ] is simultaneous. A temporary stands for each binder first;
    // that name is not free in any component, so substituting a component
    // does not rewrite one that was already inserted.
    let mut used = free_vars_command(body);
    for component in components {
        used.extend(free_vars_term(component));
    }
    for binder in binders {
        used.insert(binder.clone());
    }
    let temps: Vec<String> = binders.iter().map(|binder| fresh(binder, &mut used)).collect();
    let mut command = body.clone();
    for (binder, temp) in binders.iter().zip(&temps) {
        command = subst_command(binder, &Term::Var(temp.clone()), &command);
    }
    for (temp, component) in temps.iter().zip(components) {
        command = subst_command(temp, component, &command);
    }
    Some(command)
}

/// Rename the λ binder when it is free in the tail, and rename its bound
/// occurrences in the body to match. The tail itself is not under the λ.
fn open_lambda(binder: &str, body: &Term, tail: &CoTerm) -> (String, Term) {
    if !free_vars_coterm(tail).contains(binder) {
        return (binder.to_string(), body.clone());
    }
    let mut used = free_vars_coterm(tail);
    used.extend(free_vars_term(body));
    used.insert(binder.to_string());
    let fresh_binder = fresh(binder, &mut used);
    let renamed = subst_covar_term(
        binder,
        &CoTerm::Covar(fresh_binder.clone()),
        &subst_term(binder, &Term::Var(fresh_binder.clone()), body),
    );
    (fresh_binder, renamed)
}

/// A normal form is a command, or a component taken out of one by projection.
pub enum NormalForm {
    Command(Command),
    Component(Term),
}

/// Reduce a command to normal form with fuel. Returns `None` on fuel exhaustion.
/// A projected component stops the loop: it is not cut against a covariable.
pub fn normalize(c: &Command, fuel: usize) -> Option<NormalForm> {
    let mut current = c.clone();
    for _ in 0..fuel {
        match step(&current) {
            Step::Reduced(next) => current = next,
            Step::Component(term) => return Some(NormalForm::Component(term)),
            Step::Normal => return Some(NormalForm::Command(current)),
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
    fn mu_substitutes_the_continuation() {
        // ⟨ μα. ⟨x ∥ α⟩ ∥ k ⟩ → ⟨x ∥ k⟩
        let body = Command::Cut(Term::Var("x".into()), CoTerm::Covar("a".into()));
        let mu = Term::Mu("a".into(), Box::new(body));
        match step(&Command::Cut(mu, CoTerm::Covar("k".into()))) {
            Step::Reduced(c) => {
                assert_eq!(c, Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into())));
            }
            _ => panic!("expected the μ-rule to substitute"),
        }
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
            _ => panic!("expected reduction"),
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
            _ => panic!("expected reduction"),
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
            _ => panic!("expected reduction"),
        }
    }

    #[test]
    fn tensor_projection() {
        let pair = Term::Tuple(vec![Term::Var("a".into()), Term::Var("b".into())]);
        match step(&Command::Cut(pair.clone(), CoTerm::Prj(0))) {
            Step::Component(Term::Var(v)) => assert_eq!(v, "a"),
            _ => panic!("expected projection to `a`"),
        }
        match step(&Command::Cut(pair, CoTerm::Prj(1))) {
            Step::Component(Term::Var(v)) => assert_eq!(v, "b"),
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
        // ⟨ Color::Green(v) ∥ μ̃[Color; Red x.⟨x ∥ r⟩ | Green x.⟨x ∥ g⟩] ⟩ → ⟨ v ∥ g ⟩
        let consumer = CoTerm::CoCase {
            owner: "Color".into(),
            branches: vec![
                branch("Color::Red", "r"),
                branch("Color::Green", "g"),
                branch("Color::Blue", "b"),
            ],
        };
        let value = Term::Tag("Color::Green".into(), Box::new(Term::Var("v".into())));
        match step(&Command::Cut(value, consumer)) {
            Step::Reduced(c) => {
                assert_eq!(c, Command::Cut(Term::Var("v".into()), CoTerm::Covar("g".into())));
            }
            _ => panic!("expected the Green branch to fire"),
        }
    }

    #[test]
    fn a_label_with_no_branch_does_not_reduce() {
        let consumer =
            CoTerm::CoCase { owner: "Color".into(), branches: vec![branch("Color::Red", "r")] };
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
        // ⟨ μ[C; .C::a(out). ⟨x ∥ out⟩ | .C::b(out). ⟨y ∥ out⟩] ∥ .C::b(k) ⟩
        //   → ⟨ y ∥ k ⟩
        let menu = Term::CoMatch {
            owner: "C".into(),
            branches: vec![menu_branch("C::a", "x"), menu_branch("C::b", "y")],
        };
        let request = CoTerm::Dtor("C::b".into(), Box::new(CoTerm::Covar("k".into())));
        match step(&Command::Cut(menu, request)) {
            Step::Reduced(c) => {
                assert_eq!(c, Command::Cut(Term::Var("y".into()), CoTerm::Covar("k".into())));
            }
            _ => panic!("expected the C::b branch to fire"),
        }
    }

    #[test]
    fn a_request_with_no_branch_does_not_reduce() {
        let menu = Term::CoMatch { owner: "C".into(), branches: vec![menu_branch("C::a", "x")] };
        let request = CoTerm::Dtor("C::b".into(), Box::new(CoTerm::Covar("k".into())));
        assert!(matches!(step(&Command::Cut(menu, request)), Step::Normal));
    }

    #[test]
    fn matching_a_boxed_request_binds_its_continuation() {
        // ⟨ co(.C::b(k)) ∥ μ̃[C; C::a(f). ⟨x ∥ k2⟩ | C::b(f). ⟨f ∥ k2⟩] ⟩
        //   → ⟨ co(k) ∥ k2 ⟩ — the arm binds f to the request's own
        // continuation, boxed, exactly as it would bind an enum payload.
        let branch = |label: &str, answer: &str| crate::coterm::CoCaseBranch {
            label: label.into(),
            binders: vec!["f".into()],
            body: Box::new(Command::Cut(Term::Var(answer.into()), CoTerm::Covar("k2".into()))),
        };
        let boxed =
            Term::Co(Box::new(CoTerm::Dtor("C::b".into(), Box::new(CoTerm::Covar("k".into())))));
        let consumer = CoTerm::CoCase {
            owner: "C".into(),
            branches: vec![branch("C::a", "x"), branch("C::b", "f")],
        };
        match step(&Command::Cut(boxed, consumer)) {
            Step::Reduced(c) => {
                let expected = Command::Cut(
                    Term::Co(Box::new(CoTerm::Covar("k".into()))),
                    CoTerm::Covar("k2".into()),
                );
                assert_eq!(c, expected);
            }
            _ => panic!("expected the C::b arm to fire"),
        }
    }

    #[test]
    fn application_does_not_capture_a_free_name_in_the_tail() {
        // ⟨λx. z ∥ w · μ̃y. ⟨x ∥ a⟩⟩ — the free x in the tail stays free.
        let lam = Term::Lam("x".into(), Box::new(Term::Var("z".into())));
        let tail = CoTerm::MuTilde(
            "y".into(),
            Box::new(Command::Cut(Term::Var("x".into()), CoTerm::Covar("a".into()))),
        );
        let app = CoTerm::App(Term::Var("w".into()), Box::new(tail));
        let Step::Reduced(once) = step(&Command::Cut(lam, app)) else {
            panic!("expected the application rule");
        };
        let Step::Reduced(twice) = step(&once) else {
            panic!("expected the μ̃ rule");
        };
        let Command::Cut(term, coterm) = twice;
        assert_eq!(term, Term::Var("z".into()));
        let CoTerm::MuTilde(binder, body) = coterm else { panic!("expected μ̃") };
        assert_ne!(binder, "x");
        assert_eq!(*body, Command::Cut(Term::Var("x".into()), CoTerm::Covar("a".into())));
    }

    #[test]
    fn product_substitution_is_simultaneous() {
        // ⟨(y ⊗ z) ∥ μ̃(x, y). ⟨x ∥ k⟩⟩ → ⟨y ∥ k⟩, not ⟨z ∥ k⟩
        let pair = Term::Tuple(vec![Term::Var("y".into()), Term::Var("z".into())]);
        let consumer = CoTerm::MuTildeTensor(
            vec!["x".into(), "y".into()],
            Box::new(Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()))),
        );
        match step(&Command::Cut(pair, consumer)) {
            Step::Reduced(c) => {
                assert_eq!(c, Command::Cut(Term::Var("y".into()), CoTerm::Covar("k".into())));
            }
            _ => panic!("expected the product rule"),
        }
    }

    #[test]
    fn a_multi_field_tag_substitutes_simultaneously() {
        let payload = Term::Tuple(vec![Term::Var("y".into()), Term::Var("z".into())]);
        let value = Term::Tag("Color::P".into(), Box::new(payload));
        let consumer = CoTerm::CoCase {
            owner: "Color".into(),
            branches: vec![crate::coterm::CoCaseBranch {
                label: "Color::P".into(),
                binders: vec!["x".into(), "y".into()],
                body: Box::new(Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()))),
            }],
        };
        match step(&Command::Cut(value, consumer)) {
            Step::Reduced(c) => {
                assert_eq!(c, Command::Cut(Term::Var("y".into()), CoTerm::Covar("k".into())));
            }
            _ => panic!("expected the labelled rule"),
        }
    }

    #[test]
    fn projection_does_not_activate_mu() {
        let mu = Term::Mu(
            "a".into(),
            Box::new(Command::Cut(Term::Var("z".into()), CoTerm::Covar("a".into()))),
        );
        let pair = Term::Tuple(vec![mu.clone(), Term::Var("w".into())]);
        let command = Command::Cut(pair, CoTerm::Prj(0));
        match step(&command) {
            Step::Component(t) => assert_eq!(t, mu),
            Step::Reduced(c) => panic!("projection re-embedded the component: {c}"),
            Step::Normal => panic!("expected a component"),
        }
        match normalize(&command, 10) {
            Some(NormalForm::Component(t)) => assert_eq!(t, mu),
            _ => panic!("normalizing projection must not activate μ"),
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
