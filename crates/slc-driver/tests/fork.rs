//! `fork`: pure pairs agree, and `exchange` matches named ends.
//!
//! Link programs run on the interpreter. The native runtime does not schedule
//! an exchange; that case is a separate check on hosts that link an ELF.
//! `parallel` overlaps the two forces. Under a fuel bound, a loop beside a
//! division by zero reports the division. `sequential` reports divergence.

use std::process::Command;

fn run(interpret: bool, name: &str, source: &str) -> (String, String, bool) {
    let path = std::env::temp_dir().join(format!("slc_fork_{}_{name}.sl", std::process::id()));
    std::fs::write(&path, source).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_slc"));
    cmd.arg("run");
    if interpret {
        cmd.arg("--interpret");
    }
    cmd.arg(path.to_str().unwrap());
    let out = cmd.output().expect("slc");
    let _ = std::fs::remove_file(&path);
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

fn interpret(name: &str, source: &str) -> (String, String, bool) {
    run(true, name, source)
}

fn run_fuel(interpret: bool, fuel: &str, name: &str, source: &str) -> (String, String, bool) {
    let path = std::env::temp_dir().join(format!("slc_fork_{}_{name}.sl", std::process::id()));
    std::fs::write(&path, source).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_slc"));
    cmd.arg("run").arg("--fuel").arg(fuel);
    if interpret {
        cmd.arg("--interpret");
    }
    cmd.arg(path.to_str().unwrap());
    let out = cmd.output().expect("slc");
    let _ = std::fs::remove_file(&path);
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

const PRELUDE: &str = "\
cite lazy::Lazy;
cite fork::end;
cite fork::talk;
cite fork::exchange;
cite fork::Arms::*;
";

fn program(body: &str) -> String {
    format!("{PRELUDE}\nproc main | (exit: i32) / {{IO}} {{\n{body}\n    <0 | exit>\n}}\n")
}

fn with_funcs(funcs: &str, body: &str) -> String {
    format!("{PRELUDE}\n{funcs}\nproc main | (exit: i32) / {{IO}} {{\n{body}\n    <0 | exit>\n}}\n")
}

#[test]
fn tell_and_hear_meet_in_list_order() {
    let source = program(
        "
    let answers = do {
        let+ hear = end();
        let+ tell = mu Lazy<i64, {fork::Link<i64>}> {
            force <= {
                hear.give(1);
                hear.give(2);
                <2 | force>
            },
        };
        let+ heard = mu Lazy<i64, {fork::Link<i64>}> {
            force <= {
                let a = hear.take;
                let b = hear.take;
                <(<(a, b) | add) | force>
            },
        };
        <Arm(tell, Arm(heard, Stop)) | talk
    } exchange;
    <answers | println;",
    );
    let (stdout, stderr, ok) = interpret("tell", &source);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "[2, 3]\n", "{stderr}");
}

#[test]
fn three_senders_are_served_in_talk_order() {
    let source = with_funcs(
        "
func sender(total: fork::End<i64>, n: i64) -> Lazy<i64, {fork::Link<i64>}> {
    mu Lazy<i64, {fork::Link<i64>}> {
        force <= {
            total.give(n);
            <n | force>
        },
    }
}
",
        "
    let answers = do {
        let+ total = end();
        let+ one = <(total, 1) | sender;
        let+ two = <(total, 2) | sender;
        let+ thirty_nine = <(total, 39) | sender;
        let+ owner = mu Lazy<i64, {fork::Link<i64>}> {
            force <= {
                let a = total.take;
                let b = total.take;
                let c = total.take;
                <(<(<(a, b) | add, c) | add) | force>
            },
        };
        <Arm(one, Arm(two, Arm(thirty_nine, Arm(owner, Stop)))) | talk
    } exchange;
    <answers | println;",
    );
    let (stdout, stderr, ok) = interpret("senders", &source);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "[1, 2, 39, 42]\n", "{stderr}");
}

#[test]
fn a_take_with_no_giver_left_is_unmatched() {
    let source = program(
        "
    let answers = do {
        let+ total = end();
        let+ sender = mu Lazy<i64, {fork::Link<i64>}> {
            force <= { total.give(1); <1 | force> },
        };
        let+ owner = mu Lazy<i64, {fork::Link<i64>}> {
            force <= {
                let a = total.take;
                let _b = total.take;
                <a | force>
            },
        };
        <Arm(sender, Arm(owner, Stop)) | talk
    } exchange;
    <answers | println;",
    );
    let (stdout, stderr, ok) = interpret("unmatched", &source);
    assert!(!ok, "{stdout}");
    assert!(stderr.contains("error: type mismatch: unmatched link"), "{stderr}");
}

#[test]
fn a_second_arm_cannot_take_an_end() {
    let source = program(
        "
    let answers = do {
        let+ hear = end();
        let+ first = mu Lazy<i64, {fork::Link<i64>}> {
            force <= { let n = hear.take; <n | force> },
        };
        let+ second = mu Lazy<i64, {fork::Link<i64>}> {
            force <= { let n = hear.take; <n | force> },
        };
        <Arm(first, Arm(second, Stop)) | talk
    } exchange;
    <answers | println;",
    );
    let (stdout, stderr, ok) = interpret("taken", &source);
    assert!(!ok, "{stdout}");
    assert!(stderr.contains("error: type mismatch: end already taken"), "{stderr}");
}

#[test]
fn a_nested_exchange_is_already_open() {
    let source = program(
        "
    let answers = do {
        let+ hear = end();
        let+ arm = mu Lazy<i64, {fork::Link<i64>}> {
            force <= {
                let nested = do {
                    let+ inner = end();
                    let+ tell = mu Lazy<i64, {fork::Link<i64>}> {
                        force <= { inner.give(1); <1 | force> },
                    };
                    <Arm(tell, Stop) | talk
                } exchange;
                hear.give(1);
                <1 | force>
            },
        };
        <Arm(arm, Stop) | talk
    } exchange;
    <answers | println;",
    );
    let (stdout, stderr, ok) = interpret("nested", &source);
    assert!(!ok, "{stdout}");
    assert!(stderr.contains("error: type mismatch: link already open"), "{stderr}");
}

#[test]
fn a_later_exchange_starts_after_the_first_delivers() {
    let source = with_funcs(
        "
func ping(hear: fork::End<i64>, n: i64) -> Lazy<i64, {fork::Link<i64>}> {
    mu Lazy<i64, {fork::Link<i64>}> {
        force <= { hear.give(n); <n | force> },
    }
}

func pong(hear: fork::End<i64>) -> Lazy<i64, {fork::Link<i64>}> {
    mu Lazy<i64, {fork::Link<i64>}> {
        force <= { let n = hear.take; <n | force> },
    }
}
",
        "
    let first = do {
        let+ hear = end();
        let+ told = <(hear, 1) | ping;
        let+ heard = <hear | pong;
        <Arm(told, Arm(heard, Stop)) | talk
    } exchange;
    let second = do {
        let+ hear = end();
        let+ told = <(hear, 2) | ping;
        let+ heard = <hear | pong;
        <Arm(told, Arm(heard, Stop)) | talk
    } exchange;
    <first | println;
    <second | println;",
    );
    let (stdout, stderr, ok) = interpret("twice", &source);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "[1, 1]\n[2, 2]\n", "{stderr}");
}

#[test]
fn together_refuses_a_thunk_that_performs_an_effect() {
    let source = "\
cite lazy::Lazy;
cite fork::both;
cite fork::parallel;

proc main | (exit: i32) / {IO} {
    let left = mu Lazy<i64, {IO}> {
        force <= { <\"hi\" | println; <1 | force> },
    };
    let right = mu Lazy<i64> { force <= <2 | force> };
    let pair = do (<(left, right) | fork::both) fork::parallel;
    <pair | println;
    <0 | exit>
}
";
    let (stdout, stderr, ok) = interpret("impure", source);
    assert!(!ok, "{stdout}");
    assert!(stderr.contains("type:") || stderr.contains("effect:"), "{stderr}");
}

const PAIR: &str = "\
cite list::List::*;
cite list::length;
cite list::sum;
cite lazy::Lazy;
cite fork::both;
cite fork::sequential;
cite fork::parallel;

func nine() -> lazy::Lazy<i64> {
    mu lazy::Lazy { force <= <9 | force> }
}

func sixteen() -> lazy::Lazy<i64> {
    mu lazy::Lazy { force <= <16 | force> }
}

func length_of(xs: list::List<i64>) -> lazy::Lazy<i64> {
    mu lazy::Lazy { force <= <(<xs | length) | force> }
}

func sum_of(xs: list::List<i64>) -> lazy::Lazy<i64> {
    mu lazy::Lazy { force <= <(<xs | sum) | force> }
}

proc main | (exit: i32) / {IO} {
    let numbers = do (<(nine(), sixteen()) | fork::both) fork::sequential;
    <numbers | println;
    let numbers = do (<(nine(), sixteen()) | fork::both) fork::parallel;
    <numbers | println;
    let xs = Cons(3, Cons(14, Cons(25, Nil)));
    let counted = do (<((<xs | length_of), (<xs | sum_of)) | fork::both) fork::sequential;
    <counted | println;
    let counted = do (<((<xs | length_of), (<xs | sum_of)) | fork::both) fork::parallel;
    <counted | println;
    <0 | exit>
}
";

#[test]
fn sequential_and_parallel_print_the_same_pairs() {
    let (stdout, stderr, ok) = interpret("pair", PAIR);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "(9, 16)\n(9, 16)\n(3, 42)\n(3, 42)\n", "{stderr}");
}

#[test]
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
fn a_native_exchange_reports_that_it_is_not_scheduled() {
    let source = program(
        "
    let answers = do {
        let+ hear = end();
        let+ quiet = mu Lazy<i64, {fork::Link<i64>}> {
            force <= { hear.give(1); <1 | force> },
        };
        <Arm(quiet, Stop) | talk
    } exchange;
    <answers | println;",
    );
    let (stdout, stderr, ok) = run(false, "native", &source);
    assert!(!ok, "{stdout}");
    assert!(
        stderr.contains(
            "error: type mismatch: link exchange is not implemented in the native runtime"
        ),
        "{stderr}"
    );
}

fn overlap_program(hand: &str) -> String {
    format!(
        "\
cite lazy::Lazy;
cite fork::both;
cite fork::sequential;
cite fork::parallel;

func spin(n: i64) -> i64 {{
    <(<(n, 1) | add) | spin
}}

func looping() -> lazy::Lazy<i64> {{
    mu lazy::Lazy {{ force <= <(<0 | spin) | force> }}
}}

func broken() -> lazy::Lazy<i64> {{
    mu lazy::Lazy {{ force <= <(<(1, 0) | div) | force> }}
}}

proc main | (exit: i32) / {{IO}} {{
    let numbers = do (<(looping(), broken()) | fork::both) {hand};
    <numbers | println;
    <0 | exit>
}}
"
    )
}

fn assert_overlap(interpret: bool, fuel: &str) {
    let (stdout, stderr, ok) =
        run_fuel(interpret, fuel, "seq_spin", &overlap_program("fork::sequential"));
    assert!(!ok, "{stdout}{stderr}");
    assert!(stderr.contains("evaluation diverged (fuel exhausted)"), "{stderr}");
    assert!(!stderr.contains("division by zero"), "{stderr}");

    let (stdout, stderr, ok) =
        run_fuel(interpret, fuel, "par_spin", &overlap_program("fork::parallel"));
    assert!(!ok, "{stdout}{stderr}");
    assert!(stderr.contains("error: type mismatch: division by zero"), "{stderr}");
    assert!(!stderr.contains("evaluation diverged"), "{stderr}");
}

#[test]
fn a_loop_does_not_hide_the_other_forces_fatal() {
    assert_overlap(true, "10000");
}

#[test]
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
fn a_native_loop_does_not_hide_the_other_forces_fatal() {
    assert_overlap(false, "10000");
}
