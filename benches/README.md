# Benchmarks

Each program prints one integer. The integer is the checksum of a fixed
workload, and the workload's size is `def N` in that file.
[`checksums.txt`](checksums.txt) records the integer each program must
print. Changing `N` changes the integer.

[`run.sh`](run.sh) runs the suite. It prefers `target/release/slc`, then
`target/debug/slc`. `SLC` names a binary directly.

```sh
cargo build --release
benches/run.sh
benches/run.sh -n 5
benches/run.sh --check
```

`slc run` checks a program and then evaluates it. The table reports both
times in milliseconds, and evaluation as the run minus the check. `-n`
runs each program that many times and keeps the fastest run. `--check`
compares the checksums and prints no times.

`cargo test -p slc-driver --test benches` compares the same integers.
The later programs are classic workloads.

| Program | What the time is spent on |
|---|---|
| [`fib.sl`](fib.sl) | Naive double recursion |
| [`loop.sl`](loop.sl) | A tail call that adds `1..=N` |
| [`match.sl`](match.sl) | A variant match on every step |
| [`list.sl`](list.sl) | Building a list, mapping, and summing |
| [`tree.sl`](tree.sl) | Building and summing a perfect tree |
| [`stream.sl`](stream.sl) | Forcing the first `N` elements of a count |
| [`map.sl`](map.sl) | Insert and lookup on the ordered map |
| [`array.sl`](array.sl) | Push and index on the array |
| [`hashmap.sl`](hashmap.sl) | Insert and lookup on the hash map |
| [`string.sl`](string.sl) | Appending characters and reading them back |
| [`handler.sl`](handler.sl) | Performing an operation and resuming |
| [`ackermann.sl`](ackermann.sl) | Ackermann `A(3, N)` |
| [`tak.sl`](tak.sl) | Takeuchi's function `tak(3N, 2N, N)` |
| [`hanoi.sl`](hanoi.sl) | Towers of Hanoi, visiting every move |
| [`queens.sl`](queens.sl) | Solutions of the N-queens puzzle |
| [`fannkuch.sl`](fannkuch.sl) | Pancake flips over every permutation |
| [`sieve.sl`](sieve.sl) | The sieve of Eratosthenes |
| [`quicksort.sl`](quicksort.sl) | Sorting a fixed sequence |
| [`matmul.sl`](matmul.sl) | The product of an integer matrix with itself |
| [`mandelbrot.sl`](mandelbrot.sl) | Points that stay in the Mandelbrot set |
| [`collatz.sl`](collatz.sl) | Hailstone steps for `1..=N` |
