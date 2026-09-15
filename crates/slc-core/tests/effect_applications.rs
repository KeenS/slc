use slc_core::types::{Base, Effect, Row, Type};
use slc_core::typing::{RowAtom, RowConstraint, Unification};

fn reader(argument: Type) -> Row {
    Row { effects: [Effect { name: "Reader".into(), args: vec![argument] }].into(), tail: None }
}

#[test]
fn applications_with_the_same_name_keep_distinct_arguments() {
    let number = reader(Type::Pos(Base::I64));
    let text = reader(Type::Pos(Base::Str));
    let uni = Unification::new();
    assert!(
        uni.solve_rows(&[RowConstraint { sub: number.clone(), sup: number.clone() }]).is_empty()
    );
    let failures = uni.solve_rows(&[RowConstraint { sub: number.clone(), sup: text }]);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].atom, RowAtom::Effect(number.effects.into_iter().next().unwrap()));
}

#[test]
fn row_tails_do_not_erase_effect_arguments() {
    let mut uni = Unification::new();
    let tail = Row { effects: Default::default(), tail: Some(uni.fresh_row()) };
    let failures = uni.solve_rows(&[
        RowConstraint { sub: reader(Type::Pos(Base::I64)), sup: tail.clone() },
        RowConstraint { sub: tail, sup: reader(Type::Pos(Base::Str)) },
    ]);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].constraint, 1);
}

#[test]
fn incompatible_interception_cannot_escape_through_a_residual_row() {
    let mut uni = Unification::new();
    let residual = uni.fresh_row();
    let mut allowed = reader(Type::Pos(Base::Str));
    allowed.tail = Some(residual);
    let failures = uni.solve_rows(&[
        RowConstraint { sub: reader(Type::Pos(Base::I64)), sup: allowed },
        RowConstraint {
            sub: Row { effects: Default::default(), tail: Some(residual) },
            sup: Row::default(),
        },
    ]);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].constraint, 0);
}

#[test]
fn type_substitution_reaches_forcing_and_activation_rows() {
    let mut uni = Unification::new();
    let parameter = uni.fresh_var();
    let delayed = Type::delayed(
        Type::rowed(Type::arrow(Type::ONE, Type::ONE), reader(parameter.clone())),
        reader(parameter.clone()),
    );
    uni.unify(&parameter, &Type::Pos(Base::I64)).unwrap();
    let expected = Type::delayed(
        Type::rowed(Type::arrow(Type::ONE, Type::ONE), reader(Type::Pos(Base::I64))),
        reader(Type::Pos(Base::I64)),
    );
    assert_eq!(uni.apply(&delayed), expected);
    assert!(
        uni.solve_rows(&[RowConstraint {
            sub: reader(parameter),
            sup: reader(Type::Pos(Base::I64)),
        }])
        .is_empty()
    );
}

#[test]
fn declaration_instantiation_reaches_effect_arguments_and_row_tails() {
    let template =
        Type::rowed(Type::ONE, Row { effects: reader(Type::Param(0)).effects, tail: Some(1) });
    let extra = Row { effects: [Effect::from("IO")].into(), tail: Some(7) };
    let instantiated =
        template.instantiate(&[Type::Pos(Base::Str), Type::rowed(Type::ONE, extra.clone())]);
    let mut expected = reader(Type::Pos(Base::Str));
    expected.effects.extend(extra.effects);
    expected.tail = extra.tail;
    assert_eq!(instantiated, Type::rowed(Type::ONE, expected));
}

#[test]
fn occurs_check_reaches_variables_inside_effect_arguments() {
    for forcing in [false, true] {
        let mut uni = Unification::new();
        let variable = uni.fresh_var();
        let recursive = if forcing {
            Type::delayed(Type::TOP, reader(variable.clone()))
        } else {
            Type::rowed(Type::TOP, reader(variable.clone()))
        };
        assert!(uni.unify(&variable, &recursive).is_err());
    }
}

#[test]
fn effect_arguments_print_without_changing_nullary_effect_spelling() {
    assert_eq!(Effect::from("IO").to_string(), "IO");
    assert_eq!(reader(Type::Pos(Base::I64)).to_string(), "{Reader<+i64>}");
    let effect =
        Effect { name: "Shift".into(), args: vec![Type::Pos(Base::I64), Type::Pos(Base::Str)] };
    assert_eq!(effect.to_string(), "Shift<+i64, +String>");
}

#[test]
fn printed_effectful_types_round_trip_with_their_arguments() {
    let row = reader(Type::rowed(
        Type::arrow(Type::ONE, Type::Pos(Base::Str)),
        Row { effects: [Effect::from("IO")].into(), tail: Some(2) },
    ));
    for inner in [
        Type::ONE,
        Type::BOTTOM,
        Type::TOP,
        Type::ZERO,
        Type::Neg(Base::I64),
        Type::Named("Answer".into(), vec![Type::Param(0)]),
        Type::arrow(Type::Pos(Base::I64), Type::Pos(Base::Str)),
        Type::Tensor(vec![Type::Pos(Base::I64), Type::Pos(Base::Str)]),
    ] {
        for ty in [
            Type::rowed(inner.clone(), row.clone()),
            Type::delayed(Type::rowed(inner, row.clone()), row.clone()).dual(),
        ] {
            let printed = ty.to_string();
            assert_eq!(slc_core::parse::parse_type(&printed).unwrap(), ty, "{printed}");
        }
    }
}

#[test]
fn latent_declaration_rows_substitute_effect_arguments() {
    let mut uni = Unification::new();
    uni.set_latent_decls([("Source".into(), reader(Type::Param(0)), None)]);
    let source = Type::Named("Source".into(), vec![Type::Pos(Base::Str)]).dual();
    assert_eq!(uni.latent_row(&source), reader(Type::Pos(Base::Str)));
}

#[test]
fn argument_inference_follows_row_tails() {
    let mut uni = Unification::new();
    let argument = uni.fresh_var();
    let intermediate = Row { effects: Default::default(), tail: Some(uni.fresh_row()) };
    uni.constrain_row(reader(argument.clone()), intermediate.clone());
    uni.constrain_row(intermediate, reader(Type::Pos(Base::I64)));
    uni.infer_row_arguments(0);
    assert_eq!(uni.apply(&argument), Type::Pos(Base::I64));
    assert!(uni.solve_rows(uni.row_constraints()).is_empty());
}

#[test]
fn inferred_effect_arguments_keep_nested_rows_invariant() {
    let mut uni = Unification::new();
    let nested = Row { effects: Default::default(), tail: Some(uni.fresh_row()) };
    let io = Row { effects: [Effect::from("IO")].into(), tail: None };
    uni.constrain_row(
        reader(Type::rowed(Type::TOP, nested.clone())),
        reader(Type::rowed(Type::TOP, io)),
    );
    uni.infer_row_arguments(0);
    assert!(uni.solve_rows(uni.row_constraints()).is_empty());
    uni.constrain_row(nested, Row::default());
    uni.infer_row_arguments(0);
    assert!(!uni.solve_rows(uni.row_constraints()).is_empty());
}

#[test]
fn failed_argument_equations_do_not_partially_bind_variables() {
    let mut uni = Unification::new();
    let argument = uni.fresh_var();
    let application =
        |args| Row { effects: [Effect { name: "Pair".into(), args }].into(), tail: None };
    uni.constrain_row(
        application(vec![argument.clone(), Type::Pos(Base::I64)]),
        application(vec![Type::Pos(Base::Str), Type::Pos(Base::Str)]),
    );
    uni.infer_row_arguments(0);
    assert_eq!(uni.apply(&argument), argument);
    assert_eq!(uni.solve_rows(uni.row_constraints()).len(), 1);
}
