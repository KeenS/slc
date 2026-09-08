// mu captures the current continuation; activating it escapes with a value.

fn main() -> i32 {
    mu(k: -i32) {
        k(42)
    }
}
