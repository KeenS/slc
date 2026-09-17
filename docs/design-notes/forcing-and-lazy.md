# Forcing, activation and `Lazy`

Status: implemented, including computations whose polarity is inferred
from subsequent uses. `DESIGN.md` defines the surface rules. All delayed
evaluation is call-by-name, with no memoization or call-by-need.

## Two interfaces, not literal duals

`Delayed<T, E>` is a built-in computation annotation for negative `T`.
Its runtime value is an implicit delay: application, cut or projection
forces it and then uses the result. `let+` forces it without activating
the result. The spelling does not introduce an explicit wrapper to build
or unwrap in user code.

`lazy::Lazy` remains an ordinary library declaration:

```sl
pub menu Lazy<*T, E> / {..E} { force: T }
```

The new `*T` parameter accepts either polarity. A row parameter is still
unsigned; `+T` and `-T` retain their existing restrictions. The default
demand row is empty. `.force` is an explicit menu demand that runs the
chosen arm and returns `T`; it does not activate that answer.

`dual(Lazy<T, E>)` is its request type, carrying a continuation for `T`.
It is not `Delayed<T, E>`. Symmetry supplies the request/answer interface,
not an automatic equation between an explicit menu and an implicit delay.

For negative `T`, ordinary library functions convert the interfaces:

```sl
pub fn of_delayed<-T, E>(computation: Delayed<T, ..E>) -> Lazy<T, ..E> {
    mu Lazy<T, ..E> {
        force <= { let+ value = computation; <value | force> },
    }
}

pub fn to_delayed<-T, E>(computation: Lazy<T, ..E>) -> Delayed<T, ..E> {
    let- pending = computation.force;
    pending
}
```

Neither conversion runs the computation at conversion time. Each demand
repeats it under the handlers surrounding that demand. The conversions
also preserve a result's independent activation effects.

## Distinct effect phases

```sl
data Saved { callback: Delayed<(i64 -> i64 / {Use}), {Build}> }
```

Obtaining the function performs `Build`; calling it performs `Use`.
`let+ ready = pending` performs only the former and binds the function
with its `Use` row intact. It does not mutate `pending`, so demanding
`pending` again constructs another result. Eager binding is shallow: it
does not force fields or demand a menu item.

The type checker uses separate `Type::Delayed` and `Type::Rowed` wrappers.
Only forcing rows are combined when another implicit delay is introduced.
An ordinary negative type promises pure forcing; an activation budget
cannot absorb construction effects. `Delayed<T, {}>` normalizes to `T`,
but that effect equivalence does not change runtime demand counts.

The runtime's private forcing operation repeatedly evaluates outer
`Value::Delayed` nodes until it obtains a result. It never applies that
result. Its continuation frame participates in multi-shot resumption, so
resuming construction twice reinstates the pending eager binding twice.

## Forwarding is not activation

Previously the dual of `Rowed(T, Use)` moved `Use` onto the outside of the
answer continuation's type. Forwarding a constructed function from a menu
then incorrectly charged `Use` and discarded it before fitting the answer.

Both wrappers now remain inside an explicit dual. A continuation receiving
an effectful answer preserves the answer's row rather than performing it.
Double duality is still the identity. This is why an improved `Lazy` can
return a function directly without hiding it in a positive record.

## Existing explicit encoding

A factory still expresses the same two explicit phases on two arrows:

```sl
((,) -> (i64 -> i64 / {Use}) / {Build})
```

Calling it with `(,)` constructs the function. This remains a valid
alternative, but changes the stored type and callers' interface compared
with an implicitly delayed function. Two `/` suffixes around the same
function are not a substitute for the phase distinction.

## Validation

`examples/laziness/delayed_and_lazy.sl` covers typed storage, eager alias forcing,
repeated use, conversion and a `Lazy` returning an effectful function.
`examples/laziness/lazy_effect_phases.sl` retains explicit factories and positive
wrappers as a comparison. Both have exact-output tests.

`crates/slc-driver/tests/delayed_phases.rs` additionally checks missing
forcing and activation handlers, incompatible slots and parameter kinds,
positive and negative `Lazy` answers, multi-shot construction, delayed
menus, unrestricted generic forwarding and conversion round trips. Core
tests check independent row fitting, empty rows and double duality.

`crates/slc-driver/tests/inferred_demand.rs` checks inferred negative and
positive computations, demand-time handler selection, nullary function
values, and primitive argument and callback demand. Preliminary declaration
inference supplies missing polarities for effect placement; the final check
rejects unresolved or changed choices before lowering. The same discipline
therefore governs effect rows and runtime delay marking. The runnable
`examples/laziness/inferred_demand.sl` has an exact-output regression.
