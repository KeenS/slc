// Checked, and left unrun: it would read a file from the disk.

proc main | (exit: i32) / {IO} {
    do {
        <"notes.txt" | fs::read | (
            mu String {
                text => {
                    <text | println;
                    <0 | exit>
                },
            }
            & mu String {
                why => {
                    <why | println;
                    <1 | exit>
                },
            }
        )>
    } fs::real
}
