// let- keeps a computation and runs it at each demand. let+ runs it now.

hook Build { func build() -> i64; }

func make() -> i64 / {Build} { build() }

proc main | (exit: i32) / {IO} {
    let- pending = fn { make() };
    <"stored" | println;
    let first = do (<(,) | pending) hn { build(): resume => <10 | resume };
    let second = do (<(,) | pending) hn { build(): resume => <20 | resume };
    <first | println;
    <second | println;
    let+ eager = do make() hn { build(): resume => <3 | resume };
    <eager | println;
    <0 | exit>
}
