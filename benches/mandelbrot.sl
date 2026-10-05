// An N by N sample of the Mandelbrot set. Pixel indexes widen to f64, so
// column i is -1.5 + i * 0.0625 and row r is -1.0 + r * 0.0625. Each point
// is iterated N times. The checksum is the number of points that stay inside.

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

func widen(n: i64) -> f64 {
    <n | into
}

func col(i: i64, y: f64) -> i64 {
    of (<(i, N) | ge) {
        True => 0,
        _ => {
            let x = <(-1.5, <(<i | widen, STEP) | mul) | add;
            let bit = of (<(0.0, 0.0, x, y, 0) | mandel) {
                True => 1,
                False => 0,
            };
            <(bit, <(<(i, 1) | add, y) | col) | add
        },
    }
}

func grid(row: i64) -> i64 {
    of (<(row, N) | ge) {
        True => 0,
        _ => {
            let y = <(-1.0, <(<row | widen, STEP) | mul) | add;
            let here = <(0, y) | col;
            <(here, <(<(row, 1) | add) | grid) | add
        },
    }
}

proc main | (exit: i32) / {IO} {
    <0 | grid | println;
    <0 | exit>
}
