// The product of an N by N matrix with itself. Entry (i, j) is i * N + j,
// with indices from 0. The checksum is the sum of the product.

cite list::List;
cite list::List::*;

def N: i64 = 28;

func row(j: i64, i: i64, n: i64) -> List<i64> {
    of (<(j, n) | ge) {
        True => Nil,
        _ => Cons(<(<(i, n) | mul, j) | add, <(<(j, 1) | add, i, n) | row),
    }
}

func matrix(i: i64, n: i64) -> List<List<i64>> {
    of (<(i, n) | ge) {
        True => Nil,
        _ => Cons(<(0, i, n) | row, <(<(i, 1) | add, n) | matrix),
    }
}

func head_row(row: List<i64>) -> i64 {
    of row {
        Cons(h, _) => h,
        Nil => 0,
    }
}

func tail_row(row: List<i64>) -> List<i64> {
    of row {
        Cons(_, t) => t,
        Nil => Nil,
    }
}

func heads(rows: List<List<i64>>) -> List<i64> {
    of rows {
        Nil => Nil,
        Cons(row, rest) => Cons(<row | head_row, <rest | heads),
    }
}

func tails(rows: List<List<i64>>) -> List<List<i64>> {
    of rows {
        Nil => Nil,
        Cons(row, rest) => Cons(<row | tail_row, <rest | tails),
    }
}

func pending(rows: List<List<i64>>) -> Bool {
    of rows {
        Nil => False,
        Cons(row, _) => {
            of row {
                Nil => False,
                Cons(_, _) => True,
            }
        },
    }
}

func transpose(rows: List<List<i64>>) -> List<List<i64>> {
    of (<rows | pending) {
        False => Nil,
        True => Cons(<rows | heads, <(<rows | tails) | transpose),
    }
}

func dot(a: List<i64>, b: List<i64>, acc: i64) -> i64 {
    of a {
        Nil => acc,
        Cons(x, as) => {
            of b {
                Nil => acc,
                Cons(y, bs) => <(as, bs, <(acc, <(x, y) | mul) | add) | dot,
            }
        },
    }
}

func against(row: List<i64>, cols: List<List<i64>>) -> List<i64> {
    of cols {
        Nil => Nil,
        Cons(col, rest) => Cons(<(row, col, 0) | dot, <(row, rest) | against),
    }
}

func multiply(rows: List<List<i64>>, cols: List<List<i64>>) -> List<List<i64>> {
    of rows {
        Nil => Nil,
        Cons(row, rest) => Cons(<(row, cols) | against, <(rest, cols) | multiply),
    }
}

func sum_row(xs: List<i64>, acc: i64) -> i64 {
    of xs {
        Nil => acc,
        Cons(h, rest) => <(rest, <(acc, h) | add) | sum_row,
    }
}

func sum_matrix(rows: List<List<i64>>, acc: i64) -> i64 {
    of rows {
        Nil => acc,
        Cons(row, rest) => <(rest, <(row, acc) | sum_row) | sum_matrix,
    }
}

proc main | (exit: i32) / {IO} {
    let grid = <(0, N) | matrix;
    <(<(grid, <grid | transpose) | multiply, 0) | sum_matrix | println;
    <0 | exit>
}
