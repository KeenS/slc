# Structural stage adapters

The checker keeps ordinary unification ordered. When exact matching fails
at a value boundary, it derives adapter evidence instead of accepting a
representation-changing type equality. Failed derivations are transactional:
neither unification constraints nor expression adapters escape a failed
alternative.

## Evidence and declaration lifting

`slc-check/src/adapters.rs` builds a graph of identity, reversal, composition,
function, product, labelled-value, consumer, and menu adapters. Function
inputs reverse direction. Declaration parameters are checked in both
directions; dual parameter occurrences use dual evidence, not a variance
inferred from their polarity annotations. A dual stage reversal exchanges
the corresponding demand product's two components, without introducing a
general tuple-reordering rule.

Data and enum adapters inspect existing values and rebuild their payloads.
Menu adapters forward only the selected request and adapt its answer.
Form and other named-consumer adapters adapt incoming demands. Recursive
type pairs refer back to an adapter already being derived; lowering emits
ordinary, mutually recursive core functions with inaccessible `$` names.

The graph must be finite. Growing recursive specializations are refused,
and derivation has a 128-active-pair guard. Opaque types have no structural
lifting rule. In particular, adapting `Handler` type arguments or changing
the type arguments of an effect capability is not a structural traversal.
Rows nested in structures remain invariant, including typed effect arguments.

## Forcing without activation

Lowering applies generated adapters through the private `$adapt` intrinsic.
For an already available value, it applies the adapter. For an implicit
delay, it stores the original delay and adapter together, without running
either computation. The runtime forces that original value on every demand,
then applies the adapter, using ordinary continuation frames. Forcing an
adapted function stops at the resulting closure; it does not apply it.

Consequently `Delayed<T, E>` lifts to `Delayed<U, E>` without changing `E`
or either result's activation row. Handler capture and multi-shot resumption
include these frames just like other pending work. There is no cache, no
implicit eager binding, and no traversal through a delayed payload merely
because its enclosing record or variant is being adapted.

## Validation

`crates/slc-driver/tests/structural_adapters.rs` covers stored products and
sums, records, recursive enums and menus, higher-order inputs and outputs,
dual parameter occurrences, named consumers, repeated forcing, eager
forcing without activation, `Lazy` demand effects, generic polarities,
named forms, and multi-shot resumption through an adapter.
Double reversal and adapting before versus after wrapping have exact-output
regressions. Negative tests retain forcing and activation rows and refuse
opaque lifting, capability changes, and growing recursive specialization.

`examples/duality/structural_adapters.sl` is an exact-output acceptance example.
These regressions are not a general proof of observational equivalence in
the presence of control; the language's mechanized soundness and coherence
arguments remain open.
