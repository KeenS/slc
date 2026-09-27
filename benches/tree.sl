// A perfect tree of height N. The root holds N and each child is a tree of
// height N - 1. The checksum is sum(N) = N + 2 * sum(N - 1), with sum(0) = 0.

def N: i64 = 14;

enum Tree {
    Leaf,
    Node(Tree, i64, Tree),
}

func build(d: i64) -> Tree {
    of (<(d, 0) | eq) {
        True => Leaf,
        _ => {
            let left = <(<(d, 1) | sub) | build;
            let right = <(<(d, 1) | sub) | build;
            Node(left, d, right)
        },
    }
}

func sum(t: Tree) -> i64 {
    of t {
        Leaf => 0,
        Node(left, n, right) => {
            let a = <left | sum;
            let b = <right | sum;
            <(<(a, n) | add, b) | add
        },
    }
}

proc main | (exit: i32) / {IO} {
    <N | build | sum | println;
    <0 | exit>
}
