//! Types with explicit polarity.

/// A base (atomic) type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Base {
    I32,
    I64,
    U32,
    U64,
    Bool,
    Str,
    Char,
    Unit,
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
    /// Tensor: `A ⊗ B`.
    Tensor(Box<Type>, Box<Type>),
    /// Par: `A ⅋ B`.
    Par(Box<Type>, Box<Type>),
    /// Unit for tensor.
    One,
    /// Unit for par.
    Bottom,
    /// Explicit dual application.
    Dual(Box<Type>),
    /// Additive with: `A & B`.
    With(Box<Type>, Box<Type>),
    /// Additive sum: `A + B`.
    Sum(Box<Type>, Box<Type>),
    /// Exponential: `!A`.
    Bang(Box<Type>),
    /// List sugar.
    List(Box<Type>),
    /// A named positive declaration, such as `struct` or `enum`. Named types
    /// are opaque to core unification; declaration-specific fields and
    /// variants are checked by the surface checker.
    Named(String),
}

impl Type {
    /// A function type: `A → B` is `-A ⅋ B`, so its dual is `A ⊗ -B` — an
    /// argument together with a continuation for the result, which is what a
    /// call stack is. `A → ⊥` is `-A`, since `⊥` is the unit of `⅋`.
    pub fn arrow(argument: Type, result: Type) -> Type {
        if result == Type::Bottom {
            return argument.dual();
        }
        Type::Par(Box::new(argument.dual()), Box::new(result))
    }

    /// `arrow` with the arguments in fold order: the accumulated result
    /// first, then the input.
    pub fn arrow_from(result: Type, argument: Type) -> Type {
        Type::arrow(argument, result)
    }

    /// The dual of a type. An involution: `dual(dual(t)) == t`.
    pub fn dual(&self) -> Type {
        match self {
            Type::Var(v) => Type::Var(*v),
            Type::Pos(b) => Type::Neg(*b),
            Type::Neg(b) => Type::Pos(*b),
            Type::Tensor(a, b) => Type::Par(Box::new(a.dual()), Box::new(b.dual())),
            Type::Par(a, b) => Type::Tensor(Box::new(a.dual()), Box::new(b.dual())),
            Type::One => Type::Bottom,
            Type::Bottom => Type::One,
            Type::Dual(t) => (**t).clone(),
            Type::With(a, b) => Type::Sum(Box::new(a.dual()), Box::new(b.dual())),
            Type::Sum(a, b) => Type::With(Box::new(a.dual()), Box::new(b.dual())),
            Type::Bang(t) => Type::Bang(Box::new(t.dual())),
            Type::List(t) => Type::List(Box::new(t.dual())),
            Type::Named(name) => Type::Dual(Box::new(Type::Named(name.clone()))),
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
                    | Type::One
                    | Type::Sum(..)
                    | Type::Bang(_)
                    | Type::List(_)
                    | Type::Named(_)
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
                Type::Var(_) | Type::Neg(_) | Type::Par(..) | Type::Bottom | Type::With(..)
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
            Type::Pos(Base::Bool),
            Type::Neg(Base::Bool),
            Type::Pos(Base::Char),
            Type::Neg(Base::Char),
            Type::One,
            Type::Bottom,
            Type::Tensor(Box::new(Type::Pos(Base::I32)), Box::new(Type::Pos(Base::Bool))),
            Type::Par(Box::new(Type::Neg(Base::I32)), Box::new(Type::Neg(Base::Bool))),
            Type::With(Box::new(Type::Neg(Base::I32)), Box::new(Type::Neg(Base::Bool))),
            Type::Sum(Box::new(Type::Pos(Base::I32)), Box::new(Type::Pos(Base::Bool))),
            Type::Bang(Box::new(Type::Pos(Base::I32))),
            Type::List(Box::new(Type::Pos(Base::I32))),
            Type::Named("Color".into()),
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
        let t = Type::Tensor(Box::new(Type::Pos(Base::I32)), Box::new(Type::Pos(Base::Bool)));
        let expected = Type::Par(Box::new(Type::Neg(Base::I32)), Box::new(Type::Neg(Base::Bool)));
        assert_eq!(t.dual(), expected);
    }

    #[test]
    fn dual_swaps_units() {
        assert_eq!(Type::One.dual(), Type::Bottom);
        assert_eq!(Type::Bottom.dual(), Type::One);
    }

    #[test]
    fn dual_swaps_additives() {
        let t = Type::Sum(Box::new(Type::Pos(Base::I32)), Box::new(Type::Pos(Base::Bool)));
        let expected = Type::With(Box::new(Type::Neg(Base::I32)), Box::new(Type::Neg(Base::Bool)));
        assert_eq!(t.dual(), expected);
    }

    #[test]
    fn dual_collapses_explicit_dual() {
        // Dual is a no-op wrapper: dual(Dual(A)) == A
        assert_eq!(Type::Dual(Box::new(Type::Pos(Base::I32))).dual(), Type::Pos(Base::I32));
    }
}
