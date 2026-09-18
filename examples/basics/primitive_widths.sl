// Narrow integer widths and single-precision floats are ordinary primitives.

fn f32_value() -> f32 { 1.25 }
fn i8_value() -> i8 { 7 }
fn u8_value() -> u8 { 9 }
fn add_i8(a: i8, b: i8) -> i8 { <(a, b) | add }
fn add_u8(a: u8, b: u8) -> u8 { <(a, b) | add }
fn add_f32(a: f32, b: f32) -> f32 { <(a, b) | add }

command main | (exit: i32) / {IO} {
    <f32_value() | println;
    <i8_value() | println;
    <u8_value() | println;
    <(1, 2) | add_i8 | println;
    <(3, 4) | add_u8 | println;
    <(1.5, 2.25) | add_f32 | println;
    let exact = match f32_value() { 1.25 => "f32 exact", _ => "wrong" };
    <exact | println;
    let ranged = match f32_value() { 1.0..=1.5 => "f32 range", _ => "wrong" };
    <ranged | println;
    <0 | exit>
}
