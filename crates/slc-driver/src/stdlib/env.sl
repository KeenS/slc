// `env`: the process environment. Reading a variable offers the value or
// its absence. Setting one offers success or why it was refused. Both
// perform `Env`. `do expr env::real` answers from the process.

pub hook Env {
    func lookup_var(name: String) -> (String | unit);
    func assign_var(name: String, value: String) -> (unit | String);
}

pub proc get<E>(name: String) | (found: (-String / {..E}) & absent: (-unit / {..E})) / {Env, ..E} {
    <name | lookup_var | (found & absent)>
}

pub proc set<E>(name: String, value: String) | (
    ok: (-unit / {..E})
    & failed: (-String / {..E})
) / {Env, ..E} {
    <(name, value) | assign_var | (ok & failed)>
}

pub hand real / {IO} {
    lookup_var(name): resume => <name
        | __env_get
        | (fn(value: String) { ::0(value) } & fn(missing: unit) { ::1(missing) })
        | resume,
    assign_var(name, value): resume => <(name, value)
        | __env_set
        | (fn(done: unit) { ::0(done) } & fn(why: String) { ::1(why) })
        | resume,
}
