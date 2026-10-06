#!/bin/sh
# Check the programs the book includes.
# Runnable files must match book/examples/<name>.out.
# A file named *.check.sl is type-checked and not run.
set -eu
cd "$(dirname "$0")/.."
root=$(pwd)

if [ -n "${SLC:-}" ]; then
    slc=$SLC
elif [ -x "$root/target/debug/slc" ]; then
    slc=$root/target/debug/slc
elif [ -x "$root/target/release/slc" ]; then
    slc=$root/target/release/slc
else
    echo "build slc first: cargo build -p slc-driver" >&2
    exit 1
fi

fail=0
if ! "$slc" fmt --check book/examples/*.sl; then
    fail=1
fi

for src in book/examples/*.sl; do
    name=$(basename "$src" .sl)
    case $name in
        *.check)
            if ! "$slc" check "$src"; then
                fail=1
            fi
            ;;
        *)
            out=$(mktemp)
            if ! "$slc" run --interpret "$src" >"$out"; then
                echo "run failed: $src" >&2
                cat "$out" >&2
                fail=1
            elif ! diff -u "book/examples/$name.out" "$out"; then
                echo "output changed: $src" >&2
                fail=1
            fi
            rm -f "$out"
            ;;
    esac
done

exit "$fail"
