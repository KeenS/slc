// Floating-point literals are f64 values with arithmetic, comparison and Display.

proc main | (exit: i32) / {IO} {
    <1.5 | println;
    <(1.5, 2.25) | add | println;
    <(5.5, 2.0) | div | println;
    <1.5 | neg | println;
    <(1.5, 2.5) | lt | println;
    let value = of 2.5 { 1.5..=2.0 => "wrong", 2.5 => "matched", _ => "wrong" };
    <value | println;
    let ranged = of 2.25 { 1.5..=2.5 => "ranged", _ => "wrong" };
    <ranged | println;
    <0 | exit>
}
