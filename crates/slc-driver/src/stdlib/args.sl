// `args`: the words the program was given. `slc run file.sl one two`
// answers `[one, two]` — the words after the file, in order. The runtime
// binary, the file path, `--fuel`, and `--interpret` are not among them.
// Reading them performs `Args`. `do expr args::real` answers from the
// process, through `__argument_count` and `__argument_at`.

cite list::List;
cite list::List::*;

pub hook Args {
    func program_arguments() -> List<String>;
}

pub func arguments() -> List<String> / {Args} {
    program_arguments()
}

func gather(i: i64, n: i64) -> List<String> / {IO} {
    of (<(i, n) | ge) {
        True => Nil,
        _ => Cons(<i | __argument_at, <(<(i, 1) | add, n) | gather),
    }
}

pub hand real / {IO} {
    program_arguments(): resume => <(0, <(,) | __argument_count) | gather | resume,
}
