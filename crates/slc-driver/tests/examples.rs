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
        ("bottom_type.sl", Expected { success: true, stdout: &["()"], stderr: &[] }),
        (
            "comparison.sl",
            Expected {
                success: true,
                stdout: &["true", "true", "true", "true", "true", "true"],
                stderr: &[],
            },
        ),
        (
            "file_io.sl",
            Expected {
                success: true,
                stdout: &[
                    "The simplest Slant program",
                    "Hello, Slant!",
                    "cannot read: cannot read examples/missing.sl",
                ],
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
            "linearity_error.sl",
            Expected {
                success: false,
                stdout: &[],
                stderr: &["linearity:", "continuation `k` is never used"],
            },
        ),
        ("match_exhaustive.sl", Expected { success: true, stdout: &["red"], stderr: &[] }),
        ("mu.sl", Expected { success: true, stdout: &["42"], stderr: &[] }),
        ("mu_escape.sl", Expected { success: true, stdout: &["42"], stderr: &[] }),
        (
            "mu_tilde.sl",
            Expected {
                success: true,
                stdout: &["42", "42", "the value was consumed", "17", "100"],
                stderr: &[],
            },
        ),
        ("nested_calls.sl", Expected { success: true, stdout: &["\"320\""], stderr: &[] }),
        ("pair.sl", Expected { success: true, stdout: &["30"], stderr: &[] }),
        ("polarity.sl", Expected { success: true, stdout: &["3", "positive"], stderr: &[] }),
        (
            "polarity_error.sl",
            Expected {
                success: false,
                stdout: &[],
                stderr: &[
                    "polarity:",
                    "parameter `x` has explicitly negative type",
                    "parameter `j` has explicitly positive type",
                ],
            },
        ),
        ("select.sl", Expected { success: true, stdout: &["1", "42", "100"], stderr: &[] }),
        (
            "strings.sl",
            Expected { success: true, stdout: &["Hello, world!", "H", "world"], stderr: &[] },
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
