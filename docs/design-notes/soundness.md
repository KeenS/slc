# Subject reduction

Status: proved for the core, and the surface checker refuses an unsolved
cut. The proof is [`proof/Slc/Core.lean`](../../proof/Slc/Core.lean).
[`docs/design/core.md`](../design/core.md) states the judgment next to the
reduction rules.

## The judgment

A command `⟨t ∥ e⟩` is well typed when `t` proves `A` and `e` refutes `A`.
An inference variable is that `A` only when both sides carry the same
variable. `dual(?i)` is not `?j`. The Rust checker stores a continuation of
`A` at `dual(A)` and accepts the cut when `dual(t) = e`; it now refuses the
cut while either side still contains an inference variable
(`slc_core::typing::infer_command`).

`proof/` checks the theorem with `lake build`. `step` returns a reduct in
the same contexts, which is preservation. `progress` covers a command with
no free variable, and `step_eq_progress` says `step` takes that step.
`Ty.dual_dual` is the involution. `funEncoding` is the design's reading of
`A → B` as `(dual(A) ; B)`, and as `dual(A)` when `B` is `(;)`.

The λ and application rules match on `arr`. A reified co-term has type
`neg A`, and the co rule is `⟨co(e') ∥ unapp v⟩ → ⟨v ∥ e'⟩`. A product
consumer binds the tensor; a component is a projection of that value.
A label in the proof is an index into the payload list, which is what a
unique surface label denotes. The μ rule substitutes the co-term, as
`reduce::step` now does.

## What the surface refuses

Checked after a declaration's unification has finished:

- A `proc` body whose type is not `(;)`, including a variable nothing solved.
- A `mu` scrutinee that is not positive: a rigid `+T` and a variable that
  met `<+T>` stay, and an unsigned variable is refused.
- The right of a cut whose type is not a consumer. An unsigned variable is
  not a negative type.
- A pin whose two sides are not a solved type. Two flexible variables are
  not the same type because a throwaway unification would bind one to the
  other.

Polarity of a variable that meets a signed parameter was already checked.

## Specifications

[`proof/Slc/Spec.lean`](../../proof/Slc/Spec.lean) is the type system of a
`spec`. A method signature may mention `Self`, a trait parameter, and an
associated type. An impl method is a core term of that signature with those
replaced: `Self` by the implementing type, each parameter by its argument,
each associated type by the type the impl chose. A pin `Item = c` holds when
that choice is `c`. The dictionary entry is the method as a function, so its
type is the instantiated specification. A bounded call receives that
dictionary; substituting it preserves the command's type.

A set of impls is coherent when each key — the trait, the implementing type,
and the trait arguments — occurs once. `selected_unique` is that one
selection, and `not_selected` is that a missing key cannot be selected. A
child impl carries the parent's selection for the same key. Method names do
not repeat.

`impl_call_steps` is the bridge to the core: the command of a concrete call
reduces by the core's application rule, and `step` then preserves its type.
Default bodies, and the dictionary a generic impl such as `Display for
List<T>` builds for its element, are checked as impl methods once they have
been copied or constructed; the proof is that check, not the construction.
