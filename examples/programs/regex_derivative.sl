// Regular expressions by derivatives, as a `menu`.
//
// Brzozowski's derivative of a language `L` by a character `c` is what may
// follow `c` in `L`: the words `w` with `cw` in `L`. A word is matched by
// deriving by each of its characters in turn and asking, at the end,
// whether the empty word is left.
//
// So a regular expression is wanted for exactly two things — is it
// nullable, and what is its derivative — and a thing known by what may be
// asked of it is a `menu`:
//
//   nullable   does it match the empty word?
//   derive     what is left of it after this character?
//
// There is no syntax tree below and no `of` on one. Each constructor is
// a `mu` that answers the two questions for its own shape, the textbook
// equations read off as arms:
//
//   D_c(∅) = ∅          D_c(ε) = ∅          D_c(c) = ε, and ∅ for any other
//   D_c(r|s) = D_c(r) | D_c(s)
//   D_c(rs)  = D_c(r)s | D_c(s)  when r is nullable, and D_c(r)s otherwise
//   D_c(r*)  = D_c(r)r*
//
// `star` offers a regex that mentions itself, and that is fine: only a
// demanded arm runs, so `r*` unfolds one derivative at a time — the way a
// `Stream` offers its tail. A regex here is a state of its own matching
// automaton, and `derive` is the transition (see `mealy_machine.sl`).
//
// The other items let a regex be looked at. `show` writes it out, and
// `atom` says whether that reads as one unit, so `star` knows when to add
// parentheses. `void` and `unit` say when it is plainly ∅ or plainly ε,
// which is what the constructors need to keep a derivative from piling up
// `∅r|ε` debris.

menu Regex {
    nullable: Bool,
    derive(c: char): Regex,
    show: String,
    atom: Bool,
    void: Bool,
    unit: Bool,
}

func and(a: Bool, b: Bool) -> Bool {
    of a { True => b, False => False }
}

func or(a: Bool, b: Bool) -> Bool {
    of a { True => True, False => b }
}

// ∅, the language with no words.
func empty() -> Regex {
    mu Regex {
        nullable <= <False | nullable>,
        derive(c): out <= <empty() | out>,
        show <= <"∅" | show>,
        atom <= <True | atom>,
        void <= <True | void>,
        unit <= <False | unit>,
    }
}

// ε, the language of the empty word alone.
func epsilon() -> Regex {
    mu Regex {
        nullable <= <True | nullable>,
        derive(c): out <= <empty() | out>,
        show <= <"ε" | show>,
        atom <= <True | atom>,
        void <= <False | void>,
        unit <= <True | unit>,
    }
}

func chr(wanted: char) -> Regex {
    mu Regex {
        nullable <= <False | nullable>,
        derive(c): out <= <of (<(c, wanted) | eq) {
            True => epsilon(),
            False => empty(),
        } | out>,
        show <= <wanted | to_string | show>,
        atom <= <True | atom>,
        void <= <False | void>,
        unit <= <False | unit>,
    }
}

// `r|s`. ∅ is its identity, so a side that is plainly ∅ is dropped.
func alt(r: Regex, s: Regex) -> Regex {
    mu Regex {
        ret <= {
            of r.void {
                True => <s | ret>,
                False => (,),
            };
            of s.void {
                True => <r | ret>,
                False => (,),
            };
            <mu Regex {
                nullable <= <(r.nullable, s.nullable) | or | nullable>,
                derive(c): out <= <(<c | r.derive, <c | s.derive) | alt | out>,
                show <= <("(", r.show)
                    | add
                    | x => (x, "|") | add
                    | x => (x, s.show) | add
                    | x => (x, ")") | add
                    | show>,
                atom <= <True | atom>,
                void <= <False | void>,
                unit <= <False | unit>,
            } | ret>
        },
    }
}

// `rs`. ∅ annihilates it and ε is its identity.
func seq(r: Regex, s: Regex) -> Regex {
    mu Regex {
        ret <= {
            of (<(r.void, s.void) | or) {
                True => <empty() | ret>,
                False => (,),
            };
            of r.unit {
                True => <s | ret>,
                False => (,),
            };
            of s.unit {
                True => <r | ret>,
                False => (,),
            };
            <mu Regex {
                nullable <= <(r.nullable, s.nullable) | and | nullable>,
                derive(c): out <= {
                    let first = <(<c | r.derive, s) | seq;
                    let rest = of r.nullable {
                        True => <(first, <c | s.derive) | alt,
                        False => first,
                    };
                    <rest | out>
                },
                show <= <(r.show, s.show) | add | show>,
                atom <= <False | atom>,
                void <= <False | void>,
                unit <= <False | unit>,
            } | ret>
        },
    }
}

// `r*`, which offers itself again: the arm that says so runs only on demand.
func star(r: Regex) -> Regex {
    mu Regex {
        nullable <= <True | nullable>,
        derive(c): out <= <(<c | r.derive, <r | star) | seq | out>,
        show <= <(
            of r.atom {
                True => r.show,
                False => <("(", r.show) | add | x => (x, ")") | add,
            },
            "*",
        ) | add | show>,
        atom <= <True | atom>,
        void <= <False | void>,
        unit <= <False | unit>,
    }
}

// Matching: derive by each character, then ask whether ε is left.
func matches_from(r: Regex, word: String, at: i64) -> Bool {
    of (<(at, <word | str_len) | lt) {
        True => <(<(word, at) | index | r.derive, word, <(at, 1) | add) | matches_from,
        False => r.nullable,
    }
}

func matches(r: Regex, word: String) -> Bool {
    <(r, word, 0) | matches_from
}

// The derivation itself, a line per character: what was read, and what is
// left to match.
func derivation(r: Regex, word: String, at: i64) -> (,) / {IO} {
    mu (,) {
        ret <= {
            of (<(at, <word | str_len) | ge) {
                True => <(<("  nullable: ", <r.nullable | to_string) | add) | println | ret>,
                False => (,),
            };
            let c = <(word, at) | index;
            let rest = <c | r.derive;
            <("  ", <c | to_string)
                | add
                | x => (x, "  ") | add
                | x => (x, rest.show) | add
                | println;
            <(rest, word, <(at, 1) | add) | derivation | ret>
        },
    }
}

proc main | (exit: i32) / {IO} {
    let a = <'a' | chr;
    let b = <'b' | chr;

    // (ab)*
    let pairs = <(<(a, b) | seq) | star;
    <pairs.show | println;
    <(pairs, "abab", 0) | derivation;
    <(pairs, "aba", 0) | derivation;

    // (a|b)*abb — the textbook one: every word that ends in `abb`.
    let ends_abb = <(<(<(a, b) | alt) | star, <(a, <(b, b) | seq) | seq) | seq;
    <ends_abb.show | println;
    <(ends_abb, "babb", 0) | derivation;

    <(ends_abb, "aababb") | matches | println;
    <(ends_abb, "abba") | matches | println;
    <(ends_abb, "") | matches | println;
    <(<epsilon() | star, "") | matches | println;

    <0 | exit>
}
