// Non-local jump: searching a tree.
//
// The walk is written in continuation style: `done` is the rest of the
// traversal, threaded through every frame. When the target is found, the
// walker does not return `true` up through each level — it cuts the value
// straight to `jump`, and every pending `done` on the way up is simply
// abandoned. The jump lands at the `mu` that captured the continuation
// of the `let`, however deep the recursion was when it fired.
//
// The visit log is the proof: searching for 2 never visits 4, because after
// the hit there is no traversal left to run.

enum Tree {
    Leaf,
    Node(Tree, i64, Tree),
}

command walk(t: Tree, target: i64) | (jump: i64 & done: unit) {
    match t {
        Leaf => (,) | done⟩,
        Node(left, value, right) => {
            "visiting " + (value | int_to_str) | println;
            if value == target {
                // The non-local jump: past this walk's own frames, past
                // every enclosing walk, straight to the captured `k`.
                value | jump⟩
            } else {
                (left, target) | walk | (jump & select unit {
                    finished_left => (right, target) | walk | (jump & done)⟩,
                })⟩
            }
        },
    }
}

command main | (exit: i32) {
    let tree = Tree::Node(
        Tree::Node(Tree::Leaf, 1, Tree::Node(Tree::Leaf, 2, Tree::Leaf)),
        3,
        Tree::Node(Tree::Leaf, 4, Tree::Leaf),
    );

    // Found: the walk stops the moment it hits, and `done` never fires.
    let hit = mu { k <= (tree, 2) | walk | (k & select unit { exhausted => -1 | k⟩ })⟩ };
    "found: " + (hit | int_to_str) | println;

    // Absent: the walk exhausts the tree, and the `done` chain delivers -1.
    let missing = mu { k <= (tree, 99) | walk | (k & select unit { exhausted => -1 | k⟩ })⟩ };
    "missing: " + (missing | int_to_str) | println;

    0 | exit⟩
}
