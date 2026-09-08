fn main() {
    // Build the exact IR from parse_json's body manually and evaluate step by step
    use slc_core::{CoTerm, Command, Term};
    use slc_runtime::eval::{eval, eval_command};
    use slc_runtime::value::{Env, Value, install_stdlib};

    let mut env = Env::new();
    install_stdlib(&mut env);

    // Simulate: closure env where input = " 42"
    env.push();
    env.define("input", Value::Str(" 42".into()));

    // The curried call IR: μ__call. ⟨μ__call. ⟨skip_ws ∥ λ̄__f. ⟨input ∥ __call⟩⟩ ∥ λ̄__f. ⟨$int_0 ∥ __call⟩⟩
    let inner = Term::Mu(
        "__call".into(),
        Box::new(Command::Cut(
            Term::Var("skip_ws".into()),
            CoTerm::CoLam(
                "__f".into(),
                Box::new(Command::Cut(Term::Var("input".into()), CoTerm::Covar("__call".into()))),
            ),
        )),
    );
    let outer = Term::Mu(
        "__call".into(),
        Box::new(Command::Cut(
            inner,
            CoTerm::CoLam(
                "__f".into(),
                Box::new(Command::Cut(Term::Var("$int_0".into()), CoTerm::Covar("__call".into()))),
            ),
        )),
    );

    let mut fuel = 100000;
    match eval(&outer, &mut env, &mut fuel) {
        Ok(v) => println!("OK: {}", v.display()),
        Err(e) => println!("ERR: {e}"),
    }
}
