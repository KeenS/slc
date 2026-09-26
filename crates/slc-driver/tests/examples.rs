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
fn updated_feature_examples_have_exact_outputs() {
    for (name, expected) in [
        (
            "basics/into.sl",
            "\
7
7
7
4
40000
9
200
-3
",
        ),
        (
            "basics/defaults.sl",
            "\
false
true
false
true
",
        ),
        (
            "basics/supertraits.sl",
            "\
false
true
1
false
",
        ),
        (
            "basics/associated.sl",
            "\
3
one
8
4
0
",
        ),
        (
            "basics/builders.sl",
            "\
{a: 2, m: 1}
{z: 9}
{}
{a: 2, m: 1}
{a: 2, b: 4, m: 1}
{1, 2}
{m: 1, a: 2}
{a, b}
",
        ),
        (
            "basics/sets.sl",
            "\
{m: 1}
{m: 9, a: 2}
2
9
-1
true
2
{m: 9}
{a: 9, c: 4}
2
true
true
false
true
{B: 2}
2
{a, b}
{a}
{1, 2, 3}
[1, 2, 3]
{1, 3}
3
",
        ),
        (
            "basics/hash.sl",
            "\
0
8438048531980770162
7046029254386353131
8438048531980770162
8438048531980770162
8288065088631931893
1242035834245578762
0
2177342782468422677
5472609002491880229
620445648566982762
620445648566982762
623241706636955660
",
        ),
        (
            "basics/array.sl",
            "\
2
[1, 2]
2
-1
4
[a, b, c, d]
a
d
[a, b, z, d]
[a, b, c, d]
nothing at that index
0
[]
20
[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20]
1
4
5
16
17
20
-1
-1
100
17
[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 100, 18, 19, 20]
65
1
64
65
",
        ),
        (
            "basics/maps.sl",
            "\
true
false
true
true
true
{m: 1}
{a: 2, m: 1, z: 3}
1
nothing for that key
{a: 2, m: 9, z: 3}
{a: 2, m: 1, z: 3}
{a: 2, b: 3}
true
false
2
{a: 2, b: 3}
{b: 3}
7
3
[(1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6), (7, 7)]
3
[(1, 1), (2, 2), (3, 3), (5, 5), (6, 6), (7, 7)]
3
[(1, 1), (2, 2), (3, 3), (5, 5), (7, 7)]
3
[(1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6), (7, 7)]
3
[(1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6), (7, 7)]
{}
0
",
        ),
        ("effects/handler_forwarding.sl", "complete: 30\nforwarded: 42\n"),
        ("effects/escaping_exits.sl", "stored without running\ntick on activation\n42\n"),
        ("effects/handler_answers.sl", "resumed: answer: 42\n"),
        ("duality/consumer_returns.sl", "1\nreturned from sink\nconsumer constructed\n2\n"),
        (
            "laziness/stream_effects.sl",
            "built, not demanded\n[10, 20, 30]\n[100, 200, 300]\n[2, 4]\n",
        ),
        (
            "laziness/by_name_components.sl",
            "positive field\nstored\nrecord build\n11\nrecord build\n22\nchoice build\n33\n",
        ),
        (
            "laziness/delayed_bundle.sl",
            "stored\nbuild for value\n10\nbuild for callback\nprojected, not activated\nbuild again\nuse callback\n41\n",
        ),
        (
            "laziness/flow_evaluation.sl",
            "0\n0\n0\nbuild at demand\nbuild at demand\n23\nbuild at demand\nbuild at demand\n23\nbuild at demand\nbuild at demand\n23\n",
        ),
        ("duality/yielding_commands.sl", "14\n-14\n42\n"),
        ("effects/handler_values.sl", "140\n72\n42\n"),
        ("effects/generic_effects.sl", "42\nhello\n7\n"),
        ("effects/composable_capture.sl", "30\n"),
        ("laziness/inferred_demand.sl", "0\ndemand\ndemand\n3\nstored\ncalled\n42\ncalled\n42\n"),
        (
            "laziness/delayed_and_lazy.sl",
            "build now\nready\nuse\nuse\n11\n12\nbuild again\n23\nlazy build\nlazy ready\nlazy use\n34\n",
        ),
        (
            "laziness/lazy_effect_phases.sl",
            "factory build\nfactory ready\nfirst use\n11\nsecond use\n12\nlazy build\nlazy ready\nthird use\n23\nfourth use\n24\nlazy rebuild\nfifth use\n35\n",
        ),
    ] {
        let (stdout, stderr, success) = run_example(name);
        assert!(success, "{name}: stdout={stdout:?}, stderr={stderr:?}");
        assert!(stderr.is_empty(), "{name}: {stderr}");
        assert_eq!(stdout, expected, "{name}");
    }
}

#[test]
fn repository_example_suite_has_expected_results() {
    let expected = [
        (
            "basics/arithmetic.sl",
            Expected { success: true, stdout: &["5", "6", "42", "10", "-5"], stderr: &[] },
        ),
        (
            "basics/floats.sl",
            Expected {
                success: true,
                stdout: &["1.5", "3.75", "2.75", "-1.5", "true", "matched", "ranged"],
                stderr: &[],
            },
        ),
        (
            "basics/primitive_widths.sl",
            Expected {
                success: true,
                stdout: &["1.25", "7", "9", "3", "7", "3.75", "f32 exact", "f32 range"],
                stderr: &[],
            },
        ),
        ("duality/logical_units.sl", Expected { success: true, stdout: &["(,)"], stderr: &[] }),
        (
            "basics/sums.sl",
            Expected {
                success: true,
                stdout: &["number 7", "text hi", "second, yes", "third last", "3", "big"],
                stderr: &[],
            },
        ),
        (
            "duality/composition.sl",
            Expected {
                success: true,
                stdout: &["demo", "slant", "2", "2", "7", "quit"],
                stderr: &[],
            },
        ),
        (
            "basics/comparison.sl",
            Expected {
                success: true,
                stdout: &["true", "true", "true", "true", "true", "true"],
                stderr: &[],
            },
        ),
        (
            "duality/pipeline.sl",
            Expected {
                success: true,
                stdout: &["42", "14", "42", "11", "11", "20", "7", "13", "13"],
                stderr: &[],
            },
        ),
        (
            "duality/codata_impls.sl",
            Expected {
                success: true,
                stdout: &[
                    "slant with 3 retries",
                    "a sink for one number",
                    "stream starting 7",
                    "slant with 3 retries",
                    "a sink for one number",
                    "42",
                    "[1, 2]",
                    "the number 42",
                    "affirmative",
                ],
                stderr: &[],
            },
        ),
        (
            "effects/latent_effects.sl",
            Expected {
                success: true,
                stdout: &["21", "42", "-1", "-1", "70", "700", "21", "-1"],
                stderr: &[],
            },
        ),
        (
            "effects/effects.sl",
            Expected {
                success: true,
                stdout: &["-1", "5", "1070", "HH HT TH TT", "[4, 2]", "[]", "42"],
                stderr: &[],
            },
        ),
        (
            "basics/dictionaries.sl",
            Expected { success: true, stdout: &["42", "[77]", "[TT]"], stderr: &[] },
        ),
        (
            "duality/data_functions.sl",
            Expected { success: true, stdout: &["10", "100", "200", "2", "50"], stderr: &[] },
        ),
        (
            "programs/file_io.sl",
            Expected {
                success: true,
                stdout: &[
                    "The simplest SLC program",
                    "Hello, SLC!",
                    "first line: // The simplest SLC program.",
                    "cannot open: cannot open examples/missing.sl",
                ],
                stderr: &[],
            },
        ),
        (
            "duality/connectives.sl",
            Expected {
                success: true,
                // `,`, then `;` three ways, then `|`, `&`, a menu's two items, and
                // the units.
                stdout: &["42", "7", "green", "1", "3", "slant", "(,)"],
                stderr: &[],
            },
        ),
        (
            "duality/classical.sl",
            Expected {
                success: true,
                // The refutation is taken *after* `lem()` answered: the jump
                // re-enters the match, which then holds.
                stdout: &["42", "refuted — taking the offer", "holds: 42"],
                stderr: &[],
            },
        ),
        ("duality/command.sl", Expected { success: true, stdout: &["42"], stderr: &[] }),
        (
            "effects/io.sl",
            Expected {
                success: true,
                stdout: &[
                    "hello, world",
                    "captured instead:",
                    "about to write 12 characters",
                    "hello, again",
                ],
                stderr: &[],
            },
        ),
        (
            "basics/stdlib.sl",
            Expected {
                success: true,
                stdout: &[
                    "3",
                    "3",
                    "0",
                    "3",
                    "6",
                    "72",
                    "-1",
                    "true",
                    "(3, 2)",
                    "[1, 2, 3]",
                    "[3, 1, 2]",
                ],
                stderr: &[],
            },
        ),
        ("basics/hello.sl", Expected { success: true, stdout: &["Hello, SLC!"], stderr: &[] }),
        (
            "programs/json_parser.sl",
            Expected {
                success: true,
                stdout: &[
                    "parsed:",
                    "\"name\":\"slant\"",
                    "\"tags\":[1,2,-3.25]",
                    "\"active\":true",
                    "\"none\":null",
                    "escaped",
                ],
                stderr: &[],
            },
        ),
        ("basics/lambda.sl", Expected { success: true, stdout: &["42"], stderr: &[] }),
        (
            "basics/lists.sl",
            Expected { success: true, stdout: &["3", "42", "84", "84", "39", "0"], stderr: &[] },
        ),
        (
            "errors/command_falls_through.sl",
            Expected {
                success: false,
                stdout: &[],
                stderr: &["type:", "must reach a continuation"],
            },
        ),
        ("basics/match_exhaustive.sl", Expected { success: true, stdout: &["red"], stderr: &[] }),
        ("duality/mu_escape.sl", Expected { success: true, stdout: &["42"], stderr: &[] }),
        (
            "effects/delimited.sl",
            Expected {
                success: true,
                // Each resumption's jump lands in that resumption, with or
                // without a `reset` in between.
                stdout: &["H T\nH T\nbig\nnegative, stopping early\n42\n"],
                stderr: &[],
            },
        ),
        (
            "errors/delimited_error.sl",
            Expected {
                success: false,
                stdout: &["5\n"],
                stderr: &["left the handler it was captured under"],
            },
        ),
        (
            "duality/form.sl",
            Expected { success: true, stdout: &["answer", "42", "relabelled!", "7"], stderr: &[] },
        ),
        (
            "programs/mealy_machine.sl",
            Expected {
                success: true,
                stdout: &[
                    "alarm",
                    "turn",
                    "[alarm, open, refund, turn, alarm]",
                    "[alarm, wait, open, turn, alarm]",
                ],
                stderr: &[],
            },
        ),
        (
            "programs/string_builder.sl",
            Expected { success: true, stdout: &["left: 42", "right"], stderr: &[] },
        ),
        (
            "duality/menu.sl",
            Expected { success: true, stdout: &["3", "slant", "slant!", "3"], stderr: &[] },
        ),
        (
            "duality/mu_tilde.sl",
            Expected {
                success: true,
                stdout: &["42", "42", "the value was consumed", "17", "100"],
                stderr: &[],
            },
        ),
        ("basics/nested_calls.sl", Expected { success: true, stdout: &["320"], stderr: &[] }),
        (
            "effects/multi.sl",
            Expected {
                success: true,
                stdout: &["42, yes", "n (4 digits)", "1234 is 4 wide", "50", "-1"],
                stderr: &[],
            },
        ),
        (
            "basics/namespaces.sl",
            Expected { success: true, stdout: &["75", "420", "0"], stderr: &[] },
        ),
        ("basics/pair.sl", Expected { success: true, stdout: &["30"], stderr: &[] }),
        (
            "basics/patterns.sl",
            Expected { success: true, stdout: &["13", "71", "11", "25", "2"], stderr: &[] },
        ),
        ("basics/projection.sl", Expected { success: true, stdout: &["60", "6"], stderr: &[] }),
        (
            "duality/polarity.sl",
            Expected { success: true, stdout: &["3", "positive"], stderr: &[] },
        ),
        (
            "errors/polarity_error.sl",
            Expected {
                success: false,
                stdout: &[],
                stderr: &["polarity:", "parameter `j` has explicitly positive type"],
            },
        ),
        (
            "laziness/seq.sl",
            Expected {
                success: true,
                stdout: &[
                    "[2, 6, 10]",
                    "[1, 3, 5, 7]",
                    "[1, 2, 3, 4, 5, 6, 7, 8, 9]",
                    "[64, 32, 16, 8, 4]",
                    "[4, 5, 6]",
                    "[1, 2, 4, 8, 16]",
                ],
                stderr: &[],
            },
        ),
        ("duality/select.sl", Expected { success: true, stdout: &["1", "42", "100"], stderr: &[] }),
        (
            "laziness/stream.sl",
            Expected {
                success: true,
                stdout: &["10", "11", "13", "22", "[10, 11, 12]", "[7, 7]"],
                stderr: &[],
            },
        ),
        (
            "basics/strings.sl",
            Expected { success: true, stdout: &["Hello, world!", "H", "world"], stderr: &[] },
        ),
        (
            "programs/multi_file/main.sl",
            Expected {
                success: true,
                stdout: &["circle of area 75", "rectangle of area 42"],
                stderr: &[],
            },
        ),
        (
            "programs/regex_derivative.sl",
            Expected {
                success: true,
                stdout: &[
                    "(ab)*",
                    "  a  b(ab)*",
                    "  b  (ab)*",
                    "  a  b(ab)*",
                    "  b  (ab)*",
                    "  nullable: true",
                    "  a  b(ab)*",
                    "  b  (ab)*",
                    "  a  b(ab)*",
                    "  nullable: false",
                    "(a|b)*abb",
                    "  b  (a|b)*abb",
                    "  a  ((a|b)*abb|bb)",
                    "  b  ((a|b)*abb|b)",
                    "  b  ((a|b)*abb|ε)",
                    "  nullable: true",
                    "true",
                    "false",
                    "false",
                    "true",
                ],
                stderr: &[],
            },
        ),
        (
            "programs/tree_search.sl",
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
            "duality/two_styles.sl",
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
