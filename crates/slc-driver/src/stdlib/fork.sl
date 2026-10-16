// `fork`: two pure demands, and a rendezvous among the arms of one talk.
//
// `sequential` forces the left thunk, then the right. `parallel` asks the
// runtime to force both at once and resumes with the pair it returns. On the
// empty demand row the pair is the same either way. `talk` starts one spine of
// arms. `exchange` matches `give` with `take`. Each party is an `End` the
// program names: `writer.give(done)` sends there, and `writer.take` receives.
// The interpreter interleaves an exchange. A native exchange stops in the
// runtime, which does not schedule one yet.

cite list::List;
cite lazy::Lazy;

pub hook Link<+T> {
    func mint() -> i64;
    func give_to(token: i64, value: T) -> (,);
    func take_from(token: i64) -> T;
}

pub menu End<+T> / {Link<T>} {
    give(value: T): (,) / {Link<T>},
    take: T,
}

pub func end<+T>() -> End<T> / {Link<T>} {
    let token = mint();
    mu End<T> {
        give(value): done <= <(token, value) | give_to | done>,
        take <= <token | take_from | take>,
    }
}

// A menu is negative, and `List<+T>` holds positive values. The arms of one
// talk are their own spine. The results are positive, so `talk` returns
// `List<A>`.
pub enum Arms<-A> {
    Stop,
    Arm(A, Arms<A>),
}

pub hook Talk<+A, +T> {
    func converse(arms: Arms<Lazy<A, {Link<T>}>>) -> List<A>;
}

pub func talk<+A, +T>(arms: Arms<Lazy<A, {Link<T>}>>) -> List<A> / {Talk<A, T>} {
    <arms | converse
}

pub hand exchange {
    mint(): resume => <(<(,) | __link_mint) | resume,
    converse(arms): resume => <(arms, resume) | __link_run,
    give_to(token, value): resume => <(token, value, resume) | __link_give,
    take_from(token): resume => <(token, resume) | __link_take,
}

pub hook Fork<+A, +B> {
    func together(left: Lazy<A>, right: Lazy<B>) -> (A, B);
}

pub func both<+A, +B>(left: Lazy<A>, right: Lazy<B>) -> (A, B) / {Fork<A, B>} {
    <(left, right) | together
}

pub hand sequential {
    together(left, right): resume => {
        let+ a = left.force;
        let+ b = right.force;
        <(a, b) | resume
    },
}

pub hand parallel {
    together(left, right): resume => <(<(left, right) | __fork_both) | resume,
}
