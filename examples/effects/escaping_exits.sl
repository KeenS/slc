hook Tick {
    func tick() -> (,);
}

data Saved<E> {
    consumer: (-i64 / {..E}),
}

proc save<E> | (consumer: (-i64 / {..E}) & returned: Saved<..E>) {
    <Saved { consumer: consumer } | returned>
}

proc main | (exit: -i32) / {IO} {
    let+ saved = do (mu Saved<{Tick, IO}> {
        returned <= <(,) | save | (
            mu i64 {
                value => {
                    tick();
                    <value | println;
                    <0 | exit>
                },
            }
            & returned
        )>,
    }) {
        tick(): resume => {
            <"unexpected construction demand" | println;
            <(,) | resume
        },
    };

    <"stored without running" | println;
    do (<42 | saved.consumer>) {
        tick(): resume => {
            <"tick on activation" | println;
            <(,) | resume
        },
    }
}
