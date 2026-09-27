#!/usr/bin/env bash
# Time the programs in benches/ and check the integers in checksums.txt.
# Usage: benches/run.sh [-n repeats] [--check]

set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

repeats=1
check_only=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        -n)
            repeats=${2:-}
            if [[ ! $repeats =~ ^[1-9][0-9]*$ ]]; then
                echo "usage: benches/run.sh [-n repeats] [--check]" >&2
                exit 2
            fi
            shift 2
            ;;
        --check)
            check_only=1
            shift
            ;;
        *)
            echo "usage: benches/run.sh [-n repeats] [--check]" >&2
            exit 2
            ;;
    esac
done

if [[ -n ${SLC:-} ]]; then
    slc=$SLC
elif [[ -x target/release/slc ]]; then
    slc=$root/target/release/slc
elif [[ -x target/debug/slc ]]; then
    slc=$root/target/debug/slc
else
    cargo build --release
    slc=$root/target/release/slc
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

if [[ $check_only -eq 0 ]]; then
    echo "slc: $slc"
    echo "repeats: $repeats (fastest run)"
    printf '%-12s %8s %8s %8s %12s\n' benchmark check run eval checksum
fi

failed=0
total_run=0
count=0

while read -r name expected extra; do
    if [[ -z ${name:-} || $name == \#* ]]; then
        continue
    fi
    if [[ -n ${extra:-} || -z ${expected:-} ]]; then
        echo "bad line in benches/checksums.txt: $name ${expected:-}" >&2
        exit 2
    fi
    file=$root/benches/$name.sl
    if [[ ! -f $file ]]; then
        echo "missing $file" >&2
        exit 2
    fi
    count=$((count + 1))

    if [[ $check_only -eq 0 ]]; then
        start=$(date +%s%N)
        if ! "$slc" check "$file" >"$tmp/check.out" 2>"$tmp/check.err"; then
            echo "$name: check failed" >&2
            cat "$tmp/check.err" >&2
            failed=1
            continue
        fi
        end=$(date +%s%N)
        check_ms=$(((end - start) / 1000000))
    fi

    best=
    got=
    run_status=0
    for ((i = 0; i < repeats; i++)); do
        start=$(date +%s%N)
        if "$slc" run "$file" >"$tmp/run.out" 2>"$tmp/run.err"; then
            run_status=0
        else
            run_status=$?
        fi
        end=$(date +%s%N)
        elapsed=$(((end - start) / 1000000))
        if [[ -z $best || $elapsed -lt $best ]]; then
            best=$elapsed
        fi
        if [[ $run_status -ne 0 ]]; then
            break
        fi
        got=$(tr -d '[:space:]' <"$tmp/run.out")
        if [[ $got != "$expected" ]]; then
            break
        fi
    done

    if [[ $run_status -ne 0 ]]; then
        echo "$name: run failed" >&2
        cat "$tmp/run.err" >&2
        failed=1
        continue
    fi
    if [[ $got != "$expected" ]]; then
        echo "$name: expected $expected, got ${got:-<empty>}" >&2
        failed=1
        continue
    fi

    if [[ $check_only -eq 1 ]]; then
        echo "$name ok"
        continue
    fi

    eval_ms=$((best - check_ms))
    if [[ $eval_ms -lt 0 ]]; then
        eval_ms=0
    fi
    total_run=$((total_run + best))
    printf '%-12s %8d %8d %8d %12s\n' "$name" "$check_ms" "$best" "$eval_ms" "$expected"
done <"$root/benches/checksums.txt"

if [[ $count -eq 0 ]]; then
    echo "benches/checksums.txt has no programs" >&2
    exit 2
fi

if [[ $check_only -eq 0 && $failed -eq 0 ]]; then
    printf '%-12s %8s %8d %8s %12s\n' total "" "$total_run" "" ""
fi

exit "$failed"
