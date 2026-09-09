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

- [x] Update parser to accept `else if`
- [x] Represent it as nested `Expr::If`; no new AST node is required
- [x] Add parser unit test for a three-branch chain
- [x] Add parser unit test for a chain without a final `else`
- [x] Add integration test that evaluates an `else if` chain
- [x] Add checker test confirming each branch is still checked
- [x] Add lowering test confirming it lowers to the same term as nested `if`
- [x] Rewrite JSON parser’s `parse_value` to use `else if`
- [x] Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and
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

- [x] Add `&&`, `||`, and `!` to the lexer
- [x] Add AST operators:
  - [x] `BinOp::And`
  - [x] `BinOp::Or`
  - [x] `UnOp::Not`
- [x] Add an `UnOp` field to the surface AST if unary operators are not
      already represented
- [x] Parse `&&` and `||` with lower precedence than comparisons
- [x] Parse `!` as a unary operator
- [x] Type-check that both sides are `+bool`
- [x] Lower short-circuiting using existing `if`
- [x] Add tests for short-circuit evaluation
- [x] Add tests for boolean type mismatches
- [x] Rewrite parser code using chained comparisons and boolean operators

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

- [x] Add `Base::Char` to `slc-core::types::Base`
- [x] Update the type pretty-printer
- [x] Update dual/polarity behavior if needed
- [x] Add property tests for `dual(+char) == -char`
- [x] Change `Value::Char::type_of()` to return `Type::Pos(Base::Char)`
- [x] Add `char` to surface type lowering
- [x] Add `char` to inference support
- [x] Add type tests for positive and negative char types
- [x] Add runtime tests for char values

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

- [x] Rewrite the char lexer to use a shared escape-decoding routine
- [x] Support escapes in both strings and chars
- [x] Reject unterminated character literals with a useful span
- [x] Reject empty and multi-character char literals
- [x] Lower `Expr::Char(c)` to a char value rather than a pseudo-variable
- [x] Add parser tests for every escape form
- [x] Add runtime test for `'\n'` and `'\\'`

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

- [x] Add `Base::Char` support to equality and comparison builtins
- [x] Add `char_eq`, `char_lt`, `char_le`, `char_gt`, and `char_ge`, or make
      `eq`, `lt`, `le`, `gt`, and `ge` accept chars
- [x] Add `char_to_code(c) -> i64`
- [x] Add `code_to_char(i64) -> +char`
- [x] Add `is_digit(c: +char) -> +bool`
- [x] Add `is_ws(c: +char) -> +bool`
- [x] Add `string_push(s: +String, c: +char) -> +String`
- [x] Decide whether `char_at` remains code-based for compatibility
- [x] Add builtin tests for all new operations
- [x] Rewrite JSON parser to use `+char` instead of integer codes

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

- [x] Lower `+` to `add`
- [x] Lower `-` to `sub`
- [x] Lower `*` to `mul`
- [x] Lower `/` to `div`
- [x] Lower `%` to `rem`
- [x] Lower unary `-` to `neg`
- [x] Type-check operands as matching numeric types
- [x] Preserve arithmetic overflow and division-by-zero diagnostics
- [x] Add precedence tests
- [x] Add associativity tests
- [x] Add integration tests for each operator
- [x] Rewrite arithmetic and nested-call examples with operators

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

- [x] Lower `==` to `eq`
- [x] Lower `!=` to `ne`
- [x] Lower `<` to `lt`
- [x] Lower `>` to `gt`
- [x] Lower `<=` to `le`
- [x] Lower `>=` to `ge`
- [x] Type-check comparison operands as compatible numeric or char values
- [x] Add tests for numeric comparisons
- [x] Add tests for char comparisons
- [x] Add tests for type mismatch diagnostics
- [x] Rewrite comparison and JSON examples with operators

### 3.3 String operators

Syntax:

```sl
"Hello, " + "world"
```

Implementation checklist:

- [x] Lower `+` on strings to `str_concat`
- [x] Add inference or bidirectional checking to distinguish numeric and
      string `+`
- [x] Add a type diagnostic when operands do not agree
- [x] Add integration test for string concatenation
- [x] Rewrite string examples with `+`

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

- [x] Add postfix `[...]` parsing
- [x] Define `a[i]` as sugar for an indexing operation
- [x] Add `String` indexing returning `+char`
- [x] Add `List` indexing returning the element type
- [x] Add out-of-range diagnostics with source spans
- [x] Add checker tests for index types
- [x] Add runtime tests for valid and invalid indexes

### 4.2 Slicing

Syntax:

```sl
input[start..end]
input[start..]
input[..end]
```

Implementation checklist:

- [x] Add range expression AST:
  - [x] `Range`
  - [x] `RangeFrom`
  - [x] `RangeTo`
- [x] Parse `..` in postfix slicing position
- [x] Optionally parse `..=` for inclusive ranges
- [x] Lower string slicing to `substring`
- [x] Define behavior for empty slices
- [x] Add bounds checking
- [x] Add checker tests for range endpoints
- [x] Add runtime tests for every range form

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

- [x] Add `Pattern::Char(char)`
- [x] Parse char literals in patterns
- [x] Check char patterns against `+char`
- [x] Add runtime matching support
- [x] Add tests for char literal patterns and wildcards

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

- [x] Add `Pattern::Or(Vec<Pattern>)`
- [x] Parse `|` between patterns
- [x] Check all alternatives have compatible types
- [x] Add runtime matching support
- [x] Add tests for multiple alternatives
- [x] Ensure or-patterns interact correctly with exhaustiveness checking

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

- [x] Add `Pattern::Range(Box<Pattern>, Box<Pattern>)`
- [x] Parse `..=` ranges in patterns
- [x] Support char ranges first
- [x] Add integer ranges after chars are supported
- [x] Check endpoint types
- [x] Add runtime inclusive-range matching
- [x] Add tests for boundaries and invalid ranges
- [x] Update exhaustiveness checking to understand ranges

### 5.4 Guards

Syntax:

```sl
match c {
    c if c < ' ' => err("raw control character"),
    _ => parse_string_tail(...),
}
```

Implementation checklist:

- [x] Extend `MatchArm` with an optional guard expression
- [x] Parse `if` after a pattern and before `=>`
- [x] Type-check guards as `+bool`
- [x] Implement runtime guard evaluation
- [x] Define exhaustiveness rules:
  - [x] wildcard plus guard is not considered unconditionally exhaustive
  - [x] wildcard without a guard remains exhaustive
- [x] Add tests for matching with and without guards
- [x] Add diagnostic tests for non-bool guards

### 5.5 Other useful pattern forms

Implementation checklist:

- [x] Add `Pattern::Char`
- [x] Add `Pattern::Or`
- [x] Add `Pattern::Range`
- [x] Add optional binding with `@`:
  - [x] `c @ '0'..='9'`
  - [x] `x @ Some(_)`
- [x] Add rest patterns for lists:
  - [x] `[first, ..rest]`
- [x] Add struct field shorthand:
  - [x] `Point { x, y }`
- [x] Add tuple patterns with nested destructuring
- [x] Add negative integer patterns
- [x] Add float patterns only if floats become first-class

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

- [x] Add `TokenKind::Const`
- [x] Add `Decl::Const` to the surface AST
- [x] Parse `const NAME: Type = expression;`
- [x] Reject non-constant initializers in v0.1
- [x] Lower constants to global bindings
- [x] Check constant types
- [x] Add tests for char, integer, bool, and string constants
- [x] Add diagnostics for mutable or non-constant initializers
- [x] Rewrite JSON parser using named character constants

### 6.2 Local constants

Syntax:

```sl
let OPEN_BRACKET: +char = '[';
```

Implementation checklist:

- [x] Add optional type annotation to `let`
- [x] Check the initializer against the annotation
- [x] Add parser tests
- [x] Add checker tests
- [x] Add examples using annotated local bindings

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

- [x] Define the elaborated form of `e?`
- [x] Infer current success and error continuations in `fn` and `command`
      bodies
- [x] Lower `?` to continuation application
- [x] Reject `?` where there is no current error continuation
- [x] Add checker diagnostics:
  - [x] missing error continuation
  - [x] incompatible error type
- [x] Add tests for successful propagation
- [x] Add tests for error propagation
- [x] Add tests for using `?` in nested `fn` without inherited errors

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

- [x] Decide between suffix-name and bracket syntax
- [x] Add a surface form for selecting an error continuation
- [x] Elaborate to the selected continuation
- [x] Ensure unselected continuations remain linear
- [x] Add checker tests for dangling continuations
- [x] Add runtime tests for each error path
- [x] Rewrite JSON parser error handling using selected continuations

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

- [x] Decide whether `to` is required
- [x] Support explicit continuation arguments without helper builtins
- [x] Add partial-continuation application syntax consistently
- [x] Preserve linearity checking for each continuation
- [x] Add tests for commands with multiple continuations
- [x] Add tests for partially applied commands

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

- [x] Use arithmetic operators
- [x] Use unary minus where appropriate
- [x] Show overflow and division diagnostics
- [x] Keep the example concise

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

- [x] Use comparison operators
- [x] Add char comparison
- [x] Add boolean operators

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

- [x] Use string concatenation
- [x] Use indexing and slicing where useful
- [x] Show char values

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

- [x] Replace all character codes with `+char`
- [x] Replace `else` nesting with `else if` or `match`
- [x] Replace `add`, `sub`, `eq`, `lt`, and `ge` with operators
- [x] Replace `char_at` with indexing
- [x] Replace `substring` with slicing where appropriate
- [x] Define constants for punctuation
- [x] Keep continuation-based error handling rather than `Result`
- [x] Preserve all current JSON validation behavior
- [x] Add invalid-input examples or tests
- [x] Verify that success and error continuations each run exactly once

### 8.5 Example audit

Checklist:

- [x] `hello.sl`
- [x] `arithmetic.sl`
- [x] `comparison.sl`
- [x] `strings.sl`
- [x] `lambda.sl`
- [x] `pair.sl`
- [x] `nested_calls.sl`
- [x] `command.sl`
- [x] `match_exhaustive.sl`
- [x] `mu_escape.sl`
- [x] `file_io.sl`
- [x] `json_parser.sl`
- [x] intentional diagnostic examples:
  - [x] `linearity_error.sl`
  - [x] `polarity_error.sl`

For each example, ensure:

- [x] it runs with `slc run`
- [x] it uses the intended surface syntax rather than builtin spellings
- [x] it demonstrates one clear concept
- [x] it has no unexplained magic numbers
- [x] it remains compatible with continuation linearity

---

## Phase 9: Testing and quality gates

Every phase must end with the full quality gate.

Checklist:

- [x] `cargo fmt`
- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [x] Run every non-diagnostic example with `slc run`
- [x] Confirm diagnostic examples still fail with intended errors
- [x] Check that no compiler change reduces existing test coverage
- [x] Add regression tests for every syntax feature
- [x] Add pretty-printer or round-trip tests where applicable
- [x] Add type-checker diagnostics with spans
- [x] Add linearity diagnostics for continuation-sensitive sugar

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
