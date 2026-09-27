// Solutions of the N-queens puzzle. The checksum is the number of
// placements. Eight queens has 92.

cite list::List;
cite list::List::*;

def N: i64 = 8;

func attacks(c: i64, col: i64, delta: i64) -> Bool {
    of (<(c, col) | eq) {
        True => True,
        _ => {
            of (<(<(c, col) | sub, delta) | eq) {
                True => True,
                _ => <(<(col, c) | sub, delta) | eq,
            }
        },
    }
}

func blocked(col: i64, placed: List<i64>, delta: i64) -> Bool {
    of placed {
        Nil => False,
        Cons(c, rest) => {
            of (<(c, col, delta) | attacks) {
                True => True,
                _ => <(col, rest, <(delta, 1) | add) | blocked,
            }
        },
    }
}

func search(row: i64, placed: List<i64>) -> i64 {
    of (<(row, N) | eq) {
        True => 1,
        _ => <(0, row, placed) | try_col,
    }
}

func try_col(col: i64, row: i64, placed: List<i64>) -> i64 {
    of (<(col, N) | ge) {
        True => 0,
        _ => {
            let later = <(<(col, 1) | add, row, placed) | try_col;
            of (<(col, placed, 1) | blocked) {
                True => later,
                _ => {
                    let here = <(<(row, 1) | add, Cons(col, placed)) | search;
                    <(here, later) | add
                },
            }
        },
    }
}

proc main | (exit: i32) / {IO} {
    let placed: List<i64> = Nil;
    <(0, placed) | search | println;
    <0 | exit>
}
