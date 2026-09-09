# Slant Surface Syntax Ergonomics Plan

This plan addresses the gap between Slant’s symmetric core and its current
surface syntax. The language already has `fn`, `mu`, `command`, `match`, and
polarity types, but the examples—especially the JSON parser—still expose too
much implementation detail. Numeric character codes, nested `else` chains,
and builtin calls like `add(...)` and `eq(...)` make programs unnecessarily
ugly.

The goal is to make common Slant code look like Rust while preserving the
λ̄μμ̃-style core. Everything in this plan is either sugar over existing
constructs or a missing basic type that is already representable at runtime.

## Non-goals

- Do not add a `Result`-first error model.
- Do not add concurrency primitives.
- Do not replace the symmetric core.
- Do not introduce unrestricted copying of linear continuations.
- Do not make syntax sugar semantically ambiguous with continuation use.

---

## Phase 1: Control-flow sugar

### 1.1 `else if`

**Goal:** allow flat branching instead of deeply nested `else` blocks.

Syntax:

```sl
if is_digit(c) {
    parse_number(input, pos, ok, err)
} else if c == '-' {
    parse_number(input, pos, ok, err)
} else if c == '"' {
    parse_string(input, pos, ok, err)
} else {
    err("expected JSON value")
}
```

Semantics:

```text
if a { A } else if b { B } else { C }
```

desugars to:

```text
if a { A } else { if b { B } else { C } }
```

Implementation checklist:

- [ ] Update parser to accept `else if`
- [ ] Represent it as nested `Expr::If`; no new AST node is required
- [ ] Add parser unit test for a three-branch chain
- [ ] Add parser unit test for a chain without a final `else`
- [ ] Add integration test that evaluates an `else if` chain
- [ ] Add checker test confirming each branch is still checked
- [ ] Add lowering test confirming it lowers to the same term as nested `if`
- [ ] Rewrite JSON parser’s `parse_value` to use `else if`
- [ ] Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and
      `cargo test --workspace`

### 1.2 Boolean operators

**Goal:** support ordinary boolean expressions.

Syntax:

```sl
if is_digit(c) && c != '0' { ... }
if !is_digit(c) || at_end { ... }
```

Semantics:

- `&&` and `||` must be short-circuiting.
- `!` negates a boolean.
- These should desugar to `if`, not eager builtin calls.

Examples:

```sl
a && b
```

desugars conceptually to:

```sl
if a { b } else { false }
```

```sl
a || b
```

desugars conceptually to:

```sl
if a { true } else { b }
```

Implementation checklist:

- [ ] Add `&&`, `||`, and `!` to the lexer
- [ ] Add AST operators:
  - [ ] `BinOp::And`
  - [ ] `BinOp::Or`
  - [ ] `UnOp::Not`
- [ ] Add an `UnOp` field to the surface AST if unary operators are not
      already represented
- [ ] Parse `&&` and `||` with lower precedence than comparisons
- [ ] Parse `!` as a unary operator
- [ ] Type-check that both sides are `+bool`
- [ ] Lower short-circuiting using existing `if`
- [ ] Add tests for short-circuit evaluation
- [ ] Add tests for boolean type mismatches
- [ ] Rewrite parser code using chained comparisons and boolean operators

---

## Phase 2: Real character type

The runtime already has `Value::Char`, but the type system does not. The JSON
parser currently uses `i64` character codes such as `34`, `45`, `91`, `123`,
and `125`. This is the largest readability problem in the examples.

### 2.1 Core `char` type

Add `Base::Char` and make `+char` a first-class positive atom.

Syntax:

```sl
let c: +char = '"';
```

Implementation checklist:

- [ ] Add `Base::Char` to `slc-core::types::Base`
- [ ] Update the type pretty-printer
- [ ] Update dual/polarity behavior if needed
- [ ] Add property tests for `dual(+char) == -char`
- [ ] Change `Value::Char::type_of()` to return `Type::Pos(Base::Char)`
- [ ] Add `char` to surface type lowering
- [ ] Add `char` to inference support
- [ ] Add type tests for positive and negative char types
- [ ] Add runtime tests for char values

### 2.2 Character literals

The lexer already recognizes simple char literals, but they need escape
support and correct lowering.

Required syntax:

```sl
'"'
'\\'
'\''
'\n'
'\r'
'\t'
'\0'
'x'
```

Implementation checklist:

- [ ] Rewrite the char lexer to use a shared escape-decoding routine
- [ ] Support escapes in both strings and chars
- [ ] Reject unterminated character literals with a useful span
- [ ] Reject empty and multi-character char literals
- [ ] Lower `Expr::Char(c)` to a char value rather than a pseudo-variable
- [ ] Add parser tests for every escape form
- [ ] Add runtime test for `'\n'` and `'\\'`

### 2.3 Character operations

Current operations:

```sl
let code = char_at(input, pos); // +i64
```

Desired operations:

```sl
let c = input[pos]; // +char
```

Implementation checklist:

- [ ] Add `Base::Char` support to equality and comparison builtins
- [ ] Add `char_eq`, `char_lt`, `char_le`, `char_gt`, and `char_ge`, or make
      `eq`, `lt`, `le`, `gt`, and `ge` accept chars
- [ ] Add `char_to_code(c) -> i64`
- [ ] Add `code_to_char(i64) -> +char`
- [ ] Add `is_digit(c: +char) -> +bool`
- [ ] Add `is_ws(c: +char) -> +bool`
- [ ] Add `string_push(s: +String, c: +char) -> +String`
- [ ] Decide whether `char_at` remains code-based for compatibility
- [ ] Add builtin tests for all new operations
- [ ] Rewrite JSON parser to use `+char` instead of integer codes

---

## Phase 3: Binary operators

The parser already parses ordinary binary operators, but lowering currently
rejects them. This makes arithmetic examples use builtin calls.

Current ugly code:

```sl
add(pos, 1)
eq(c, 45)
lt(start, end)
```

Desired code:

```sl
pos + 1
c == '-'
start < end
```

### 3.1 Numeric operators

Syntax:

```sl
a + b
a - b
a * b
a / b
a % b
-a
```

Implementation checklist:

- [ ] Lower `+` to `add`
- [ ] Lower `-` to `sub`
- [ ] Lower `*` to `mul`
- [ ] Lower `/` to `div`
- [ ] Lower `%` to `rem`
- [ ] Lower unary `-` to `neg`
- [ ] Type-check operands as matching numeric types
- [ ] Preserve arithmetic overflow and division-by-zero diagnostics
- [ ] Add precedence tests
- [ ] Add associativity tests
- [ ] Add integration tests for each operator
- [ ] Rewrite arithmetic and nested-call examples with operators

### 3.2 Comparison operators

Syntax:

```sl
a == b
a != b
a < b
a > b
a <= b
a >= b
```

Implementation checklist:

- [ ] Lower `==` to `eq`
- [ ] Lower `!=` to `ne`
- [ ] Lower `<` to `lt`
- [ ] Lower `>` to `gt`
- [ ] Lower `<=` to `le`
- [ ] Lower `>=` to `ge`
- [ ] Type-check comparison operands as compatible numeric or char values
- [ ] Add tests for numeric comparisons
- [ ] Add tests for char comparisons
- [ ] Add tests for type mismatch diagnostics
- [ ] Rewrite comparison and JSON examples with operators

### 3.3 String operators

Syntax:

```sl
"Hello, " + "world"
```

Implementation checklist:

- [ ] Lower `+` on strings to `str_concat`
- [ ] Add inference or bidirectional checking to distinguish numeric and
      string `+`
- [ ] Add a type diagnostic when operands do not agree
- [ ] Add integration test for string concatenation
- [ ] Rewrite string examples with `+`

---

## Phase 4: Indexing and slicing

Desired syntax:

```sl
let c = input[pos];
let text = input[start..end];
```

Equivalent primitive calls:

```sl
char_at(input, pos)
substring(input, start, end)
```

### 4.1 Indexing

Implementation checklist:

- [ ] Add postfix `[...]` parsing
- [ ] Define `a[i]` as sugar for an indexing operation
- [ ] Add `String` indexing returning `+char`
- [ ] Add `List` indexing returning the element type
- [ ] Add out-of-range diagnostics with source spans
- [ ] Add checker tests for index types
- [ ] Add runtime tests for valid and invalid indexes

### 4.2 Slicing

Syntax:

```sl
input[start..end]
input[start..]
input[..end]
```

Implementation checklist:

- [ ] Add range expression AST:
  - [ ] `Range`
  - [ ] `RangeFrom`
  - [ ] `RangeTo`
- [ ] Parse `..` in postfix slicing position
- [ ] Optionally parse `..=` for inclusive ranges
- [ ] Lower string slicing to `substring`
- [ ] Define behavior for empty slices
- [ ] Add bounds checking
- [ ] Add checker tests for range endpoints
- [ ] Add runtime tests for every range form

---

## Phase 5: Pattern-matching improvements

`match` is intended to be the main control structure, but the current pattern
language is too weak for parsers and ordinary data manipulation.

### 5.1 Char patterns

Syntax:

```sl
match c {
    '"' => ...,
    '-' => ...,
    _ => ...,
}
```

Implementation checklist:

- [ ] Add `Pattern::Char(char)`
- [ ] Parse char literals in patterns
- [ ] Check char patterns against `+char`
- [ ] Add runtime matching support
- [ ] Add tests for char literal patterns and wildcards

### 5.2 Or-patterns

Syntax:

```sl
match c {
    'e' | 'E' => ...,
    '0'..='9' | '-' => ...,
    _ => ...,
}
```

Implementation checklist:

- [ ] Add `Pattern::Or(Vec<Pattern>)`
- [ ] Parse `|` between patterns
- [ ] Check all alternatives have compatible types
- [ ] Add runtime matching support
- [ ] Add tests for multiple alternatives
- [ ] Ensure or-patterns interact correctly with exhaustiveness checking

### 5.3 Range patterns

Syntax:

```sl
match c {
    '0'..='9' => ...,
    'a'..='f' => ...,
    'A'..='F' => ...,
    _ => ...,
}
```

Implementation checklist:

- [ ] Add `Pattern::Range(Box<Pattern>, Box<Pattern>)`
- [ ] Parse `..=` ranges in patterns
- [ ] Support char ranges first
- [ ] Add integer ranges after chars are supported
- [ ] Check endpoint types
- [ ] Add runtime inclusive-range matching
- [ ] Add tests for boundaries and invalid ranges
- [ ] Update exhaustiveness checking to understand ranges

### 5.4 Guards

Syntax:

```sl
match c {
    c if c < ' ' => err("raw control character"),
    _ => parse_string_tail(...),
}
```

Implementation checklist:

- [ ] Extend `MatchArm` with an optional guard expression
- [ ] Parse `if` after a pattern and before `=>`
- [ ] Type-check guards as `+bool`
- [ ] Implement runtime guard evaluation
- [ ] Define exhaustiveness rules:
  - [ ] wildcard plus guard is not considered unconditionally exhaustive
  - [ ] wildcard without a guard remains exhaustive
- [ ] Add tests for matching with and without guards
- [ ] Add diagnostic tests for non-bool guards

### 5.5 Other useful pattern forms

Implementation checklist:

- [ ] Add `Pattern::Char`
- [ ] Add `Pattern::Or`
- [ ] Add `Pattern::Range`
- [ ] Add optional binding with `@`:
  - [ ] `c @ '0'..='9'`
  - [ ] `x @ Some(_)`
- [ ] Add rest patterns for lists:
  - [ ] `[first, ..rest]`
- [ ] Add struct field shorthand:
  - [ ] `Point { x, y }`
- [ ] Add tuple patterns with nested destructuring
- [ ] Add negative integer patterns
- [ ] Add float patterns only if floats become first-class

---

## Phase 6: Constants and declarations

### 6.1 `const`

Syntax:

```sl
const COMMA: +char = ',';
const OPEN_BRACKET: +char = '[';
const CLOSE_BRACKET: +char = ']';
```

This removes magic numbers and character-code aliases from parsers.

Implementation checklist:

- [ ] Add `TokenKind::Const`
- [ ] Add `Decl::Const` to the surface AST
- [ ] Parse `const NAME: Type = expression;`
- [ ] Reject non-constant initializers in v0.1
- [ ] Lower constants to global bindings
- [ ] Check constant types
- [ ] Add tests for char, integer, bool, and string constants
- [ ] Add diagnostics for mutable or non-constant initializers
- [ ] Rewrite JSON parser using named character constants

### 6.2 Local constants

Syntax:

```sl
let OPEN_BRACKET: +char = '[';
```

Implementation checklist:

- [ ] Add optional type annotation to `let`
- [ ] Check the initializer against the annotation
- [ ] Add parser tests
- [ ] Add checker tests
- [ ] Add examples using annotated local bindings

---

## Phase 7: Continuation error-handling sugar

This phase builds on the design in `DESIGN.md` rather than replacing it.

### 7.1 Single-error `?`

Syntax:

```sl
let bytes = read_file(path)?;
```

Conceptual desugaring:

```sl
read_file(path, to current_success, current_error)
```

Implementation checklist:

- [ ] Define the elaborated form of `e?`
- [ ] Infer current success and error continuations in `fn` and `command`
      bodies
- [ ] Lower `?` to continuation application
- [ ] Reject `?` where there is no current error continuation
- [ ] Add checker diagnostics:
  - [ ] missing error continuation
  - [ ] incompatible error type
- [ ] Add tests for successful propagation
- [ ] Add tests for error propagation
- [ ] Add tests for using `?` in nested `fn` without inherited errors

### 7.2 Multiple error continuations

Potential syntax:

```sl
read_file(path)?missing;
parse(bytes)?invalid;
```

Alternative syntax to investigate:

```sl
read_file(path)?[ok, missing];
```

Implementation checklist:

- [ ] Decide between suffix-name and bracket syntax
- [ ] Add a surface form for selecting an error continuation
- [ ] Elaborate to the selected continuation
- [ ] Ensure unselected continuations remain linear
- [ ] Add checker tests for dangling continuations
- [ ] Add runtime tests for each error path
- [ ] Rewrite JSON parser error handling using selected continuations

### 7.3 Command-call continuation sugar

Desired symmetry:

```sl
read_file(path, to ok, error)
```

or:

```sl
read_file(path, ok, error)
```

Implementation checklist:

- [ ] Decide whether `to` is required
- [ ] Support explicit continuation arguments without helper builtins
- [ ] Add partial-continuation application syntax consistently
- [ ] Preserve linearity checking for each continuation
- [ ] Add tests for commands with multiple continuations
- [ ] Add tests for partially applied commands

---

## Phase 8: Example cleanup

Once the preceding features exist, rewrite the examples to demonstrate the
intended language rather than runtime limitations.

### 8.1 Rewrite arithmetic example

Before:

```sl
println(add(2, 3));
```

After:

```sl
println(2 + 3);
```

Checklist:

- [ ] Use arithmetic operators
- [ ] Use unary minus where appropriate
- [ ] Show overflow and division diagnostics
- [ ] Keep the example concise

### 8.2 Rewrite comparison example

Before:

```sl
println(eq(1, 1));
```

After:

```sl
println(1 == 1);
```

Checklist:

- [ ] Use comparison operators
- [ ] Add char comparison
- [ ] Add boolean operators

### 8.3 Rewrite string example

Before:

```sl
str_concat("Hello, ", "world!")
```

After:

```sl
"Hello, " + "world!"
```

Checklist:

- [ ] Use string concatenation
- [ ] Use indexing and slicing where useful
- [ ] Show char values

### 8.4 Rewrite JSON parser

Desired shape:

```sl
const COMMA: +char = ',';
const COLON: +char = ':';
const OPEN_BRACKET: +char = '[';
const CLOSE_BRACKET: +char = ']';
const OPEN_BRACE: +char = '{';
const CLOSE_BRACE: +char = '}';

fn parse_value(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    match input[pos] {
        '0'..='9' | '-' => parse_number(input, pos, ok, err),
        '"' => parse_string(input, pos, ok, err),
        OPEN_BRACKET => parse_array(input, pos, ok, err),
        OPEN_BRACE => parse_object(input, pos, ok, err),
        't' => parse_literal(input, pos, "true", ok, err),
        'f' => parse_literal(input, pos, "false", ok, err),
        'n' => parse_literal(input, pos, "null", ok, err),
        _ => err("expected JSON value"),
    }
}
```

Checklist:

- [ ] Replace all character codes with `+char`
- [ ] Replace `else` nesting with `else if` or `match`
- [ ] Replace `add`, `sub`, `eq`, `lt`, and `ge` with operators
- [ ] Replace `char_at` with indexing
- [ ] Replace `substring` with slicing where appropriate
- [ ] Define constants for punctuation
- [ ] Keep continuation-based error handling rather than `Result`
- [ ] Preserve all current JSON validation behavior
- [ ] Add invalid-input examples or tests
- [ ] Verify that success and error continuations each run exactly once

### 8.5 Example audit

Checklist:

- [ ] `hello.sl`
- [ ] `arithmetic.sl`
- [ ] `comparison.sl`
- [ ] `strings.sl`
- [ ] `lambda.sl`
- [ ] `pair.sl`
- [ ] `nested_calls.sl`
- [ ] `command.sl`
- [ ] `match_exhaustive.sl`
- [ ] `mu_escape.sl`
- [ ] `file_io.sl`
- [ ] `json_parser.sl`
- [ ] intentional diagnostic examples:
  - [ ] `linearity_error.sl`
  - [ ] `polarity_error.sl`

For each example, ensure:

- [ ] it runs with `slc run`
- [ ] it uses the intended surface syntax rather than builtin spellings
- [ ] it demonstrates one clear concept
- [ ] it has no unexplained magic numbers
- [ ] it remains compatible with continuation linearity

---

## Phase 9: Testing and quality gates

Every phase must end with the full quality gate.

Checklist:

- [ ] `cargo fmt`
- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Run every non-diagnostic example with `slc run`
- [ ] Confirm diagnostic examples still fail with intended errors
- [ ] Check that no compiler change reduces existing test coverage
- [ ] Add regression tests for every syntax feature
- [ ] Add pretty-printer or round-trip tests where applicable
- [ ] Add type-checker diagnostics with spans
- [ ] Add linearity diagnostics for continuation-sensitive sugar

---

## Suggested implementation order

The phases are ordered by impact and dependency:

1. `else if`
2. Real `+char`
3. Binary operators
4. `&&`, `||`, and `!`
5. Indexing and slicing
6. Char, or-, and range patterns
7. Match guards
8. `const`
9. `?` and continuation-call sugar
10. Example cleanup

The highest immediate impact comes from the first three phases. They turn:

```sl
if eq(ch, 45) {
    parse_number(input, pos, ok, err)
} else {
    if eq(ch, 34) {
        parse_string(input, pos, ok, err)
    } else {
        err("expected JSON value")
    }
}
```

into:

```sl
if ch == '-' {
    parse_number(input, pos, ok, err)
} else if ch == '"' {
    parse_string(input, pos, ok, err)
} else {
    err("expected JSON value")
}
```

and eventually into:

```sl
match input[pos] {
    '-' => parse_number(input, pos, ok, err),
    '"' => parse_string(input, pos, ok, err),
    _ => err("expected JSON value"),
}
```

without changing the underlying symmetric calculus.
