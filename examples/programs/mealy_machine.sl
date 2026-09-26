// A Mealy machine, as a `menu`.
//
// A Mealy machine reads an input, answers an output, and moves to a next
// state — and the output depends on both the state and the input. Written
// as data, that is a transition table and a state to look up in it. Written
// as codata, it is a menu:
//
//   the input alphabet is the menu     one item per input symbol
//   feeding an input is a demand       `machine.coin`
//   an answer is a `Step`              the output, and the machine to ask next
//   a state is a `mu`                  one arm per transition out of it
//
// So there is no state *value* anywhere below, and no table. A state is the
// machine you are holding, and its transitions are its arms. Only the
// demanded arm runs, so a machine with a cycle — every machine — is just a
// menu that offers another menu, the way a `Stream` offers its tail.
//
// The machine here is a turnstile: a coin unlocks it, a push turns it and
// locks it again.
//
//               coin / Open                      push / Turn
//   locked  ─────────────────▶  unlocked  ─────────────────▶  locked
//   push / Alarm, and stay      coin / Refund, and stay

cite list::List;
cite list::List::*;

enum Input { Coin, Push }

enum Output { Open, Turn, Refund, Alarm, Wait }

impl Display for Output {
    func fmt(self: Output) -> String {
        of self {
            Open => "open",
            Turn => "turn",
            Refund => "refund",
            Alarm => "alarm",
            Wait => "wait",
        }
    }
}

// What a transition answers: the output on the edge, and the state the edge
// leads to.
data Step { output: Output, next: Turnstile }

// The machine: its input alphabet, item by item.
menu Turnstile {
    coin: Step,
    push: Step,
}

// One function per state, and one arm per edge out of it. The arm's name is
// the input, and what it cuts into is the edge: `coin / Open`, to `unlocked`.
func locked() -> Turnstile {
    mu Turnstile {
        coin <= <Step { output: Open, next: unlocked() } | coin>,
        push <= <Step { output: Alarm, next: locked() } | push>,
    }
}

func unlocked() -> Turnstile {
    mu Turnstile {
        coin <= <Step { output: Refund, next: unlocked() } | coin>,
        push <= <Step { output: Turn, next: locked() } | push>,
    }
}

// States need not be finitely many functions: a state can carry what it
// remembers as parameters. This gate wants `fare` coins before it opens, and
// its state is the credit so far — one definition, a family of states.
func gate(credit: i64, fare: i64) -> Turnstile {
    let paid = <(credit, fare) | ge;
    mu Turnstile {
        coin <= of paid {
            True => <Step { output: Refund, next: <(credit, fare) | gate } | coin>,
            False => {
                let credit = <(credit, 1) | add;
                let output = of (<(credit, fare) | ge) { True => Open, False => Wait };
                <Step { output: output, next: <(credit, fare) | gate } | coin>
            },
        },
        push <= of paid {
            True => <Step { output: Turn, next: <(0, fare) | gate } | push>,
            False => <Step { output: Alarm, next: <(credit, fare) | gate } | push>,
        },
    }
}

// Running a machine turns the data it is fed into demands: each input picks
// the item to ask for. This is the one place the two alphabets meet — `Input`
// is the alphabet as data, `Turnstile` the same alphabet as codata.
func feed(machine: Turnstile, input: Input) -> Step {
    of input {
        Coin => machine.coin,
        Push => machine.push,
    }
}

func run(machine: Turnstile, inputs: List<Input>) -> List<Output> {
    of inputs {
        Nil => Nil,
        Cons(input, rest) => {
            let step = <(machine, input) | feed;
            Cons(step.output, <(step.next, rest) | run)
        },
    }
}

proc main | (exit: i32) / {IO} {
    // A machine is driven by projection alone: each `.push` or `.coin` is a
    // transition, and `.next` the machine that remains.
    <locked().push.output | println;
    <locked().coin.next.push.output | println;

    let inputs = Cons(Push, Cons(Coin, Cons(Coin, Cons(Push, Cons(Push, Nil)))));
    <(locked(), inputs) | run | println;

    // The same alphabet, a different machine: two coins a turn.
    <(<(0, 2) | gate, inputs) | run | println;

    <0 | exit>
}
