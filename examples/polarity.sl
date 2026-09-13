// Polarity by position.
//
// A type is positive or negative on its own — `+i64` is data, `-i64` consumes
// data — and where it is written says which is expected. There are four
// places to write one, and this program uses all four:
//
//                    argument                 continuation
//   positive type    data arrives             the type after `<-`
//   negative type    a consumer arrives       the row of a `mu`
//
// The one combination a `mu` rejects is in `polarity_error.sl`.
//
// Two of the four are what the position already implies, and everywhere else
// in the corpus they are left off: a value parameter is positive and a
// continuation row is negative. This program writes every sign out, because
// the signs are its subject.

// A request enum: each variant carries the continuation that wants the
// answer. Its dual is codata — a provider that answers one request.
enum Request {
    Retries(-i64),
    Name(-String),
}

// (1) POSITIVE TYPE, ARGUMENT POSITION — `label` is data the function reads.
// (2) NEGATIVE TYPE, ARGUMENT POSITION — `note` is a consumer received as
//     a value: the parameter is `-String`, the caller passes `note`,
//     and the body cuts into it directly.
fn describe(label: +i64, note: -String) -> (;) {
    match (<(label, 0) | gt) { True => {
        <"positive" | note>
    }, _ => {
        <"not positive" | note>
    } }
}

// (3) POSITIVE TYPE, CONTINUATION POSITION — what follows `<-` is the
//     positive type `Request`, and what this produces is its *consumer*: the
//     provider. A continuation is named by the type it consumes, so the type
//     written here is positive while the thing produced is negative.
fn config() <- Request {
    select Request {
        Retries(k) => <3 | k>,
        Name(k) => <"slant" | k>,
    }
}

// (4) NEGATIVE TYPE, CONTINUATION POSITION — the second parameter group is
//     the continuation row: control leaves this command through one of them.
//     `provider` is codata, so it is negative and belongs here too.
command retries | (provider: -Request & answer: -i64) {
    // Consuming codata is the dual of consuming data: the provider is on the
    // consumer side of the cut, and the *positive* request drives it.
    <Request::Retries(answer) | provider>
}

command main | (exit: -i32) / {IO} {
    <mu i64 { answer <= <(,) | retries | (config & answer)> } | println;
    <mu String { note <= <(1, note) | describe } | println;
    <0 | exit>
}
