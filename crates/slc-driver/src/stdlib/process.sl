// `process`: start another program and wait for it. The success is the exit
// status: the code the process exited with, or 128 plus the signal that
// killed it. A process that cannot be started is the failure, and a
// non-zero exit is not that failure. The child inherits the environment,
// the standard streams, and nothing else is captured. Spawning performs
// `Process`. `do expr process::real` answers from the runtime.

cite list::List;
cite list::List::*;

pub hook Process {
    func spawn_process(program: String, args: List<String>) -> (i64 | String);
}

pub proc spawn<E>(program: String, args: List<String>) | (
    exited: (-i64 / {..E})
    & failed: (-String / {..E})
) / {Process, ..E} {
    <(program, args) | spawn_process | (exited & failed)>
}

func push_args(xs: List<String>) -> (,) / {IO} {
    of xs {
        Nil => (,),
        Cons(arg, rest) => {
            <arg | __spawn_push;
            <rest | push_args
        },
    }
}

func launch(program: String, args: List<String>) -> (i64 | String) / {IO} {
    <program | __spawn_begin;
    <args | push_args;
    <(,) | __spawn_exec | (fn(status: i64) { ::0(status) } & fn(why: String) { ::1(why) })
}

pub hand real / {IO} {
    spawn_process(program, args): resume => <(program, args) | launch | resume,
}
