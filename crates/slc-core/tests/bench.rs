//! Lightweight benchmarks for net reduction (no external deps).
//! Run with `cargo test -p slc-core --test bench -- --nocapture`.

use slc_core::command::Command;
use slc_core::coterm::CoTerm;
use slc_core::net::{AgentKind, Net, Port, normalize};
use slc_core::term::Term;
use std::time::Instant;

fn bench(name: &str, iterations: usize, f: impl Fn()) {
    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let elapsed = start.elapsed();
    let per_iter = elapsed / iterations as u32;
    println!("{name}: {iterations} iters in {elapsed:?} ({per_iter:?}/iter)");
}

#[test]
fn bench_net_reduction() {
    bench("identity", 10_000, || {
        let mut net = Net::new();
        let a = net.add_agent(AgentKind::Lam, 1);
        let b = net.add_agent(AgentKind::MuTilde, 1);
        net.connect(Port::principal(a), Port::principal(b));
        let _ = normalize(&net, 100);
    });
    bench("tensor_par", 10_000, || {
        let mut net = Net::new();
        let a = net.add_agent(AgentKind::Tensor, 2);
        let b = net.add_agent(AgentKind::Par, 2);
        net.connect(Port::principal(a), Port::principal(b));
        let _ = normalize(&net, 100);
    });
}

#[test]
fn bench_net_compilation() {
    let nested = {
        let mut t = Term::Var("x".into());
        for i in 0..100 {
            t = Term::Lam(format!("v{i}"), Box::new(t));
        }
        t
    };
    bench("compile/nested_lams_100", 1_000, || {
        let _ = slc_core::net::compile_term(&nested);
    });
    bench("compile/cut", 10_000, || {
        let t = Term::Lam("x".into(), Box::new(Term::Var("x".into())));
        let e = CoTerm::MuTilde(
            "y".into(),
            Box::new(Command::Cut(Term::Var("y".into()), CoTerm::Covar("k".into()))),
        );
        let _ = slc_core::net::compile_term(&Term::Mu("m".into(), Box::new(Command::Cut(t, e))));
    });
}
