use std::path::PathBuf;
use std::process::Command;

struct Expected {
    success: bool,
    stdout: &'static [&'static str],
    stderr: &'static [&'static str],
}

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name)
}

fn run_example(name: &str) -> (String, String, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_slc"))
        .args(["run", example(name).to_str().unwrap()])
        .current_dir(env!("CARGO_MANIFEST_DIR").to_string() + "/../..")
        .output()
        .expect("failed to run slc");
    (
        String::from_utf8(out.stdout).expect("invalid UTF-8 in stdout"),
        String::from_utf8(out.stderr).expect("invalid UTF-8 in stderr"),
        out.status.success(),
    )
}

#[test]
fn repository_example_suite_has_expected_results() {
    let expected = [
        (
            "arithmetic.sl",
            Expected { success: true, stdout: &["5", "6", "42", "10", "-5"], stderr: &[] },
        ),
        ("logical_units.sl", Expected { success: true, stdout: &["(,)"], stderr: &[] }),
        (
            "composition.sl",
            Expected {
                success: true,
                stdout: &["\"demo\"", "\"slant\"", "2", "2", "\"7\"", "\"quit\""],
                stderr: &[],
            },
        ),
        (
            "comparison.sl",
            Expected {
                success: true,
                stdout: &["true", "true", "true", "true", "true", "true"],
                stderr: &[],
            },
        ),
        (
            "pipeline.sl",
            Expected {
                success: true,
                stdout: &["42", "14", "42", "11", "11", "20", "7", "13", "13"],
                stderr: &[],
            },
        ),
        (
            "codata_impls.sl",
            Expected {
                success: true,
                stdout: &[
                    "\"slant with 3 retries\"",
                    "\"a sink for one number\"",
                    "\"stream starting 7\"",
                    "\"slant with 3 retries\"",
                    "\"a sink for one number\"",
                    "\"42\"",
                    "\"[1, 2]\"",
                    "\"the number 42\"",
                    "\"affirmative\"",
                ],
                stderr: &[],
            },
        ),
        (
            "latent_effects.sl",
            Expected {
                success: true,
                stdout: &["21", "42", "-1", "-1", "70", "700", "21", "-1"],
                stderr: &[],
            },
        ),
        (
            "effects.sl",
            Expected {
                success: true,
                stdout: &["-1", "5", "1070", "HH HT TH TT", "\"[4, 2]\"", "\"[]\"", "42"],
                stderr: &[],
            },
        ),
        (
            "dictionaries.sl",
            Expected { success: true, stdout: &["42", "[77]", "[TT]"], stderr: &[] },
        ),
        (
            "data_functions.sl",
            Expected { success: true, stdout: &["10", "100", "200", "2", "50"], stderr: &[] },
        ),
        (
            "file_io.sl",
            Expected {
                success: true,
                stdout: &[
                    "The simplest Slant program",
                    "Hello, Slant!",
                    "first line: // The simplest Slant program.",
                    "cannot open: cannot open examples/missing.sl",
                ],
                stderr: &[],
            },
        ),
        (
            "connectives.sl",
            Expected {
                success: true,
                // ⊗, then ⅋ three ways, then ⊕, &, a menu's two items, and
                // the units.
                stdout: &["42", "7", "\"green\"", "1", "3", "\"slant\"", "(,)"],
                stderr: &[],
            },
        ),
        (
            "classical.sl",
            Expected {
                success: true,
                // The refutation is taken *after* `lem()` answered: the jump
                // re-enters the match, which then holds.
                stdout: &["42", "refuted — taking the offer", "holds: 42"],
                stderr: &[],
            },
        ),
        ("command.sl", Expected { success: true, stdout: &["42"], stderr: &[] }),
        (
            "io.sl",
            Expected {
                success: true,
                stdout: &[
                    "\"hello, world\"",
                    "captured instead:",
                    "about to write 14 characters",
                    "\"hello, again\"",
                ],
                stderr: &[],
            },
        ),
        (
            "stdlib.sl",
            Expected {
                success: true,
                stdout: &["3", "3", "0", "3", "\"[1, 2, 3]\"", "\"[3, 1, 2]\""],
                stderr: &[],
            },
        ),
        ("hello.sl", Expected { success: true, stdout: &["Hello, Slant!"], stderr: &[] }),
        (
            "json_parser.sl",
            Expected {
                success: true,
                stdout: &[
                    "parsed:",
                    "\\\"name\\\":\\\"slant\\\"",
                    "\\\"tags\\\":[1,2,-3.25]",
                    "\\\"active\\\":true",
                    "\\\"none\\\":null",
                    "escaped",
                ],
                stderr: &[],
            },
        ),
        ("lambda.sl", Expected { success: true, stdout: &["42"], stderr: &[] }),
        (
            "lists.sl",
            Expected { success: true, stdout: &["3", "42", "84", "84", "39", "0"], stderr: &[] },
        ),
        (
            "command_falls_through.sl",
            Expected {
                success: false,
                stdout: &[],
                stderr: &["type:", "must reach a continuation"],
            },
        ),
        ("match_exhaustive.sl", Expected { success: true, stdout: &["red"], stderr: &[] }),
        ("mu_escape.sl", Expected { success: true, stdout: &["42"], stderr: &[] }),
        (
            "form.sl",
            Expected {
                success: true,
                stdout: &["\"answer\"", "42", "\"relabelled!\"", "7"],
                stderr: &[],
            },
        ),
        (
            "menu.sl",
            Expected { success: true, stdout: &["3", "\"slant\"", "\"slant!\"", "3"], stderr: &[] },
        ),
        (
            "mu_tilde.sl",
            Expected {
                success: true,
                stdout: &["42", "42", "the value was consumed", "17", "100"],
                stderr: &[],
            },
        ),
        ("nested_calls.sl", Expected { success: true, stdout: &["\"320\""], stderr: &[] }),
        (
            "multi.sl",
            Expected {
                success: true,
                stdout: &["42, yes", "n (4 digits)", "50", "-1"],
                stderr: &[],
            },
        ),
        ("namespaces.sl", Expected { success: true, stdout: &["75", "420", "0"], stderr: &[] }),
        ("pair.sl", Expected { success: true, stdout: &["30"], stderr: &[] }),
        (
            "patterns.sl",
            Expected { success: true, stdout: &["13", "71", "11", "25", "2"], stderr: &[] },
        ),
        ("projection.sl", Expected { success: true, stdout: &["60", "6"], stderr: &[] }),
        ("polarity.sl", Expected { success: true, stdout: &["3", "positive"], stderr: &[] }),
        (
            "polarity_error.sl",
            Expected {
                success: false,
                stdout: &[],
                stderr: &["polarity:", "parameter `j` has explicitly positive type"],
            },
        ),
        (
            "seq.sl",
            Expected {
                success: true,
                stdout: &[
                    "\"[2, 6, 10]\"",
                    "\"[1, 3, 5, 7]\"",
                    "\"[1, 2, 3, 4, 5, 6, 7, 8, 9]\"",
                    "\"[64, 32, 16, 8, 4]\"",
                    "\"[4, 5, 6]\"",
                    "\"[1, 2, 4, 8, 16]\"",
                ],
                stderr: &[],
            },
        ),
        ("select.sl", Expected { success: true, stdout: &["1", "42", "100"], stderr: &[] }),
        (
            "stream.sl",
            Expected {
                success: true,
                stdout: &["10", "11", "13", "22", "\"[10, 11, 12]\"", "\"[7, 7]\""],
                stderr: &[],
            },
        ),
        (
            "strings.sl",
            Expected { success: true, stdout: &["Hello, world!", "H", "world"], stderr: &[] },
        ),
        (
            "tree_search.sl",
            Expected {
                success: true,
                // The first search never visits 4: the hit jumps out.
                stdout: &[
                    "visiting 3",
                    "visiting 1",
                    "visiting 2",
                    "found: 2",
                    "visiting 3",
                    "visiting 1",
                    "visiting 2",
                    "visiting 4",
                    "missing: -1",
                ],
                stderr: &[],
            },
        ),
        (
            "two_styles.sl",
            Expected {
                success: true,
                // The value-first half and the continuation-first half print
                // the same two answers, in the same order — written the
                // same way, since the two styles share a type.
                stdout: &["big", "small", "big", "small", "and directly: big"],
                stderr: &[],
            },
        ),
    ];

    for (name, expected) in expected {
        let (stdout, stderr, success) = run_example(name);
        assert_eq!(success, expected.success, "{name}: stdout={stdout:?}, stderr={stderr:?}");
        for fragment in expected.stdout {
            assert!(stdout.contains(fragment), "{name}: missing stdout {fragment:?} in {stdout:?}");
        }
        for fragment in expected.stderr {
            assert!(stderr.contains(fragment), "{name}: missing stderr {fragment:?} in {stderr:?}");
        }
    }
}
