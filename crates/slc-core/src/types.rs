//! Types with explicit polarity.

/// A base (atomic) type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Base {
    I32,
    I64,
    U32,
    U64,
    Str,
    Char,
    Unit,
    /// An open file handle, produced by `fs::open` and consumed by
    /// `fs::close`.
    File,
}

/// What running a value performs: effects by name, and at most one row
/// variable standing for the rest. A negative type without a row performs
/// nothing (`docs/design-notes/rows-in-types.md`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    pub effects: std::collections::BTreeSet<String>,
    pub tail: Option<usize>,
}

impl Row {
    /// Performs nothing: no effect, and no variable that could stand for one.
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty() && self.tail.is_none()
    }
}

/// A type in the λ̄μμ̃ calculus, with explicit positive/negative polarity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// Inference variable.
    Var(usize),
    /// Positive atom: `+B`.
    Pos(Base),
    /// Negative atom: `-B`.
    Neg(Base),
    /// Tensor: `(A, B, …)`, the positive product of its components — any
    /// number of them, so `(A, (B, C))` and `(A, B, C)` are different types.
    /// With none it is the unit, `(,)`.
    Tensor(Vec<Type>),
    /// Par: `(A ; B ; …)`, the negative product; with no components, `(;)`.
    Par(Vec<Type>),
    /// Explicit dual application.
    Dual(Box<Type>),
    /// With: `(A & B & …)`, the negative sum; with no components, `(&)`.
    With(Vec<Type>),
    /// Sum: `(A | B | …)`, the positive sum; with no components, `(|)`.
    Sum(Vec<Type>),
    /// A declaration's type parameter, by position: what `T` becomes inside
    /// the declaration's own field and payload types. It never reaches
    /// unification — a use of the declaration substitutes its arguments for
    /// the parameters first.
    Param(usize),
    /// A named positive declaration, such as `data` or `enum`. Named types
    /// are opaque to core unification; declaration-specific fields and
    /// variants are checked by the surface checker.
    Named(String, Vec<Type>),
    /// A negative type together with what running a value of it performs:
    /// calling a function, feeding a consumer, demanding an item. Never
    /// built with the empty row, which is the type alone. Its dual keeps the
    /// row, so `dual` stays an involution; a positive one means nothing of
    /// its own.
    Rowed(Box<Type>, Row),
}

impl Type {
    /// `(,)`, the tensor of nothing.
    pub const ONE: Type = Type::Tensor(Vec::new());
    /// `(;)`, the par of nothing.
    pub const BOTTOM: Type = Type::Par(Vec::new());
    /// `(|)`, the sum of nothing.
    pub const ZERO: Type = Type::Sum(Vec::new());
    /// `(&)`, the with of nothing.
    pub const TOP: Type = Type::With(Vec::new());

    /// Substitute a declaration's arguments for its parameters: `Param(i)`
    /// becomes `args[i]`, recursively. The instantiation of `List<T>`'s
    /// payload types at `List<i64>`.
    pub fn instantiate(&self, args: &[Type]) -> Type {
        match self {
            Type::Param(i) => args.get(*i).cloned().unwrap_or_else(|| self.clone()),
            Type::Tensor(xs) => Type::Tensor(xs.iter().map(|x| x.instantiate(args)).collect()),
            Type::Par(xs) => Type::Par(xs.iter().map(|x| x.instantiate(args)).collect()),
            Type::With(xs) => Type::With(xs.iter().map(|x| x.instantiate(args)).collect()),
            Type::Sum(xs) => Type::Sum(xs.iter().map(|x| x.instantiate(args)).collect()),
            Type::Dual(t) => Type::Dual(Box::new(t.instantiate(args))),
            Type::Named(name, own) => {
                Type::Named(name.clone(), own.iter().map(|a| a.instantiate(args)).collect())
            }
            Type::Rowed(t, row) => {
                // A declaration's row variable is a parameter by position:
                // where that argument is a row — carried on the unit, or the
                // empty one — the row takes its place.
                let row = match row.tail.and_then(|position| args.get(position)) {
                    Some(Type::Rowed(unit, given)) if **unit == Type::ONE => Row {
                        effects: row.effects.iter().chain(&given.effects).cloned().collect(),
                        tail: given.tail,
                    },
                    Some(unit) if *unit == Type::ONE => {
                        Row { effects: row.effects.clone(), tail: None }
                    }
                    _ => row.clone(),
                };
                Type::rowed(t.instantiate(args), row)
            }
            atom => atom.clone(),
        }
    }

    /// `ty`, performing `row` when it runs. The empty row is `ty` itself.
    pub fn rowed(ty: Type, row: Row) -> Type {
        if row.is_empty() { ty } else { Type::Rowed(Box::new(ty), row) }
    }

    /// A function type: `A -> B` is `(dual(A) ; B)`, so its dual is
    /// `(A, dual(B))` — an argument together with a continuation for the
    /// result, which is what a call stack is. `(A -> (;))` is `dual(A)`, since
    /// `(;)` is the unit of `;`.
    pub fn arrow(argument: Type, result: Type) -> Type {
        if result == Type::BOTTOM {
            return argument.dual();
        }
        Type::Par(vec![argument.dual(), result])
    }

    /// `arrow` with the arguments in fold order: the accumulated result
    /// first, then the input.
    pub fn arrow_from(result: Type, argument: Type) -> Type {
        Type::arrow(argument, result)
    }

    /// The dual of a type. An involution: `dual(dual(t)) == t`.
    pub fn dual(&self) -> Type {
        match self {
            // The dual of an unknown is not itself: `dual(?a)` is the
            // consumer of whatever `?a` becomes, and stays wrapped until it
            // is known.
            Type::Var(v) => Type::Dual(Box::new(Type::Var(*v))),
            Type::Pos(b) => Type::Neg(*b),
            Type::Neg(b) => Type::Pos(*b),
            Type::Tensor(xs) => Type::Par(xs.iter().map(Type::dual).collect()),
            Type::Par(xs) => Type::Tensor(xs.iter().map(Type::dual).collect()),
            Type::Dual(t) => (**t).clone(),
            Type::With(xs) => Type::Sum(xs.iter().map(Type::dual).collect()),
            Type::Sum(xs) => Type::With(xs.iter().map(Type::dual).collect()),
            Type::Named(name, args) => {
                Type::Dual(Box::new(Type::Named(name.clone(), args.clone())))
            }
            Type::Param(i) => Type::Dual(Box::new(Type::Param(*i))),
            Type::Rowed(t, row) => Type::Rowed(Box::new(t.dual()), row.clone()),
        }
    }

    /// Is this a positive type?
    pub fn is_positive(&self) -> bool {
        match self {
            // The dual of a negative type is positive.
            Type::Dual(inner) => inner.is_negative(),
            Type::Rowed(inner, _) => inner.is_positive(),
            other => matches!(
                other,
                Type::Var(_)
                    | Type::Pos(_)
                    | Type::Tensor(..)
                    | Type::Sum(..)
                    | Type::Named(..)
                    | Type::Param(_)
            ),
        }
    }

    /// Is this a negative type?
    pub fn is_negative(&self) -> bool {
        match self {
            // The dual of a positive type is negative.
            Type::Dual(inner) => inner.is_positive(),
            Type::Rowed(inner, _) => inner.is_negative(),
            other => matches!(
                other,
                Type::Var(_) | Type::Param(_) | Type::Neg(_) | Type::Par(..) | Type::With(..)
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_types() -> Vec<Type> {
        vec![
            Type::Pos(Base::I32),
            Type::Neg(Base::I32),
            Type::Pos(Base::Char),
            Type::Neg(Base::Char),
            Type::ONE,
            Type::BOTTOM,
            Type::Tensor(vec![Type::Pos(Base::I32), Type::Pos(Base::Char)]),
            Type::Par(vec![Type::Neg(Base::I32), Type::Neg(Base::Char)]),
            Type::With(vec![Type::Neg(Base::I32), Type::Neg(Base::Char)]),
            Type::Sum(vec![Type::Pos(Base::I32), Type::Pos(Base::Char)]),
            Type::Named("Color".into(), Vec::new()),
        ]
    }

    #[test]
    fn dual_is_involution() {
        for t in all_types() {
            assert_eq!(t.dual().dual(), t, "dual(dual({t:?})) != {t:?}");
        }
    }

    #[test]
    fn dual_maps_atoms() {
        assert_eq!(Type::Pos(Base::I32).dual(), Type::Neg(Base::I32));
        assert_eq!(Type::Neg(Base::I32).dual(), Type::Pos(Base::I32));
    }

    #[test]
    fn dual_swaps_tensor_par() {
        let t = Type::Tensor(vec![Type::Pos(Base::I32), Type::Pos(Base::Char)]);
        let expected = Type::Par(vec![Type::Neg(Base::I32), Type::Neg(Base::Char)]);
        assert_eq!(t.dual(), expected);
    }

    #[test]
    fn dual_swaps_units() {
        assert_eq!(Type::ONE.dual(), Type::BOTTOM);
        assert_eq!(Type::BOTTOM.dual(), Type::ONE);
    }

    #[test]
    fn dual_swaps_additives() {
        let t = Type::Sum(vec![Type::Pos(Base::I32), Type::Pos(Base::Char)]);
        let expected = Type::With(vec![Type::Neg(Base::I32), Type::Neg(Base::Char)]);
        assert_eq!(t.dual(), expected);
    }

    fn exn() -> Row {
        Row { effects: ["Exn".to_string()].into_iter().collect(), tail: None }
    }

    #[test]
    fn a_row_rides_through_dual() {
        let function = Type::rowed(Type::arrow(Type::Pos(Base::I64), Type::Pos(Base::I64)), exn());
        let consumer = Type::rowed(Type::Neg(Base::Str), exn());
        for t in [function, consumer] {
            assert_eq!(t.dual().dual(), t, "dual(dual({t:?})) != {t:?}");
            assert!(matches!(t.dual(), Type::Rowed(_, ref row) if *row == exn()), "{t:?}");
        }
    }

    #[test]
    fn the_empty_row_is_no_wrapper() {
        assert_eq!(Type::rowed(Type::Neg(Base::I64), Row::default()), Type::Neg(Base::I64));
    }

    #[test]
    fn a_rowed_type_has_the_polarity_beneath() {
        let consumer = Type::rowed(Type::Neg(Base::I64), exn());
        assert!(consumer.is_negative() && !consumer.is_positive());
        assert!(consumer.dual().is_positive() && !consumer.dual().is_negative());
    }

    fn io() -> Row {
        Row { effects: ["IO".to_string()].into_iter().collect(), tail: None }
    }

    #[test]
    fn a_value_meeting_its_slot_records_that_its_row_fits() {
        use crate::typing::{RowAtom, RowConstraint, Unification};
        let mut uni = Unification::new();
        let var = uni.fresh_var();
        let value = Type::rowed(Type::arrow(var.clone(), Type::Pos(Base::I64)), io());
        let slot = Type::rowed(Type::arrow(Type::Pos(Base::Str), Type::Pos(Base::I64)), exn());
        assert!(uni.unify(&slot, &value).is_ok(), "rows do not stop unification");
        assert_eq!(uni.apply(&var), Type::Pos(Base::Str));
        assert_eq!(uni.row_constraints(), [RowConstraint { sub: io(), sup: exn() }]);
        let failures = uni.solve_rows(uni.row_constraints());
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].atom, RowAtom::Effect("IO".into()));
    }

    #[test]
    fn a_pure_value_fits_any_slot_and_a_row_fits_no_pure_one() {
        let mut uni = crate::typing::Unification::new();
        let pure = Type::arrow(Type::Pos(Base::I64), Type::Pos(Base::I64));
        let rowed = Type::rowed(pure.clone(), exn());
        assert!(uni.unify(&rowed, &pure).is_ok());
        assert!(uni.solve_rows(uni.row_constraints()).is_empty());
        assert!(uni.unify(&pure, &rowed).is_ok());
        assert_eq!(uni.solve_rows(uni.row_constraints()).len(), 1);
    }

    #[test]
    fn a_flexible_row_grows_to_what_flows_into_it_and_a_rigid_one_stays_itself() {
        use crate::typing::{RowAtom, RowConstraint, Unification};
        let mut uni = Unification::new();
        let body = uni.fresh_row();
        let declared = uni.fresh_rigid_row();
        let open = |tail| Row { effects: Default::default(), tail: Some(tail) };
        // What the body performs flows into its row, and the body's row must
        // fit inside `{IO, ..E}`.
        let constraints = vec![
            RowConstraint { sub: exn(), sup: open(body) },
            RowConstraint { sub: open(declared), sup: open(body) },
            RowConstraint { sub: open(body), sup: Row { tail: Some(declared), ..io() } },
        ];
        let failures = uni.solve_rows(&constraints);
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert_eq!(failures[0].atom, RowAtom::Effect("Exn".into()));
        // Handled, `Exn` no longer reaches the declaration's row.
        let handled = vec![
            RowConstraint { sub: exn(), sup: Row { tail: Some(body), ..exn() } },
            RowConstraint { sub: open(declared), sup: open(body) },
            RowConstraint { sub: open(body), sup: Row { tail: Some(declared), ..io() } },
        ];
        assert!(uni.solve_rows(&handled).is_empty());
    }

    #[test]
    fn a_nested_row_must_be_the_same_on_both_sides() {
        let mut uni = crate::typing::Unification::new();
        let pure = Type::arrow(Type::Pos(Base::I64), Type::Pos(Base::I64));
        let list = |f: Type| Type::Named("List".into(), vec![f]);
        assert!(uni.unify(&list(Type::rowed(pure.clone(), exn())), &list(pure)).is_ok());
        assert_eq!(uni.solve_rows(uni.row_constraints()).len(), 1, "{:?}", uni.row_constraints());
    }

    #[test]
    fn a_row_prints_as_the_surface_writes_it() {
        let function = Type::rowed(Type::arrow(Type::Pos(Base::I64), Type::Pos(Base::I64)), exn());
        assert_eq!(function.to_string(), "(+i64 -> +i64 / {Exn})");
        let open = Row { effects: exn().effects, tail: Some(3) };
        assert_eq!(Type::rowed(Type::Neg(Base::Str), open).to_string(), "(-String / {Exn, ..?3})");
    }

    #[test]
    fn dual_collapses_explicit_dual() {
        // Dual is a no-op wrapper: dual(Dual(A)) == A
        assert_eq!(Type::Dual(Box::new(Type::Pos(Base::I32))).dual(), Type::Pos(Base::I32));
    }
}
