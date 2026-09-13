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
            atom => atom.clone(),
        }
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
        }
    }

    /// Is this a positive type?
    pub fn is_positive(&self) -> bool {
        match self {
            // The dual of a negative type is positive.
            Type::Dual(inner) => inner.is_negative(),
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

    #[test]
    fn dual_collapses_explicit_dual() {
        // Dual is a no-op wrapper: dual(Dual(A)) == A
        assert_eq!(Type::Dual(Box::new(Type::Pos(Base::I32))).dual(), Type::Pos(Base::I32));
    }
}
