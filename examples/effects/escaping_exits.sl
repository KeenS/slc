effect Tick {
    fn tick() -> (,);
}

data Saved<E> {
    consumer: (-i64 / {..E}),
}

command save<E> | (consumer: (-i64 / {..E}) & returned: Saved<..E>) {
    <Saved { consumer: consumer } | returned>
}

command main | (exit: -i32) / {IO} {
    let+ saved = handle (mu Saved<{Tick, IO}> {
        returned <= <(,) | save | (
            select i64 {
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
    handle (<42 | saved.consumer>) {
        tick(): resume => {
            <"tick on activation" | println;
            <(,) | resume
        },
    }
}
