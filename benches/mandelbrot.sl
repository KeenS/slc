// An N by N sample of the Mandelbrot set. Sampling starts at (-1.5, -1)
// and steps by 0.0625, and each point is iterated N times. The checksum
// is the number of points that stay inside.

def N: i64 = 24;

def STEP: f64 = 0.0625;

func mandel(zr: f64, zi: f64, cr: f64, ci: f64, i: i64) -> Bool {
    of (<(i, N) | ge) {
        True => True,
        _ => {
            let zr2 = <(zr, zr) | mul;
            let zi2 = <(zi, zi) | mul;
            of (<(<(zr2, zi2) | add, 4.0) | gt) {
                True => False,
                _ => {
                    let next_r = <(<(zr2, zi2) | sub, cr) | add;
                    let next_i = <(<(<(zr, zi) | mul, 2.0) | mul, ci) | add;
                    <(next_r, next_i, cr, ci, <(i, 1) | add) | mandel
                },
            }
        },
    }
}

func col(x: f64, left: i64, y: f64) -> i64 {
    of (<(left, 0) | eq) {
        True => 0,
        _ => {
            let bit = of (<(0.0, 0.0, x, y, 0) | mandel) {
                True => 1,
                False => 0,
            };
            let rest = <(<(x, STEP) | add, <(left, 1) | sub, y) | col;
            <(bit, rest) | add
        },
    }
}

func grid(y: f64, rows: i64) -> i64 {
    of (<(rows, 0) | eq) {
        True => 0,
        _ => {
            let here = <(-1.5, N, y) | col;
            let rest = <(<(y, STEP) | add, <(rows, 1) | sub) | grid;
            <(here, rest) | add
        },
    }
}

proc main | (exit: i32) / {IO} {
    <(-1.0, N) | grid | println;
    <0 | exit>
}
