//! An alternative flows into a consumer function of its sum, and the chain
//! carries on with what the function hands its continuation — no `mu` to
//! name the continuation.

use std::process::Command;

#[test]
fn an_alternative_is_read_through_a_consumer_function() {
    let path = std::env::temp_dir().join("slc_sum_through_consumer.sl");
    std::fs::write(
        &path,
        r#"fn describe(out: String) <- (i64 | String) {
            select (i64 | String) {
                ::0(n) => <("number ", <n | int_to_str) | add | out>,
                ::1(s) => <("text ", s) | add | out>,
            }
        }

        fn rank(out: String) <- (i64 | Bool | String) {
            select (i64 | Bool | String) {
                ::0(n) => <("first ", <n | int_to_str) | add | out>,
                ::1(b) => <"second" | out>,
                ::2(s) => <("third ", s) | add | out>,
            }
        }

        command classify(n: i64) | (outcome: (i64 | String)) {
            match (<(n, 10) | lt) { True => <::0(n) | outcome>, False => <::1("big") | outcome> }
        }

        command main | (exit: -i32) / {IO} {
            <::0(7) | describe | println;
            <::1("hi") | describe | println;
            <::2("last") | rank | println;
            // A command still takes what flows in as its values.
            <mu String {
                s <= <42 | classify | (select i64 { n => <"small" | s> } & select String { t => <t | s> })>
            } | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("failed to run slc");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(out.status.success(), "stderr: {stderr}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "number 7\ntext hi\nthird last\nbig\n");
}
