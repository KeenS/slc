//! Lowering from surface AST to λ̄μμ̃ core IR.

use crate::ast::*;
use crate::token::Span;
use slc_core::command::Command;
use slc_core::coterm::{CoCaseBranch, CoTerm};
use slc_core::term::{CoMatchBranch, Term};
use slc_core::types::{Base, Type};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static CONSTANTS: RefCell<HashMap<String, Pattern>> = RefCell::new(HashMap::new());
    /// Variant name → fully qualified label, for every declared enum. An
    /// unqualified variant name is recorded only when it is unambiguous.
    static VARIANTS: RefCell<HashMap<String, Option<String>>> = RefCell::new(HashMap::new());
    /// Trait-method call span → how the checker resolved it. A concrete
    /// receiver is a direct call to the impl; a bounded one projects the
    /// method from the enclosing function's dictionary parameter.
    static METHODS: RefCell<HashMap<Span, MethodDispatch>> = RefCell::new(HashMap::new());
    /// Call-to-bounded-function span → the dictionary arguments to pass,
    /// in the order the function's bounds are declared.
    static CALLS: RefCell<HashMap<Span, Vec<DictExpr>>> = RefCell::new(HashMap::new());
    /// Projection span → the component index the checker resolved (`.i`, or a
    /// record field's position).
    static PROJECTIONS: RefCell<HashMap<Span, Projection>> = RefCell::new(HashMap::new());
    /// Destructor name → fully qualified label, for every declared menu. An
    /// unqualified destructor name is recorded only when it is unambiguous.
    static MENUS: RefCell<HashMap<String, Option<String>>> = RefCell::new(HashMap::new());
    /// Demand span → the qualified destructor label the checker resolved
    /// (`cfg.item` on a menu, as opposed to a struct projection).
    static DEMANDS: RefCell<HashMap<Span, String>> = RefCell::new(HashMap::new());
    /// How many of a call's arguments are its value product; the rest are
    /// its menu of exits.
    static CALL_GROUPS: RefCell<HashMap<Span, usize>> = RefCell::new(HashMap::new());
    static FLOWS: RefCell<HashMap<Span, FlowShape>> = RefCell::new(HashMap::new());
    static ELABORATED: RefCell<HashMap<Span, Node<Expr>>> = RefCell::new(HashMap::new());
    /// Expression span → the swap its value needs: it is used at the
    /// mirrored `;` spelling of its type.
    static SWAPS: RefCell<HashMap<Span, usize>> = RefCell::new(HashMap::new());
    static PARS: RefCell<HashMap<Span, Vec<bool>>> = RefCell::new(HashMap::new());
    /// The spans of the computations a plain `let` delays: negative, and
    /// not values, so each runs where its result is demanded.
    static DELAYS: RefCell<std::collections::HashSet<Span>> =
        RefCell::new(std::collections::HashSet::new());
    /// The spans of the names of type `(;)`: standing as a command, each
    /// runs what it holds.
    static RUNS: RefCell<std::collections::HashSet<Span>> =
        RefCell::new(std::collections::HashSet::new());
}

/// How a trait-method call dispatches, as the checker resolved it.
#[derive(Debug, Clone)]
pub enum MethodDispatch {
    /// The receiver type is concrete: call the impl directly.
    Static(String),
    /// The receiver is a bound type parameter: project method `index` (of
    /// `count` the trait declares) from the dictionary named `dict_var`.
    Dict { dict_var: String, index: usize, count: usize },
}

/// A dictionary argument: a named dictionary — a global, or the enclosing
/// function's own parameter — applied to the dictionaries a bounded impl's
/// parameters need: `__dict_Display_List(__dict_Display_i64)`, recursively.
#[derive(Debug, Clone)]
pub struct DictExpr {
    pub name: String,
    pub args: Vec<DictExpr>,
    /// How many methods the constructed dictionary holds. One method is the
    /// function itself. Several methods are a tuple, and each method of a
    /// bounded impl is applied to `args` before the tuple is built — a tuple
    /// cannot be applied.
    pub methods: usize,
}

/// What the checker resolved about a program's trait dispatch, handed to
/// lowering so method calls become direct calls or dictionary projections
/// and bounded functions take and forward dictionaries.
/// What `base.i` or `base.field` resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Projection {
    /// The component's position.
    pub index: usize,
    /// How many components the product has.
    pub arity: usize,
    /// The record whose field it reads, when the base is a record; its fields
    /// are bound under its label, so one field needs no special case.
    pub record: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DispatchInfo {
    pub elaborated: HashMap<Span, Node<Expr>>,
    pub methods: HashMap<Span, MethodDispatch>,
    pub calls: HashMap<Span, Vec<DictExpr>>,
    /// Projection span → what `.i` or `.field` resolved to.
    pub projections: HashMap<Span, Projection>,
    /// Demand span → the qualified destructor label: `cfg.item` resolved
    /// against a `menu` declaration rather than a struct's fields.
    pub demands: HashMap<Span, String>,
    /// Call span → how many of its arguments are the value product. The
    /// rest are the menu of exits: each group packs into one argument, so
    /// the callee's single binder for that group receives it.
    pub call_groups: HashMap<Span, usize>,
    /// Flow span → what the chain turned out to be. Two bits settle it,
    /// since every middle step is an application.
    pub flows: HashMap<Span, FlowShape>,
    /// Expression span → the swap its value needs, where a value of `(A ; B)`
    /// is stored at, passed as, or returned for `(B ; A)`.
    pub swaps: HashMap<Span, usize>,
    pub adapters: Vec<Adapter>,
    /// Form value span → whether each component is positive: a consumer takes
    /// its part, and a value is taken by it.
    pub pars: HashMap<Span, Vec<bool>>,
    /// The spans of the computations a plain `let` binds unrun: its type is
    /// negative and the computation is not a value.
    pub delays: std::collections::HashSet<Span>,
    /// The spans of the names whose type is `(;)`: where one stands as a
    /// command it is run, since a delayed exit does nothing held.
    pub runs: std::collections::HashSet<Span>,
}

/// What a flow chain does, read off the types at its ends.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlowShape {
    pub yielding: Option<usize>,
    /// The chain begins with a function or a consumer rather than a value,
    /// so it denotes one: `f | k` is `λx. x | f | k`.
    pub eta: bool,
    /// The chain ends in a consumer, so its last step is the cut.
    pub cut: bool,
    /// A stage that is a command: it takes what flows in as its values and
    /// the rest of the chain as its menu of exits, so the chain ends there
    /// in a two-group call rather than a cut.
    pub row_stage: Option<usize>,
    /// The stages read the other way round, in order. `;` is commutative, so
    /// a stage may read as a consumer transformer instead of a function —
    /// `area_of(out: -i64) <- Shape` takes the continuation of its own step —
    /// and what has flowed in that far is fed to the consumer it builds.
    pub commuted: Vec<usize>,
    /// The value cut into the closing consumer has the consumer's type at
    /// its other spelling: `(A ; B)` where `(B ; A)` is wanted. The two are one
    /// type, but a value of it is a closure facing one way, so it is lowered
    /// through the swap that faces it the other.
    pub swap: Option<usize>,
    /// Stages whose result meets the next stage at the other spelling of its
    /// type, with the swap that turns the result around between the two
    /// steps.
    pub turned: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Adapter {
    Identity,
    Swap(Swap),
    Compose(usize, usize),
    Function { input: usize, output: usize },
    Consumer { input: usize },
    Product { items: Vec<usize>, additive: bool },
    ReverseProduct(Swap),
    Tagged { owner: String, branches: Vec<(String, Vec<usize>)> },
    Menu { owner: String, answers: Vec<(String, usize)> },
}

fn adapter_name(index: usize) -> String {
    format!("$adapter_{index}")
}

fn adapt_term(value: Term, adapter: usize) -> Term {
    call_curried(
        Term::Var("$adapt".into()),
        vec![Term::Tuple(vec![Term::Var(adapter_name(adapter)), value])],
    )
}

fn adapter_definition(adapter: &Adapter) -> Term {
    let value = || Term::Var("$adapt_value".into());
    let returned = |term| Command::Cut(term, CoTerm::Covar("$adapt_return".into()));
    let body = match adapter {
        Adapter::Identity => value(),
        Adapter::Swap(swap) => swap_adapter(value(), *swap),
        Adapter::Compose(first, second) => adapt_term(adapt_term(value(), *first), *second),
        Adapter::Function { input, output } => Term::Lam(
            "$adapt_argument".into(),
            Box::new(adapt_term(
                call_curried(
                    value(),
                    vec![adapt_term(Term::Var("$adapt_argument".into()), *input)],
                ),
                *output,
            )),
        ),
        Adapter::Consumer { input } => Term::Lam(
            "$adapt_argument".into(),
            Box::new(call_curried(
                value(),
                vec![adapt_term(Term::Var("$adapt_argument".into()), *input)],
            )),
        ),
        Adapter::ReverseProduct(_) => Term::Mu(
            "$adapt_return".into(),
            Box::new(Command::Cut(
                value(),
                CoTerm::MuTildeTensor(
                    vec!["$adapt_left".into(), "$adapt_right".into()],
                    Box::new(returned(Term::Tuple(vec![
                        Term::Var("$adapt_right".into()),
                        Term::Var("$adapt_left".into()),
                    ]))),
                ),
            )),
        ),
        Adapter::Product { items, .. } => {
            let binders: Vec<String> =
                (0..items.len()).map(|index| format!("$adapt_part_{index}")).collect();
            let parts = binders
                .iter()
                .zip(items)
                .map(|(name, adapter)| adapt_term(Term::Var(name.clone()), *adapter))
                .collect();
            Term::Mu(
                "$adapt_return".into(),
                Box::new(Command::Cut(
                    value(),
                    CoTerm::MuTildeTensor(binders, Box::new(returned(Term::Tuple(parts)))),
                )),
            )
        }
        Adapter::Tagged { owner, branches } => {
            let branches = branches
                .iter()
                .map(|(label, items)| {
                    let binders: Vec<String> =
                        (0..items.len()).map(|index| format!("$adapt_part_{index}")).collect();
                    let parts = binders
                        .iter()
                        .zip(items)
                        .map(|(name, adapter)| adapt_term(Term::Var(name.clone()), *adapter))
                        .collect();
                    CoCaseBranch {
                        label: label.clone(),
                        binders,
                        body: Box::new(returned(Term::Tag(
                            label.clone(),
                            Box::new(pack_group(parts)),
                        ))),
                    }
                })
                .collect();
            Term::Mu(
                "$adapt_return".into(),
                Box::new(Command::Cut(value(), CoTerm::CoCase { owner: owner.clone(), branches })),
            )
        }
        Adapter::Menu { owner, answers } => Term::CoMatch {
            owner: owner.clone(),
            branches: answers
                .iter()
                .map(|(label, adapter)| CoMatchBranch {
                    label: label.clone(),
                    binder: "$adapt_continuation".into(),
                    body: Box::new(Command::Cut(
                        value(),
                        CoTerm::Dtor(
                            label.clone(),
                            Box::new(CoTerm::MuTilde(
                                "$adapt_answer".into(),
                                Box::new(Command::Cut(
                                    adapt_term(Term::Var("$adapt_answer".into()), *adapter),
                                    CoTerm::Covar("$adapt_continuation".into()),
                                )),
                            )),
                        ),
                    )),
                })
                .collect(),
        },
    };
    Term::Lam("$adapt_value".into(), Box::new(body))
}

/// The polarities of a `;` value's two halves, `(left ; right)`, which is what
/// the swap needs to orient itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swap {
    pub left_positive: bool,
    pub right_positive: bool,
}

/// `(left ; right)` to `(right ; left)` for a value `f`: a closure taking
/// `dual(left)` becomes one taking `dual(right)` and giving back a `left`.
///
/// How it gives one back follows `left`'s polarity, because the runtime only
/// has a real continuation where a type is negative. A positive `left` is
/// captured with `μ`, whose binder — `dual(left)`, negative — is then a
/// genuine continuation to hand `f`. A negative `left` is built as the
/// consumer it is, with `μ̃`, whose binder is then a genuine value. The inner
/// cut sends `f`'s result to `k`, or `k` to it, by `right`'s polarity.
fn swap_adapter(f: Term, swap: Swap) -> Term {
    const K: &str = "__swap_k";
    const X: &str = "__swap_x";
    let result = call_curried(f, vec![Term::Var(X.into())]);
    let tail = || Box::new(CoTerm::Covar("__tail".into()));
    let cut = if swap.right_positive {
        Command::Cut(Term::Var(K.into()), CoTerm::App(result, tail()))
    } else {
        Command::Cut(result, CoTerm::App(Term::Var(K.into()), tail()))
    };
    let body = if swap.left_positive {
        Term::Mu(X.into(), Box::new(cut))
    } else {
        Term::Co(Box::new(CoTerm::MuTilde(X.into(), Box::new(cut))))
    };
    Term::Lam(K.into(), Box::new(body))
}

/// The dictionary parameter name for a bound: one value threaded into a
/// bounded function, carrying the trait's impls for that type parameter.
pub fn dict_param_name(trait_name: &str, type_param: &str) -> String {
    format!("__dict_{trait_name}_{type_param}")
}

/// The same name when the trait is applied: `T: Into<String>` is not
/// `T: Into<i64>`, so each application has its own parameter.
pub fn dict_param_name_for(trait_name: &str, arg_key: &str, type_param: &str) -> String {
    if arg_key.is_empty() {
        dict_param_name(trait_name, type_param)
    } else {
        format!("__dict_{trait_name}_{arg_key}_{type_param}")
    }
}

/// The global dictionary name for a concrete `(trait, type key)`.
pub fn dict_global_name(trait_name: &str, key: &str) -> String {
    format!("__dict_{trait_name}_{key}")
}

/// Project method `index` from a dictionary. A single-method trait's
/// dictionary is its one impl, so the dictionary *is* the method; a
/// multi-method dictionary is a right-nested tuple of impls, projected by
/// binding all `count` components and returning the `index`-th.
fn dict_projection(dict_var: &str, index: usize, count: usize) -> Term {
    if count <= 1 {
        return Term::Var(dict_var.to_string());
    }
    let binders: Vec<String> = (0..count).map(|i| format!("__d{i}")).collect();
    let chosen = binders[index].clone();
    Term::Mu(
        "__dp".into(),
        Box::new(Command::Cut(
            Term::Var(dict_var.to_string()),
            CoTerm::MuTildeTensor(
                binders,
                Box::new(Command::Cut(Term::Var(chosen), CoTerm::Covar("__dp".into()))),
            ),
        )),
    )
}

/// Wrap a bounded declaration's body in its dictionary parameters, outermost
/// and in declared-bound order, so a call supplies them before the value
/// arguments.
fn bind_dict_params(bounds: &[crate::ast::TraitBound], mut term: Term) -> Term {
    for bound in bounds.iter().rev() {
        let name = dict_param_name_for(
            &bound.trait_name,
            &crate::traits::rendered_args(&bound.args),
            &bound.param,
        );
        term = Term::Lam(name, Box::new(term));
    }
    term
}

/// The fully qualified label a variant path or unambiguous variant name
/// denotes, if it names a declared enum variant.
fn lookup_variant(name: &str) -> Option<String> {
    VARIANTS.with(|cell| cell.borrow().get(name).cloned().flatten())
}

/// How the checker resolved the trait-method call at `span`, if it is one.
fn method_dispatch(span: Span) -> Option<MethodDispatch> {
    METHODS.with(|cell| cell.borrow().get(&span).cloned())
}

/// The dictionary arguments a call at `span` must pass, if it calls a
/// bounded function.
fn call_dicts(span: Span) -> Option<Vec<DictExpr>> {
    CALLS.with(|cell| cell.borrow().get(&span).cloned())
}

/// How many of this call's arguments are values, when the checker resolved
/// the callee and found a menu of exits after them.
fn call_groups(span: Span) -> Option<usize> {
    CALL_GROUPS.with(|cell| cell.borrow().get(&span).copied())
}

/// The term a dictionary argument lowers to: the named dictionary, applied
/// to its constructor arguments when the impl behind it is bounded.
fn dict_term(dict: &DictExpr) -> Term {
    let base = Term::Var(dict.name.clone());
    if dict.args.is_empty() {
        return base;
    }
    let args: Vec<Term> = dict.args.iter().map(dict_term).collect();
    if dict.methods <= 1 {
        return call_curried(base, args);
    }
    Term::Tuple(
        (0..dict.methods)
            .map(|index| {
                call_curried(dict_projection(&dict.name, index, dict.methods), args.clone())
            })
            .collect(),
    )
}

/// The component index the checker resolved for a projection at `span`.
fn projection(span: Span) -> Option<Projection> {
    PROJECTIONS.with(|cell| cell.borrow().get(&span).cloned())
}

/// Whether the checker found the computation at `span` to be delayed.
fn is_delayed(span: Span) -> bool {
    DELAYS.with(|cell| cell.borrow().contains(&span))
}

/// Lower an expression standing as a command — a `of` or `mu` arm, a
/// block's statement or its last expression. A name of type `(;)` there is
/// demanded: it is run, by applying what it holds to the unit, so a delayed
/// exit jumps where it is taken. Anywhere else it is passed on unrun.
fn lower_in_command_position(e: &Node<Expr>, continuations: &[String]) -> Result<Term, LowerError> {
    let term = lower_expr(e, continuations)?;
    let runs =
        matches!(e.kind, Expr::Ident(_)) && RUNS.with(|cell| cell.borrow().contains(&e.span));
    Ok(if runs { call_curried(term, vec![Term::Var("$unit".into())]) } else { term })
}

/// Lower an expression standing in a by-name position: a computation the
/// checker found negative is delayed, to run where it is demanded.
fn lower_by_name(e: &Node<Expr>, continuations: &[String]) -> Result<Term, LowerError> {
    let term = lower_expr(e, continuations)?;
    Ok(delay_if_delayed(e.span, term))
}

/// `term`, delayed when the checker found the computation at `span` to be.
fn delay_if_delayed(span: Span, term: Term) -> Term {
    if is_delayed(span) {
        Term::Lam(slc_core::term::DELAY_BINDER.into(), Box::new(term))
    } else {
        term
    }
}

/// Which components of the form value at `span` are positive.
fn par_polarities(span: Span) -> Option<Vec<bool>> {
    PARS.with(|cell| cell.borrow().get(&span).cloned())
}

/// The qualified destructor label a destructor name denotes, if it names a
/// declared menu item unambiguously (or is already qualified).
fn lookup_dtor(name: &str) -> Option<String> {
    MENUS.with(|cell| cell.borrow().get(name).cloned().flatten())
}

/// The qualified destructor label the checker resolved for the demand at
/// `span`, if `base.item` demands a menu rather than projecting a struct.
fn demand(span: Span) -> Option<String> {
    DEMANDS.with(|cell| cell.borrow().get(&span).cloned())
}

/// Lower a program with the checker's dispatch resolution in force, so that
/// trait-method calls become direct calls or dictionary projections and
/// bounded functions take and forward their dictionaries.
pub fn lower_program_resolving(
    p: &Program,
    dispatch: &DispatchInfo,
) -> Result<Vec<(String, Term)>, LowerError> {
    METHODS.with(|cell| *cell.borrow_mut() = dispatch.methods.clone());
    CALLS.with(|cell| *cell.borrow_mut() = dispatch.calls.clone());
    PROJECTIONS.with(|cell| *cell.borrow_mut() = dispatch.projections.clone());
    DEMANDS.with(|cell| *cell.borrow_mut() = dispatch.demands.clone());
    CALL_GROUPS.with(|cell| *cell.borrow_mut() = dispatch.call_groups.clone());
    FLOWS.with(|cell| *cell.borrow_mut() = dispatch.flows.clone());
    ELABORATED.with(|cell| *cell.borrow_mut() = dispatch.elaborated.clone());
    SWAPS.with(|cell| *cell.borrow_mut() = dispatch.swaps.clone());
    PARS.with(|cell| *cell.borrow_mut() = dispatch.pars.clone());
    DELAYS.with(|cell| *cell.borrow_mut() = dispatch.delays.clone());
    RUNS.with(|cell| *cell.borrow_mut() = dispatch.runs.clone());
    let result = lower_program(p).map(|mut definitions| {
        definitions.extend(
            dispatch
                .adapters
                .iter()
                .enumerate()
                .map(|(index, adapter)| (adapter_name(index), adapter_definition(adapter))),
        );
        definitions
    });
    METHODS.with(|cell| cell.borrow_mut().clear());
    CALLS.with(|cell| cell.borrow_mut().clear());
    PROJECTIONS.with(|cell| cell.borrow_mut().clear());
    DEMANDS.with(|cell| cell.borrow_mut().clear());
    CALL_GROUPS.with(|cell| cell.borrow_mut().clear());
    FLOWS.with(|cell| cell.borrow_mut().clear());
    ELABORATED.with(|cell| cell.borrow_mut().clear());
    SWAPS.with(|cell| cell.borrow_mut().clear());
    PARS.with(|cell| cell.borrow_mut().clear());
    DELAYS.with(|cell| cell.borrow_mut().clear());
    RUNS.with(|cell| cell.borrow_mut().clear());
    result
}

#[derive(Debug, Clone, PartialEq)]
pub enum LowerError {
    UnknownType(String),
    Unsupported(String),
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LowerError::UnknownType(s) => write!(f, "unknown type: {s}"),
            LowerError::Unsupported(s) => write!(f, "unsupported construct: {s}"),
        }
    }
}

impl std::error::Error for LowerError {}

pub fn lower_type(t: &TypeExpr) -> Result<Type, LowerError> {
    match t {
        // A declaration applied to arguments resolves against the checker's
        // declaration table, not here.
        TypeExpr::Apply(name, _) => Err(LowerError::UnknownType(name.clone())),
        TypeExpr::Base(s) => match s.as_str() {
            "i32" => Ok(Type::Pos(Base::I32)),
            "i8" => Ok(Type::Pos(Base::I8)),
            "i64" => Ok(Type::Pos(Base::I64)),
            "u8" => Ok(Type::Pos(Base::U8)),
            "u32" => Ok(Type::Pos(Base::U32)),
            "u64" => Ok(Type::Pos(Base::U64)),
            "f32" => Ok(Type::Pos(Base::F32)),
            "f64" => Ok(Type::Pos(Base::F64)),
            "String" | "str" => Ok(Type::Pos(Base::Str)),
            "char" => Ok(Type::Pos(Base::Char)),
            "unit" => Ok(Type::ONE),
            "File" => Ok(Type::Pos(Base::File)),
            other => Err(LowerError::UnknownType(other.to_string())),
        },
        // `-(;)` is not the dual of `(;)`; it is the impossible command
        // type. It is negative already.
        TypeExpr::Negative(inner) if inner.kind.is_bottom() => Ok(Type::BOTTOM),
        TypeExpr::Positive(inner) => Ok(lower_type(&inner.kind)?.dual().dual()),
        TypeExpr::Negative(inner) => Ok(lower_type(&inner.kind)?.dual()),
        TypeExpr::Tensor(items) => Ok(Type::Tensor(lower_types(items)?)),
        TypeExpr::Par(items) => Ok(Type::Par(lower_types(items)?)),
        TypeExpr::With(items) => Ok(Type::With(lower_types(items)?)),
        TypeExpr::Sum(items) => Ok(Type::Sum(lower_types(items)?)),
        // `A -> B` is `(dual(A) ; B)`, so a function is negative and `(A -> (;))` is
        // `-A`: a function that never returns is a consumer of its argument.
        TypeExpr::Fun(a, b) => Ok(Type::arrow(lower_type(&a.kind)?, lower_type(&b.kind)?)),
        // The effect row is the effect checker's concern; the core type is
        // the arrow underneath.
        TypeExpr::Effectful(inner, _) => lower_type(&inner.kind),
        // A row argument is the effect checker's alone.
        TypeExpr::Row(_) => Ok(Type::ONE),
        // `dual(A)` applies the involution rather than wrapping a node, so
        // `dual(+i64)` is `-i64` and `dual(dual(A))` is `A`. Only a
        // declaration's name stays wrapped: it is opaque to the core.
        TypeExpr::Dual(inner) => Ok(lower_type(&inner.kind)?.dual()),
    }
}

fn lower_types(items: &[Node<TypeExpr>]) -> Result<Vec<Type>, LowerError> {
    items.iter().map(|item| lower_type(&item.kind)).collect()
}

/// Lower an expression to a core term.
/// Lower an expression in an explicit lexical continuation scope. New
/// continuation binders extend `continuations` for their body only.
/// Lower an expression, turned around first if the checker found its value
/// used at the mirrored `;` spelling of its type.
fn lower_expr(e: &Node<Expr>, continuations: &[String]) -> Result<Term, LowerError> {
    let elaborated = ELABORATED.with(|cell| cell.borrow().get(&e.span).cloned());
    let term = lower_expr_facing(elaborated.as_ref().unwrap_or(e), continuations)?;
    Ok(match SWAPS.with(|cell| cell.borrow().get(&e.span).copied()) {
        Some(adapter) => adapt_term(term, adapter),
        None => term,
    })
}

fn lower_expr_facing(e: &Node<Expr>, continuations: &[String]) -> Result<Term, LowerError> {
    match &e.kind {
        Expr::Int(n) => Ok(Term::Var(format!("$int_{n}"))),
        Expr::Float(n) => Ok(Term::Var(format!("$float_{n}"))),
        Expr::Str(s) => Ok(Term::Var(format!("$str_{s:?}"))),
        Expr::Char(c) => Ok(Term::Var(format!("$char_{c}"))),
        Expr::Ident(s) => {
            // An unambiguous bare variant resolves to its label, the way
            // the checker resolves it — `None` is `Option::None`. A variant
            // path resolves to the global the enum declaration installs;
            // any other identifier stays a variable.
            match lookup_variant(s) {
                Some(label) => Ok(Term::Var(label)),
                None => Ok(Term::Var(s.clone())),
            }
        }

        Expr::Lambda { param, param_type: _, return_type: _, body } => {
            // fn(x) { body } → λx. body'. An ordinary lambda opens a fresh
            // lexical continuation scope: only its own local mu/select
            // binders are visible in its body.
            let b = lower_expr(body, &[])?;
            Ok(Term::Lam(param.clone(), Box::new(b)))
        }

        Expr::Call { callee, args } => {
            // An enum constructor is not an application: `E::V(a)` is the
            // labelled injection `E::V(a)`. Several payload values are packed
            // into one right-nested tensor, so a variant always carries
            // exactly one payload term.
            if let Expr::Ident(name) = &callee.kind
                && let Some(label) = lookup_variant(name)
            {
                let payload = args
                    .iter()
                    .map(|arg| lower_by_name(arg, continuations))
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(Term::Tag(label, Box::new(pack_group(payload))));
            }
            // f(a, b) lowers to nested single-argument applications:
            //   f(a) applied to (b)
            // Multi-arg functions are curried: fn f(x, y) → λx. λy. body.
            // A trait-method call resolves to a direct impl call (concrete
            // receiver) or a projection from a dictionary parameter (bounded
            // receiver) — never a runtime method value.
            let mut result = match &callee.kind {
                Expr::Ident(_) => match method_dispatch(e.span) {
                    Some(MethodDispatch::Static(mangled)) => Term::Var(mangled),
                    Some(MethodDispatch::Dict { dict_var, index, count }) => {
                        dict_projection(&dict_var, index, count)
                    }
                    None => lower_expr(callee, continuations)?,
                },
                _ => lower_expr(callee, continuations)?,
            };
            // A call to a bounded function forwards its dictionaries first,
            // in bound order, then the value arguments.
            let mut call_args: Vec<Term> =
                call_dicts(e.span).unwrap_or_default().iter().map(dict_term).collect();
            let mut lowered = Vec::new();
            for arg in args {
                lowered.push(lower_by_name(arg, continuations)?);
            }
            // The arguments group as the callee's parameters do: the value
            // product, then the menu of exits. Each packs into one argument,
            // and a call with no values still passes unit.
            match call_groups(e.span) {
                // The callee has a menu of exits, so it binds that group
                // separately — and binds a value group only if it declared
                // one, which a negative function does not.
                Some(values) => {
                    let row = lowered.split_off(values.min(lowered.len()));
                    if !lowered.is_empty() {
                        call_args.push(pack_group(lowered));
                    }
                    call_args.push(pack_group(row));
                }
                // One group: the values, or the unit a call with none passes.
                None => call_args.push(pack_group(lowered)),
            }
            for arg in call_args {
                result = Term::Mu(
                    "__call".into(),
                    Box::new(Command::Cut(
                        result,
                        CoTerm::App(arg, Box::new(CoTerm::Covar("__call".into()))),
                    )),
                );
            }
            Ok(result)
        }

        // `(k1 & k2 & …)` — a bundle of exits: a tuple of them, and taking an
        // exit is projecting a component; only the checker tells `&` from `,`.
        Expr::Bundle(items) => Ok(Term::Tuple(
            items
                .iter()
                .map(|item| lower_by_name(item, continuations))
                .collect::<Result<_, _>>()?,
        )),
        // `(k1 ; k2)` → co(μ̃(x1, x2). ⟨x1 ∥ k1⟩; ⟨x2 ∥ k2⟩): the consumer of the
        // product its continuations want, handing each its part left to right
        // as a block runs its statements — so a part sent to an exit that
        // jumps is the last part sent.
        Expr::Par(items) => {
            let positives = par_polarities(e.span).ok_or_else(|| {
                LowerError::Unsupported("a form value was not resolved by the checker".into())
            })?;
            let parts: Vec<String> = (0..items.len()).map(|i| format!("__part{i}")).collect();
            let mut commands = Vec::new();
            for ((item, positive), part) in items.iter().zip(positives).zip(&parts) {
                let wanting = lower_expr(item, continuations)?;
                commands.push(if positive {
                    // A value: the part is its consumer.
                    Command::Cut(wanting, CoTerm::Covar(part.clone()))
                } else {
                    // A consumer: it takes the part.
                    let consumer = format!("{part}_consumer");
                    Command::Cut(
                        wanting,
                        CoTerm::MuTilde(
                            consumer.clone(),
                            Box::new(Command::Cut(
                                Term::Var(part.clone()),
                                CoTerm::Covar(consumer),
                            )),
                        ),
                    )
                });
            }
            let mut command = commands.pop().expect("a form value has at least two components");
            while let Some(first) = commands.pop() {
                command = Command::Cut(
                    Term::Mu("__part_seq".into(), Box::new(first)),
                    CoTerm::MuTilde("__discarded".into(), Box::new(command)),
                );
            }
            Ok(Term::Co(Box::new(CoTerm::MuTildeTensor(parts, Box::new(command)))))
        }
        // `::i(v)`: the alternative at position `i`, labelled by it alone.
        Expr::Inject { index, value } => {
            Ok(Term::Tag(alternative_label(*index), Box::new(lower_by_name(value, continuations)?)))
        }
        Expr::Pair(items) => Ok(pack_group(
            items
                .iter()
                .map(|item| lower_by_name(item, continuations))
                .collect::<Result<_, _>>()?,
        )),

        Expr::Let { pattern, value, body, mode, .. } => {
            let b = body
                .as_ref()
                .map(|b| lower_expr(b, continuations))
                .transpose()?
                .unwrap_or_else(|| Term::Var("$unit".into()));
            lower_binding(pattern, value, b, *mode, continuations)
        }

        // `a | b | c` — everything flows left to right. A chain that does
        // not begin with a value denotes one that would: `f | k` is
        // `λx. x | f | k`, so eta-expanding leaves every middle step an
        // ordinary application and the last one either an application or
        // the cut.
        Expr::Flow { stages, from_value, into_consumer } => {
            // Without `<` a function heads the chain, and the chain denotes
            // one that would take a value — `f | k>` is `λx. <x | f | k>` —
            // so eta-expanding leaves every middle step an application.
            let shape =
                FLOWS.with(|cell| cell.borrow().get(&e.span).cloned()).unwrap_or(FlowShape {
                    eta: !*from_value,
                    cut: *into_consumer,
                    commuted: Vec::new(),
                    row_stage: None,
                    yielding: None,
                    swap: None,
                    turned: Vec::new(),
                });
            let mut lowered = Vec::new();
            if shape.eta {
                lowered.push(Term::Var(FLOW_ARGUMENT.into()));
            }
            for (index, stage) in stages.iter().enumerate() {
                let term = lower_flow_stage(stage, continuations)?;
                // What flows in stands by name.
                lowered.push(
                    if index == 0 || (shape.yielding.is_some() && index + 1 == stages.len()) {
                        delay_if_delayed(stage.span, term)
                    } else {
                        term
                    },
                );
            }
            // A command takes both its groups from the chain: what flowed
            // in that far is its values, and the closing stage its menu of
            // exits. That is a call, not a cut — a partially applied
            // command is a closure, whatever its type says.
            // Every step folds left. A stage read the other way round builds
            // a consumer from the continuation of its step, and what flowed in
            // that far is fed to it; any other stage is applied.
            let commuted: Vec<usize> =
                shape.commuted.iter().map(|i| i + usize::from(shape.eta)).collect();
            // A result the next stage takes at the other spelling of its type
            // is turned around between the two steps.
            let turned: Vec<(usize, usize)> =
                shape.turned.iter().map(|(i, swap)| (i + usize::from(shape.eta), *swap)).collect();
            let turn = |at: usize, acc: Term| match turned.iter().find(|(i, _)| *i == at) {
                Some((_, adapter)) => adapt_term(acc, *adapter),
                None => acc,
            };
            let fold = |steps: Vec<Term>| {
                let mut steps = steps.into_iter().enumerate();
                let (first, mut acc) = steps.next().expect("a flow has a first stage");
                acc = turn(first, acc);
                for (at, stage) in steps {
                    acc = flow_step(stage, acc, commuted.contains(&at).then_some(at));
                    acc = turn(at, acc);
                }
                acc
            };
            if let Some(at) = shape.row_stage.map(|i| i + usize::from(shape.eta)) {
                let row = lowered.pop().expect("a row stage has a closing menu");
                let callee = lowered.remove(at);
                // The stages before the command fold as any chain's do; the
                // command is the last step and closes on its menu.
                let values = fold(lowered);
                let term = match shape.yielding {
                    Some(count) => yielding_command(callee, values, row, count),
                    None => call_curried(callee, vec![values, row]),
                };
                return Ok(if shape.eta {
                    Term::Lam(FLOW_ARGUMENT.into(), Box::new(term))
                } else {
                    term
                });
            }
            let last = shape.cut.then(|| lowered.pop().expect("a closed flow has a consumer"));
            let acc = fold(lowered);
            let term = match last {
                None => acc,
                Some(last) => {
                    // The closed chain is a cut, and lowers to exactly the
                    // term a cut has always lowered to.
                    let closing = &stages.last().expect("a flow has stages").kind;
                    // A value used at the mirrored spelling of its type is
                    // turned to face the consumer before it meets it.
                    let acc = match shape.swap {
                        Some(adapter) => adapt_term(acc, adapter),
                        None => acc,
                    };
                    let command = match named_consumer(closing) {
                        Some(name) => Command::Cut(acc, CoTerm::Covar(name.clone())),
                        None => Command::Cut(
                            acc,
                            CoTerm::MuTilde(
                                "$cut_value".into(),
                                Box::new(Command::Cut(
                                    last,
                                    CoTerm::App(
                                        Term::Var("$cut_value".into()),
                                        Box::new(CoTerm::Covar("__tail".into())),
                                    ),
                                )),
                            ),
                        ),
                    };
                    Term::Mu(cut_binder(closing), Box::new(command))
                }
            };
            Ok(if shape.eta { Term::Lam(FLOW_ARGUMENT.into(), Box::new(term)) } else { term })
        }
        Expr::Mu { continuation_params, body, .. } => {
            let mut body_scope = continuations.to_vec();
            body_scope.extend(continuation_params.iter().map(continuation_name));
            let mut term = lower_expr(body, &body_scope)?;
            for p in continuation_params.iter().rev() {
                let name = continuation_name(p);
                term = Term::Mu(name.clone(), Box::new(Command::Cut(term, CoTerm::Covar(name))));
            }
            Ok(term)
        }
        // A shift is a coercion the checker cares about and the core does
        // not: a boxed consumer and the consumer are the same value.
        // `base.i` / `base.field` → μ. ⟨ ⟦base⟧ ∥ prj:index ⟩. The checker
        // resolved the component index from the base's type; the μ binder is
        // vestigial — the projected component returns to the ambient
        // continuation, as a match dispatch does.
        Expr::Project { base, .. } => {
            // `cfg.item` on a menu is a demand: cut the menu against the
            // request, with the μ binder as the answer's continuation.
            if let Some(label) = demand(e.span) {
                let base = lower_expr(base, continuations)?;
                return Ok(Term::Mu(
                    "__ask".into(),
                    Box::new(Command::Cut(
                        base,
                        CoTerm::Dtor(label, Box::new(CoTerm::Covar("__ask".into()))),
                    )),
                ));
            }
            let Projection { index, arity, record } = projection(e.span).ok_or_else(|| {
                LowerError::Unsupported("a projection was not resolved by the checker".into())
            })?;
            let base = lower_expr(base, continuations)?;
            Ok(match record {
                // A record's fields are bound under its label, one binder each
                // — so a record of one field is read the same way.
                Some(record) => {
                    let fields: Vec<String> = (0..arity).map(|i| format!("__field{i}")).collect();
                    let chosen = Term::Var(fields[index].clone());
                    Term::Mu(
                        "__prj".into(),
                        Box::new(Command::Cut(
                            base,
                            CoTerm::CoCase {
                                owner: record.clone(),
                                branches: vec![CoCaseBranch {
                                    label: record,
                                    binders: fields,
                                    body: Box::new(Command::Cut(
                                        chosen,
                                        CoTerm::Covar("__prj".into()),
                                    )),
                                }],
                            },
                        )),
                    )
                }
                // A product of one is its component.
                None if arity == 1 => base,
                None => Term::Mu("__prj".into(), Box::new(Command::Cut(base, CoTerm::Prj(index)))),
            })
        }
        // `.item(k)` — a request literal: the continuation boxed under its
        // destructor. A named continuation is a co-variable directly; any
        // other expression is bound first, then named.
        Expr::Request { dtor, arg } => {
            let label = lookup_dtor(dtor).ok_or_else(|| {
                LowerError::Unsupported(format!("`.{dtor}` does not name a declared menu item"))
            })?;
            if let Expr::Ident(name) = &arg.kind {
                return Ok(Term::Co(Box::new(CoTerm::Dtor(
                    label,
                    Box::new(CoTerm::Covar(name.clone())),
                ))));
            }
            let payload = lower_expr(arg, continuations)?;
            Ok(Term::Mu(
                "__req".into(),
                Box::new(Command::Cut(
                    payload,
                    CoTerm::MuTilde(
                        "__k".into(),
                        Box::new(Command::Cut(
                            Term::Co(Box::new(CoTerm::Dtor(
                                label,
                                Box::new(CoTerm::Covar("__k".into())),
                            ))),
                            CoTerm::Covar("__req".into()),
                        )),
                    ),
                )),
            ))
        }
        // `handle` lowers to a `__handle` call the runtime special-cases: the
        // effect name, a value encoding the clauses, and a thunk of the body.
        Expr::Handle { clauses, ret, .. } | Expr::Handler { clauses, ret, .. } => {
            // Each clause → ($str_op ⊗ λpayload. λresume. body): performing
            // an operation is a call, so its arguments arrive packed, and a
            // clause of several parameters destructures them. A nullary
            // operation still takes one ignored binder, for the unit its
            // caller passes.
            let mut entries = Vec::new();
            for clause in clauses {
                let mut body_scope = continuations.to_vec();
                body_scope.push(clause.resume.clone());
                body_scope.extend(clause.params.iter().cloned());
                let inner = lower_expr(&clause.body, &body_scope)?;
                let answered = Term::Lam(clause.resume.clone(), Box::new(inner));
                let closure = if clause.params.is_empty() {
                    Term::Lam("__op_arg".into(), Box::new(answered))
                } else {
                    bind_names(&clause.params, "op", answered)
                };
                entries
                    .push(Term::Tuple(vec![Term::Var(format!("$str_\"{}\"", clause.op)), closure]));
            }
            // The return clause, or the identity.
            let ret_closure = match ret {
                Some((binder, rbody)) => {
                    Term::Lam(binder.clone(), Box::new(lower_expr(rbody, continuations)?))
                }
                None => Term::Lam("__ret".into(), Box::new(Term::Var("__ret".into()))),
            };
            entries.push(Term::Tuple(vec![Term::Var("$str_\"return\"".into()), ret_closure]));
            let encoded = Term::Tuple(entries);
            let encoded = Term::Tag("__clauses".into(), Box::new(encoded));
            let Expr::Handle { body, .. } = &e.kind else {
                return Ok(encoded);
            };
            // The body stands as a command when it is one: a program of type
            // `(;)` handed in by name runs under the handler.
            let body_thunk = Term::Lam(
                "__handle_thunk".into(),
                Box::new(lower_in_command_position(body, continuations)?),
            );
            // The clause tree is wrapped so the runtime's argument collection,
            // which flattens pairs, passes it as one value.
            Ok(call_curried(Term::Var("__handle".into()), vec![encoded, body_thunk]))
        }
        Expr::WithHandler { handler, body } => Ok(call_curried(
            Term::Var("__handle".into()),
            vec![
                lower_expr(handler, continuations)?,
                Term::Lam(
                    "$handler_body".into(),
                    Box::new(lower_in_command_position(body, continuations)?),
                ),
            ],
        )),
        Expr::Match { scrutinee, arms } => {
            // A match the core can express lowers to a genuine cut against
            // its branch table — μ̃[…], μ̃(x…), or μ̃x — with each arm's value
            // delivered to the match's own continuation. Literals, or-patterns,
            // defaults among labelled arms, and everything else order-sensitive
            // falls through to the dispatch builtin below.
            if let Some(term) = lower_match_canonical(scrutinee, arms, continuations)? {
                return Ok(term);
            }
            // match s { p1 => e1, p2 => e2, ... }
            // → μmatch. ⟨ __match_dispatch(s', arms...) ∥ match ⟩
            // The dispatch builtin evaluates the scrutinee and selects
            // the arm whose pattern matches, applying it as a thunk.
            let s = lower_expr(scrutinee, continuations)?;
            // Encode arms as thunks: one closure per arm.
            // Each arm is a Lam so it is only evaluated when selected.
            let mut arm_terms = Vec::new();
            for arm in arms {
                let descriptor = Term::Var(format!("$str_{}", pattern_descriptor(&arm.pattern)));
                let b = lower_in_command_position(&arm.body, continuations)?;
                arm_terms.push(Term::Tag(
                    "__match_arm".into(),
                    Box::new(Term::Tuple(vec![
                        descriptor,
                        Term::Lam("__match_arg".into(), Box::new(b)),
                    ])),
                ));
            }
            // One tuple: the scrutinee, then the arms in order.
            let payload = Term::Tuple(std::iter::once(s).chain(arm_terms).collect());
            Ok(Term::Mu(
                "__match".into(),
                Box::new(Command::Cut(
                    Term::Var("__match_dispatch".into()),
                    CoTerm::App(payload, Box::new(CoTerm::Covar("__match".into()))),
                )),
            ))
        }

        Expr::Data { name, fields } => {
            // A record value is a labelled product: the declaration's name
            // tags the right-nested tensor of its field values. An `enum`
            // variant is the same shape with a different label, so one core
            // form covers both.
            let payload = fields
                .iter()
                .map(|(_, value)| lower_by_name(value, continuations))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Term::Tag(name.clone(), Box::new(pack_group(payload))))
        }

        // `mu T { item: k <= c, … }` — the copattern form: a menu value,
        // μ[…], one branch per demand. Arms may refine an item with nested
        // copatterns — `tail: head: out` — which group by their outer
        // destructor into an inner menu.
        Expr::CoMatch { ty, arms } => {
            let qualifier = ty.as_ref().and_then(|ty| match &ty.kind {
                TypeExpr::Base(name) | TypeExpr::Apply(name, _) => Some(name.as_str()),
                TypeExpr::Sum(items) if items.is_empty() => Some(EMPTY_SUM),
                TypeExpr::With(items) if items.is_empty() => Some(EMPTY_MENU),
                _ => None,
            });
            let rows: Vec<(&Pattern, &Node<Expr>)> =
                arms.iter().map(|arm| (&arm.pattern, &arm.command)).collect();
            lower_comatch(qualifier, rows, continuations, 0)
        }

        Expr::Select { ty, arms } => {
            // `select T { p => c, … }` is the consumer of T, given by cases
            // on it: one branch per shape, binding that shape's components.
            //
            //   labelled (enum, struct) ⟹ co(μ̃[T; L(x…). c | … ])
            //   product (tensor)        ⟹ co(μ̃(x…). c)
            // Request arms belong to `mu`: `mu` answers data.
            if arms.iter().any(|arm| matches!(arm.pattern, Pattern::Dtor { .. })) {
                return Err(LowerError::Unsupported(
                    "`mu` answers data; a menu answers demands and is built by \
                     `mu Menu { item: k <= c, … }`"
                        .into(),
                ));
            }
            // A sum's alternatives, by position: `::0(x)`, `::1(y)`, each its
            // own label.
            if arms.iter().any(|arm| matches!(arm.pattern, Pattern::Inject { .. })) {
                let mut branches = Vec::new();
                for arm in arms {
                    let Pattern::Inject { index, pattern } = &arm.pattern else {
                        return Err(LowerError::Unsupported(
                            "a `mu` over a sum covers its alternatives, `::0(x)` and \
                             `::1(y)`, and nothing else"
                                .into(),
                        ));
                    };
                    let command = lower_select_command(&arm.command, continuations)?;
                    let (binders, command) =
                        components(std::iter::once(pattern.as_ref()), command)?;
                    branches.push(CoCaseBranch {
                        label: alternative_label(*index),
                        binders,
                        body: Box::new(command),
                    });
                }
                return Ok(Term::Co(Box::new(CoTerm::CoCase {
                    owner: EMPTY_SUM.into(),
                    branches,
                })));
            }
            let qualifier = ty.as_ref().and_then(|ty| match &ty.kind {
                TypeExpr::Base(name) | TypeExpr::Apply(name, _) => Some(name.as_str()),
                TypeExpr::Sum(items) if items.is_empty() => Some(EMPTY_SUM),
                TypeExpr::With(items) if items.is_empty() => Some(EMPTY_MENU),
                _ => None,
            });
            let mut branches = Vec::new();
            let mut product: Option<(Vec<String>, Command)> = None;
            for arm in arms {
                let command = lower_select_command(&arm.command, continuations)?;
                let (label, binders, command) = select_arm_shape(&arm.pattern, command)?;
                match label {
                    Some(label) => {
                        branches.push(CoCaseBranch { label, binders, body: Box::new(command) })
                    }
                    None => product = Some((binders, command)),
                }
            }
            match (branches.is_empty(), product) {
                (true, Some((binders, command))) => {
                    // One binder takes the whole value: that is `μ̃x. c`.
                    let consumer = match <[String; 1]>::try_from(binders) {
                        Ok([binder]) => CoTerm::MuTilde(binder, Box::new(command)),
                        Err(binders) => CoTerm::MuTildeTensor(binders, Box::new(command)),
                    };
                    Ok(Term::Co(Box::new(consumer)))
                }
                (false, None) => Ok(Term::Co(Box::new(lower_cocase(qualifier, branches)?))),
                (true, None) if qualifier.is_some() => {
                    Ok(Term::Co(Box::new(lower_cocase(qualifier, branches)?)))
                }
                _ => Err(LowerError::Unsupported(
                    "a `mu` covers either a labelled type or one product, not both".into(),
                )),
            }
        }

        Expr::Block(exprs) => {
            // A block evaluates expressions in order. A bodyless let scopes
            // over the rest of the block, so lower the rest inside its scope.
            fn lower_block(
                exprs: &[Node<Expr>],
                index: usize,
                seq_counter: &mut usize,
                continuations: &[String],
            ) -> Result<Term, LowerError> {
                let Some(e) = exprs.get(index) else {
                    return Ok(Term::Var("$unit".into()));
                };

                if let Expr::Let { pattern, value, body: None, mode, .. } = &e.kind {
                    // A bodyless `let` scopes over the rest of the block, so
                    // the rest is lowered as its body. Both `let` forms use
                    // the same binding lowering.
                    let rest = lower_block(exprs, index + 1, seq_counter, continuations)?;
                    return lower_binding(pattern, value, rest, *mode, continuations);
                }

                let rest = lower_block(exprs, index + 1, seq_counter, continuations)?;
                if index + 1 == exprs.len() {
                    return lower_in_command_position(e, continuations);
                }
                let t = lower_in_command_position(e, continuations)?;
                let mut used = slc_core::substitution::free_vars_term(&t);
                used.extend(slc_core::substitution::free_vars_term(&rest));
                let seq_name =
                    slc_core::substitution::fresh(&format!("__seq{}", *seq_counter), &mut used);
                let discarded = slc_core::substitution::fresh("__discarded", &mut used);
                *seq_counter += 1;
                Ok(Term::Mu(
                    seq_name.clone(),
                    Box::new(Command::Cut(
                        t,
                        CoTerm::MuTilde(
                            discarded,
                            Box::new(Command::Cut(rest, CoTerm::Covar(seq_name))),
                        ),
                    )),
                ))
            }

            let mut seq_counter = 0;
            lower_block(exprs, 0, &mut seq_counter, continuations)
        }
    }
}

pub fn lower_program(p: &Program) -> Result<Vec<(String, Term)>, LowerError> {
    let mut variant_labels: HashMap<String, Option<String>> = HashMap::new();
    for d in &p.decls {
        if let Decl::Enum { name, variants, .. } = &d.kind {
            for (variant, _) in variants {
                let label = format!("{name}::{variant}");
                variant_labels.insert(label.clone(), Some(label.clone()));
                // An unqualified name is usable only while it is unambiguous.
                variant_labels
                    .entry(variant.clone())
                    .and_modify(|existing| *existing = None)
                    .or_insert(Some(label));
            }
        }
    }
    VARIANTS.with(|cell| {
        *cell.borrow_mut() = variant_labels;
    });
    let mut dtor_labels: HashMap<String, Option<String>> = HashMap::new();
    for d in &p.decls {
        if let Decl::Menu { name, items, .. } = &d.kind {
            for (item, _) in items {
                let label = format!("{name}::{item}");
                dtor_labels.insert(label.clone(), Some(label.clone()));
                // An unqualified destructor is usable only while unambiguous.
                dtor_labels
                    .entry(item.clone())
                    .and_modify(|existing| *existing = None)
                    .or_insert(Some(label));
            }
        }
    }
    MENUS.with(|cell| {
        *cell.borrow_mut() = dtor_labels;
    });

    let mut out = Vec::new();
    for d in &p.decls {
        if let Decl::Enum { name, variants, .. } = &d.kind {
            for (variant, _) in variants {
                // An enum value is a labelled additive injection. Variant
                // payloads are not yet constructed, so the payload is unit.
                out.push((
                    format!("{name}::{variant}"),
                    Term::Tag(format!("{name}::{variant}"), Box::new(Term::Var("$unit".into()))),
                ));
            }
        }
    }
    let constants: HashMap<String, Pattern> = p
        .decls
        .iter()
        .filter_map(|d| {
            if let Decl::Const { name, value, .. } = &d.kind {
                let pattern = match &value.kind {
                    Expr::Char(c) => Some(Pattern::Char(*c)),
                    Expr::Int(n) => Some(Pattern::Int(*n)),
                    Expr::Str(s) => Some(Pattern::Str(s.clone())),
                    _ => None,
                };
                pattern.map(|pattern| (name.clone(), pattern))
            } else {
                None
            }
        })
        .collect();
    CONSTANTS.with(|cell| {
        *cell.borrow_mut() = constants;
    });
    for d in &p.decls {
        match &d.kind {
            Decl::Fn { name, params, body, polarity, bounds, .. } => {
                // Multi-param fn: nest lambdas. Continuation parameters form
                // the declaration's explicit lexical continuation row.
                // Negative functions are genuine co-abstractions: each
                // continuation parameter becomes a μ binder, not a λ binder.
                let continuations: Vec<String> =
                    params.iter().filter(|p| p.is_continuation).map(continuation_name).collect();
                let mut term = lower_expr(body, &continuations)?;
                // A function's parameters are one group — a product of values
                // for `->`, a menu of exits for `<-` — so it binds one
                // argument and the body destructures it.
                term = bind_group(params, "args", term)?;
                // A positive function with no parameters is still called, so
                // it binds the unit its callers pass. A negative one produces
                // a continuation and is used by name.
                if params.is_empty() && *polarity == FunctionPolarity::Positive {
                    term = Term::Lam(NO_ARGUMENTS.into(), Box::new(term));
                }

                // A bounded function takes its dictionaries outermost, before
                // the value arguments.
                term = bind_dict_params(bounds, term);
                out.push((name.clone(), term));
            }
            Decl::Command { name, value_params, continuation_params, body, bounds, .. } => {
                // mu f(x: +A) | (k: -B) { E } → λx. μk. E
                let continuations: Vec<String> =
                    continuation_params.iter().map(continuation_name).collect();
                let mut term = lower_expr(body, &continuations)?;
                // Two groups, two binders: the product of values, then the
                // menu of exits, each destructured when it holds several.
                term = bind_group(continuation_params, "row", term)?;
                term = bind_group(value_params, "values", term)?;
                // The value group is a group even when it is empty: a caller
                // writes `(,) | retries | …`, so the unit still arrives.
                if value_params.is_empty() {
                    term = Term::Lam(NO_ARGUMENTS.into(), Box::new(term));
                }
                term = bind_dict_params(bounds, term);
                out.push((name.clone(), term));
            }
            Decl::Const { name, ty: _, value, .. } => {
                out.push((name.clone(), lower_expr(value, &[])?));
            }
            // Modules are flattened by resolution before lowering; one that
            // reaches here unresolved has nothing to lower.
            Decl::Mod { .. }
            | Decl::Use { .. }
            | Decl::Trait { .. }
            | Decl::Impl { .. }
            | Decl::Effect { .. }
            | Decl::Hand { .. } => {}
            Decl::Data { .. } | Decl::Enum { .. } | Decl::Menu { .. } | Decl::Form { .. } => {
                // Type declarations are handled by the checker, not lowering
            }
        }
    }
    Ok(out)
}

/// The μ binder that wraps a cut. The binder is never referenced — a command
/// has no result — but it must not capture the consumer's own name.
fn cut_binder(consumer: &Expr) -> String {
    match named_consumer(consumer) {
        Some(name) if name == CUT_BINDER => format!("{CUT_BINDER}_"),
        _ => CUT_BINDER.to_string(),
    }
}

const CUT_BINDER: &str = "__cut";

/// The owners the nullary additive tables retain: `mu (|) {}` and `(&)`
/// have no declaration to name them.
const EMPTY_SUM: &str = "(|)";
const EMPTY_MENU: &str = "(&)";

/// `let x = v; body` → `μlet. ⟨ v ∥ μ̃x. ⟨ body ∥ let ⟩ ⟩`.
///
/// A binder is `μ̃`, the value abstraction: it takes what the cut delivers
/// and runs the rest with it bound. `v · e` is application, and nothing else.
///
/// Both surface `let` forms — the expression form with an explicit body and
/// the bodyless form that scopes over the rest of its block — lower here.
/// A binding: `let p = v` for the value `v` and the body it scopes over. A
/// bare name is a μ̃ binder, exactly as it always was; any other pattern is
/// the one-arm `of` it abbreviates, so destructuring needs nothing the
/// core did not already have.
fn lower_binding(
    pattern: &Pattern,
    value: &Node<Expr>,
    body: Term,
    mode: crate::ast::LetMode,
    continuations: &[String],
) -> Result<Term, LowerError> {
    if let Some(name) = pattern.binder_name() {
        let value_span = value.span;
        let value = lower_expr(value, continuations)?;
        // `let-` binds the computation itself, run where it is demanded, and
        // so does a plain `let` of a negative computation.
        let delay = match mode {
            crate::ast::LetMode::Delay => true,
            crate::ast::LetMode::Follow => is_delayed(value_span),
            crate::ast::LetMode::Now => false,
        };
        let value = if delay {
            Term::Lam(slc_core::term::DELAY_BINDER.into(), Box::new(value))
        } else if mode == crate::ast::LetMode::Now {
            call_curried(Term::Var("$force".into()), vec![value])
        } else {
            value
        };
        return Ok(lower_let(name, value, body));
    }
    if matches!(pattern, Pattern::Wildcard) {
        let value = lower_expr(value, continuations)?;
        let value = if mode == crate::ast::LetMode::Now {
            call_curried(Term::Var("$force".into()), vec![value])
        } else {
            value
        };
        return Ok(lower_let(DISCARDED_BINDING, value, body));
    }
    // The one-arm match it abbreviates: the pattern's binders scope over the
    // body, so the body is the arm's own command.
    let arm = (pattern, Command::Cut(body, CoTerm::Covar(MATCH_COVAR.into())));
    let consumer = branch_table(vec![arm])?.ok_or_else(|| {
        LowerError::Unsupported("a binder pattern the core cannot express".into())
    })?;
    let value = lower_expr(value, continuations)?;
    let value = if mode == crate::ast::LetMode::Now {
        call_curried(Term::Var("$force".into()), vec![value])
    } else {
        value
    };
    Ok(Term::Mu(MATCH_COVAR.into(), Box::new(Command::Cut(value, consumer))))
}

/// The binder a `let _` introduces and never mentions.
const DISCARDED_BINDING: &str = "__discarded_binding";

fn lower_let(name: &str, value: Term, body: Term) -> Term {
    Term::Mu(
        "let".into(),
        Box::new(Command::Cut(
            value,
            CoTerm::MuTilde(
                name.to_string(),
                Box::new(Command::Cut(body, CoTerm::Covar("__tail".into()))),
            ),
        )),
    )
}

/// Wrap `body` in the λ binder a declared parameter introduces — value and
/// continuation parameters alike, since a continuation is a value.
/// Bind one parameter group as a single binder. A group of several is one
/// packed argument — a product of values, or a menu of exits — which the
/// body destructures, so a declaration takes at most one of each.
fn bind_group(params: &[Param], group: &str, body: Term) -> Result<Term, LowerError> {
    // The ordinary case: every leaf is a name, so the group is the μ̃ binder
    // it has always been.
    if let Some(names) = params.iter().map(|p| p.name().map(str::to_string)).collect() {
        let names: Vec<String> = names;
        return Ok(bind_names(&names, group, body));
    }
    // A leaf is a pattern, so the group is the tuple pattern its parameters
    // spell out, and the argument is destructured by the one-arm match that
    // pattern abbreviates.
    let pattern = match params {
        [only] => only.pattern.clone(),
        several => Pattern::Tuple(several.iter().map(|p| p.pattern.clone()).collect()),
    };
    let packed = format!("__{group}");
    let out = format!("__{group}_body");
    let consumer = branch_table(vec![(&pattern, Command::Cut(body, CoTerm::Covar(out.clone())))])?
        .ok_or_else(|| {
            LowerError::Unsupported("a parameter pattern the core cannot express".into())
        })?;
    Ok(Term::Lam(
        packed.clone(),
        Box::new(Term::Mu(out, Box::new(Command::Cut(Term::Var(packed), consumer)))),
    ))
}

/// The name a continuation parameter binds. Control leaves through a name,
/// so a continuation parameter is never a compound pattern; one written that
/// way is refused before lowering.
fn continuation_name(p: &Param) -> String {
    p.name().unwrap_or(UNUSED_BINDER).to_string()
}

/// `bind_group` over plain binder names.
fn bind_names(names: &[String], group: &str, body: Term) -> Term {
    match names {
        [] => body,
        [only] => Term::Lam(only.clone(), Box::new(body)),
        several => {
            let packed = format!("__{group}");
            let out = format!("__{group}_body");
            Term::Lam(
                packed.clone(),
                Box::new(Term::Mu(
                    out.clone(),
                    Box::new(Command::Cut(
                        Term::Var(packed),
                        CoTerm::MuTildeTensor(
                            several.to_vec(),
                            Box::new(Command::Cut(body, CoTerm::Covar(out))),
                        ),
                    )),
                )),
            )
        }
    }
}

/// Pack a call's arguments into one term per group: nothing is unit, one is
/// itself, and several are the tuple the callee destructures.
fn pack_group(mut terms: Vec<Term>) -> Term {
    match terms.len() {
        0 => Term::Var("$unit".into()),
        1 => terms.pop().expect("one term"),
        _ => Term::Tuple(terms),
    }
}

/// The shape a `mu` arm covers: the label it answers to, if it has one,
/// and the binders for that shape's components.
fn select_arm_shape(
    pattern: &Pattern,
    command: Command,
) -> Result<(Option<String>, Vec<String>, Command), LowerError> {
    match pattern {
        // `Red`: an unqualified variant written without a payload. Any other
        // name binds the whole value: a type with no structure has one shape
        // whose single component is the value itself.
        Pattern::Ident(name) => match lookup_variant(name) {
            Some(label) => Ok((Some(label), Vec::new(), command)),
            None => Ok((None, vec![name.clone()], command)),
        },
        Pattern::Wildcard => Ok((None, vec![UNUSED_BINDER.to_string()], command)),
        // `Color::Red(x)` or `Red(x)`.
        Pattern::Enum { name, variant, fields } => {
            let written =
                if variant.is_empty() { name.clone() } else { format!("{name}::{variant}") };
            let label = lookup_variant(&written).unwrap_or(written);
            let (binders, command) = components(fields.iter(), command)?;
            Ok((Some(label), binders, command))
        }
        // `S { left: a, right: b }`: a record is a labelled product.
        Pattern::Data { name, fields } => {
            let (binders, command) = components(fields.iter().map(|(_, p)| p), command)?;
            Ok((Some(name.clone()), binders, command))
        }
        // `(a, b)`: an unlabelled product.
        Pattern::Tuple(items) => {
            let (binders, command) = components(items.iter(), command)?;
            Ok((None, binders, command))
        }
        Pattern::Dtor { .. } => Err(LowerError::Unsupported(
            "a `mu` covers either a menu's requests or a data type's shapes, not both".into(),
        )),
        other => Err(LowerError::Unsupported(format!(
            "a `mu` arm covers one shape of the type; found {other:?}"
        ))),
    }
}

/// The binders of an arm's components, with the command wrapped by whatever
/// deeper destructuring the components ask for. A nested product has one
/// shape, so taking it apart keeps the one-arm-per-shape law: the component
/// is bound to a fresh name and taken apart again inside the command.
fn components<'p>(
    patterns: impl Iterator<Item = &'p Pattern>,
    command: Command,
) -> Result<(Vec<String>, Command), LowerError> {
    fn component(
        pattern: &Pattern,
        fresh: String,
        command: Command,
    ) -> Result<(String, Command), LowerError> {
        match pattern {
            Pattern::Ident(name) => Ok((name.clone(), command)),
            Pattern::Wildcard => Ok((UNUSED_BINDER.to_string(), command)),
            Pattern::Tuple(items) => {
                let (binders, command) = nested(items.iter(), &fresh, command)?;
                let cut = Command::Cut(
                    Term::Var(fresh.clone()),
                    CoTerm::MuTildeTensor(binders, Box::new(command)),
                );
                Ok((fresh, cut))
            }
            Pattern::Data { name, fields } => {
                let (binders, command) = nested(fields.iter().map(|(_, p)| p), &fresh, command)?;
                let cut = Command::Cut(
                    Term::Var(fresh.clone()),
                    CoTerm::CoCase {
                        owner: name.clone(),
                        branches: vec![CoCaseBranch {
                            label: name.clone(),
                            binders,
                            body: Box::new(command),
                        }],
                    },
                );
                Ok((fresh, cut))
            }
            other => Err(LowerError::Unsupported(format!(
                "a `mu` arm covers one shape: a sum or a value inside a component needs \
                 its own `of` in the arm; found {other:?}"
            ))),
        }
    }
    fn nested<'p>(
        patterns: impl Iterator<Item = &'p Pattern>,
        parent: &str,
        command: Command,
    ) -> Result<(Vec<String>, Command), LowerError> {
        let mut command = command;
        let mut binders = Vec::new();
        for (i, pattern) in patterns.enumerate() {
            let (name, wrapped) = component(pattern, format!("{parent}_{i}"), command)?;
            command = wrapped;
            binders.push(name);
        }
        Ok((binders, command))
    }
    nested(patterns, "__s", command)
}

/// The μ binder a canonical `of` captures: each arm's value is cut
/// against it, so the whole expression answers with the taken branch.
const MATCH_COVAR: &str = "__match";

/// Lower a `of` to a genuine cut against its branch table when the core
/// can express it: every arm is a shape — a variant, a record, a tuple, a
/// request, or one whole-value binder — with components that are binders or
/// nested products. `Ok(None)` means the match needs the runtime dispatch
/// (literals, or-patterns, ordered defaults); errors are real.
fn lower_match_canonical(
    scrutinee: &Node<Expr>,
    arms: &[MatchArm],
    continuations: &[String],
) -> Result<Option<Term>, LowerError> {
    let mut lowered = Vec::new();
    for arm in arms {
        lowered.push((&arm.pattern, lower_match_body(&arm.body, continuations)?));
    }
    let Some(consumer) = branch_table(lowered)? else { return Ok(None) };
    let scrutinee = lower_expr(scrutinee, continuations)?;
    Ok(Some(Term::Mu(MATCH_COVAR.into(), Box::new(Command::Cut(scrutinee, consumer)))))
}

/// The consumer a set of arms builds, each already lowered to the command it
/// runs: a labelled table, a single product, or a single whole-value binder.
/// `Ok(None)` is a mix the core's branch tables cannot express, which a
/// `of` answers with the runtime dispatch — and a binder refuses.
fn branch_table(arms: Vec<(&Pattern, Command)>) -> Result<Option<CoTerm>, LowerError> {
    fn canonical_component(pattern: &Pattern) -> bool {
        match pattern {
            Pattern::Ident(name) => lookup_constant(name).is_none(),
            Pattern::Wildcard => true,
            Pattern::Tuple(items) => items.iter().all(canonical_component),
            Pattern::Data { fields, .. } => fields.iter().all(|(_, p)| canonical_component(p)),
            _ => false,
        }
    }
    // A sum's alternatives are a labelled table too, one label per position,
    // when every arm is one and binds its payload plainly.
    if arms.iter().any(|(pattern, _)| matches!(pattern, Pattern::Inject { .. })) {
        let mut branches = Vec::new();
        for (pattern, body) in arms {
            match pattern {
                Pattern::Inject { index, pattern } if canonical_component(pattern) => {
                    let label = alternative_label(*index);
                    if branches.iter().any(|branch: &CoCaseBranch| branch.label == label) {
                        return Ok(None);
                    }
                    let (binders, body) = components(std::iter::once(pattern.as_ref()), body)?;
                    branches.push(CoCaseBranch { label, binders, body: Box::new(body) });
                }
                _ => return Ok(None),
            }
        }
        return Ok(Some(CoTerm::CoCase { owner: EMPTY_SUM.into(), branches }));
    }
    // One pass over the arms, sorting them into a labelled table, a single
    // product, or a single whole-value binder. Any mix the core's branch
    // tables cannot express aborts to the dispatch path.
    let mut branches: Vec<CoCaseBranch> = Vec::new();
    let mut atom: Option<CoTerm> = None;
    let mut product: Option<CoTerm> = None;
    let arm_count = arms.len();
    for (pattern, body) in arms {
        match pattern {
            Pattern::Ident(name)
                if lookup_constant(name).is_none() && lookup_variant(name).is_none() =>
            {
                // A whole-value binder: the atom form, alone or not at all —
                // a default among labelled arms is order-sensitive.
                if !branches.is_empty() || product.is_some() || atom.is_some() || arm_count != 1 {
                    return Ok(None);
                }
                atom = Some(CoTerm::MuTilde(name.clone(), Box::new(body)));
            }
            Pattern::Wildcard => {
                if !branches.is_empty() || product.is_some() || atom.is_some() || arm_count != 1 {
                    return Ok(None);
                }
                atom = Some(CoTerm::MuTilde(UNUSED_BINDER.into(), Box::new(body)));
            }
            Pattern::Ident(name) => match lookup_variant(name) {
                // A constant pattern is a literal; dispatch handles it.
                _ if lookup_constant(name).is_some() => return Ok(None),
                Some(label) => {
                    branches.push(CoCaseBranch { label, binders: Vec::new(), body: Box::new(body) })
                }
                None => return Ok(None),
            },
            Pattern::Enum { name, variant, fields } => {
                if !fields.iter().all(canonical_component) {
                    return Ok(None);
                }
                let written =
                    if variant.is_empty() { name.clone() } else { format!("{name}::{variant}") };
                let Some(label) = lookup_variant(&written) else { return Ok(None) };
                let (binders, body) = components(fields.iter(), body)?;
                branches.push(CoCaseBranch { label, binders, body: Box::new(body) });
            }
            Pattern::Data { name, fields } => {
                if !fields.iter().all(|(_, p)| canonical_component(p)) || arm_count != 1 {
                    return Ok(None);
                }
                let (binders, body) = components(fields.iter().map(|(_, p)| p), body)?;
                branches.push(CoCaseBranch { label: name.clone(), binders, body: Box::new(body) });
            }
            Pattern::Tuple(items) => {
                if !items.iter().all(canonical_component) || arm_count != 1 {
                    return Ok(None);
                }
                let (binders, body) = components(items.iter(), body)?;
                product = Some(CoTerm::MuTildeTensor(binders, Box::new(body)));
            }
            Pattern::Dtor { dtor, arg } => {
                // A matched request binds its continuation whole; its label
                // dispatches like any other.
                let binder = match arg.as_ref() {
                    Pattern::Ident(name) => name.clone(),
                    Pattern::Wildcard => UNUSED_BINDER.to_string(),
                    _ => return Ok(None),
                };
                let Some(label) = lookup_dtor(dtor) else { return Ok(None) };
                branches.push(CoCaseBranch { label, binders: vec![binder], body: Box::new(body) });
            }
            _ => return Ok(None),
        }
    }
    // Duplicate labels are ordered, first-match territory: not a table.
    let mut seen: Vec<&String> = Vec::new();
    for branch in &branches {
        if seen.contains(&&branch.label) {
            return Ok(None);
        }
        seen.push(&branch.label);
    }
    Ok(Some(match (branches.is_empty(), atom, product) {
        (true, Some(atom), None) => atom,
        (true, None, Some(product)) => product,
        (false, None, None) => lower_cocase(None, branches)?,
        _ => return Ok(None),
    }))
}

/// The command a canonical `of` arm runs: its value goes to the match's
/// own continuation, unless the arm is already a cut against a named
/// consumer, which stands as written.
fn lower_match_body(body: &Node<Expr>, continuations: &[String]) -> Result<Command, LowerError> {
    if let Some(command) = lower_closed_flow(body, continuations)? {
        return Ok(command);
    }
    Ok(Command::Cut(
        lower_in_command_position(body, continuations)?,
        CoTerm::Covar(MATCH_COVAR.into()),
    ))
}

/// Build a labelled consumer while retaining the declaration it refutes.
/// Nonempty tables can recover that declaration from their first qualified
/// label; empty tables must receive it from the surface type annotation.
/// The label of an anonymous sum's alternative at `index`: `|0`, `|1`, …. A
/// position is the whole of it — the sum need not be known to build one.
pub fn alternative_label(index: usize) -> String {
    format!("|{index}")
}

fn lower_cocase(
    qualifier: Option<&str>,
    branches: Vec<CoCaseBranch>,
) -> Result<CoTerm, LowerError> {
    let owner = qualifier
        .map(str::to_string)
        .or_else(|| {
            branches.first().map(|branch| {
                branch
                    .label
                    .rsplit_once("::")
                    .map_or_else(|| branch.label.clone(), |(owner, _)| owner.to_string())
            })
        })
        .ok_or_else(|| {
            LowerError::Unsupported("an empty labelled consumer must name its type".into())
        })?;
    Ok(CoTerm::CoCase { owner, branches })
}

/// Build a menu from copattern rows, grouping arms by their outer
/// destructor. A group with one plainly-bound arm is a leaf; a group whose
/// arms all nest — `tail: head: out` — answers its item with an inner
/// menu, built recursively from the arms' payload patterns and cut against
/// the request's continuation.
fn lower_comatch(
    qualifier: Option<&str>,
    rows: Vec<(&Pattern, &Node<Expr>)>,
    continuations: &[String],
    depth: usize,
) -> Result<Term, LowerError> {
    let mut order: Vec<&String> = Vec::new();
    let mut groups: HashMap<&String, Vec<(&Pattern, &Node<Expr>)>> = HashMap::new();
    for (pattern, command) in rows {
        let Pattern::Dtor { dtor, arg } = pattern else {
            return Err(LowerError::Unsupported(
                "`mu` with arms answers a menu's demands; every arm is `item: pattern`".into(),
            ));
        };
        if !groups.contains_key(dtor) {
            order.push(dtor);
        }
        groups.entry(dtor).or_default().push((arg.as_ref(), command));
    }
    let mut branches = Vec::new();
    for dtor in order {
        let label = qualifier
            .and_then(|q| lookup_dtor(&format!("{q}::{dtor}")))
            .or_else(|| lookup_dtor(dtor))
            .ok_or_else(|| {
                LowerError::Unsupported(format!("`.{dtor}` does not name a declared menu item"))
            })?;
        let group = groups.remove(dtor).expect("grouped above");
        if let [(Pattern::Ident(name), command)] = group.as_slice() {
            // The copattern binds the demand's continuation, so the arm's
            // body may reach it: it belongs to the arm's scope.
            let mut arm_scope = continuations.to_vec();
            arm_scope.push((*name).clone());
            let body = lower_select_command(command, &arm_scope)?;
            branches.push(CoMatchBranch { label, binder: name.clone(), body: Box::new(body) });
        } else if let [(Pattern::Wildcard, command)] = group.as_slice() {
            let body = lower_select_command(command, continuations)?;
            branches.push(CoMatchBranch {
                label,
                binder: UNUSED_BINDER.into(),
                body: Box::new(body),
            });
        } else if group.iter().all(|(arg, _)| matches!(arg, Pattern::Dtor { .. })) {
            // The item is refined: its answer is an inner menu, and the
            // whole of it goes to this request's continuation.
            let inner = lower_comatch(None, group, continuations, depth + 1)?;
            let binder = format!("__k{depth}");
            let body = Command::Cut(inner, CoTerm::Covar(binder.clone()));
            branches.push(CoMatchBranch { label, binder, body: Box::new(body) });
        } else {
            return Err(LowerError::Unsupported(format!(
                "item `{dtor}` is answered once with a binder, or refined by nested requests — \
                 not both"
            )));
        }
    }
    let owner = qualifier
        .map(str::to_string)
        .or_else(|| {
            branches
                .first()
                .and_then(|branch| branch.label.rsplit_once("::"))
                .map(|(owner, _)| owner.to_string())
        })
        .ok_or_else(|| {
            LowerError::Unsupported("an empty menu value must name its menu type".into())
        })?;
    Ok(Term::CoMatch { owner, branches })
}

/// The consumer a cut names. Reification erases at
/// lowering — a boxed consumer and the consumer are the same value at run
/// time, so a reified consumer names itself.
fn named_consumer(consumer: &Expr) -> Option<&String> {
    match consumer {
        Expr::Ident(name) => Some(name),
        _ => None,
    }
}

/// The command a `mu` arm runs. A cut against a named consumer is that
/// command directly; any other command-typed expression is lowered as a term
/// and cut against the arm's own co-variable, which nothing returns to.
fn lower_select_command(
    command: &Node<Expr>,
    continuations: &[String],
) -> Result<Command, LowerError> {
    if let Some(command) = lower_closed_flow(command, continuations)? {
        return Ok(command);
    }
    Ok(Command::Cut(
        lower_in_command_position(command, continuations)?,
        CoTerm::Covar(ARM_COVAR.into()),
    ))
}

/// One stage of a chain. A bare name the checker resolved as a trait method
/// is that dispatch, and a bounded one takes its dictionaries first, as a
/// bounded call does — but only when the stage *is* the bare name: a call
/// already carries its own, keyed by the call's span.
fn lower_flow_stage(stage: &Node<Expr>, continuations: &[String]) -> Result<Term, LowerError> {
    let dispatch = matches!(stage.kind, Expr::Ident(_)).then(|| method_dispatch(stage.span));
    let mut term = match dispatch.flatten() {
        Some(MethodDispatch::Static(mangled)) => Term::Var(mangled),
        Some(MethodDispatch::Dict { dict_var, index, count }) => {
            dict_projection(&dict_var, index, count)
        }
        None => lower_expr(stage, continuations)?,
    };
    if matches!(stage.kind, Expr::Ident(_))
        && let Some(dicts) = call_dicts(stage.span)
    {
        term = call_curried(term, dicts.iter().map(dict_term).collect());
    }
    Ok(term)
}

/// A plain chain that closes against a named consumer, as the command it
/// already is: everything before the closing stage is what flows in, and it
/// is cut against that consumer directly rather than through a μ binder
/// nothing returns to. Anything the general arm reads specially — an
/// eta-expanded chain, a commuted stage, a `command`'s row — is left to it.
fn lower_closed_flow(
    command: &Node<Expr>,
    continuations: &[String],
) -> Result<Option<Command>, LowerError> {
    let elaborated = ELABORATED.with(|cell| cell.borrow().get(&command.span).cloned());
    let command = elaborated.as_ref().unwrap_or(command);
    let Expr::Flow { stages, from_value: true, into_consumer: true } = &command.kind else {
        return Ok(None);
    };
    let plain = FLOWS.with(|cell| cell.borrow().get(&command.span).cloned()).is_none_or(|shape| {
        !shape.eta
            && shape.commuted.is_empty()
            && shape.row_stage.is_none()
            && shape.swap.is_none()
            && shape.turned.is_empty()
    });
    if !plain {
        return Ok(None);
    }
    let Some((closing, flowing)) = stages.split_last() else { return Ok(None) };
    let Some(name) = named_consumer(&closing.kind) else { return Ok(None) };
    let Some((first, rest)) = flowing.split_first() else { return Ok(None) };
    let mut value = delay_if_delayed(first.span, lower_flow_stage(first, continuations)?);
    for stage in rest {
        value = flow_step(lower_flow_stage(stage, continuations)?, value, None);
    }
    Ok(Some(Command::Cut(value, CoTerm::Covar(name.clone()))))
}

/// One step of a chain read the other way round: `stage` builds a consumer
/// from the continuation of the step, and `value` is fed to that consumer —
/// `μk. ⟨stage(k) ∥ value · k⟩`. The step at `at` names its own continuation,
/// so steps nested in one another never capture each other's.
fn turned_step(stage: Term, value: Term, at: usize) -> Term {
    let k = format!("__turn{at}");
    Term::Mu(
        k.clone(),
        Box::new(Command::Cut(
            call_curried(stage, vec![Term::Var(k.clone())]),
            CoTerm::App(value, Box::new(CoTerm::Covar(k))),
        )),
    )
}

fn flow_step(stage: Term, value: Term, commuted: Option<usize>) -> Term {
    let pure_stage = matches!(stage, Term::Var(_) | Term::Lam(..));
    let apply = |argument| match commuted {
        Some(index) => turned_step(stage, argument, index),
        None => call_curried(stage, vec![argument]),
    };
    if pure_stage {
        apply(value)
    } else {
        let result = apply(Term::Var("$flow_argument".into()));
        Term::Mu(
            "$flow_binding".into(),
            Box::new(Command::Cut(
                value,
                CoTerm::MuTilde(
                    "$flow_argument".into(),
                    Box::new(Command::Cut(result, CoTerm::Covar("$flow_binding".into()))),
                ),
            )),
        )
    }
}

/// The binder a parameterless declaration introduces for the unit its
/// callers pass. `f()` is `f((,))`, so there is nothing special about it
/// beyond the name never being mentioned.
const NO_ARGUMENTS: &str = "__no_args";

/// The value an eta-expanded flow abstracts over: `f | k` denotes the
/// consumer `λx. x | f | k`.
const FLOW_ARGUMENT: &str = "__flow_value";

/// A binder a lowered arm introduces but never mentions.
const UNUSED_BINDER: &str = "__unused";
/// The co-variable an arm's command is cut against when it is not already a
/// cut against a named consumer. Nothing binds it: an arm does not return.
const ARM_COVAR: &str = "__arm";

fn call_curried(callee: Term, args: Vec<Term>) -> Term {
    let mut result = callee;
    for arg in args {
        result = Term::Mu(
            "__call".into(),
            Box::new(Command::Cut(
                result,
                CoTerm::App(arg, Box::new(CoTerm::Covar("__call".into()))),
            )),
        );
    }
    result
}

fn yielding_command(callee: Term, values: Term, row: Term, count: usize) -> Term {
    let callbacks = (0..count)
        .map(|index| {
            let callback = if count == 1 {
                Term::Var("$yield_row".into())
            } else {
                Term::Mu(
                    "$yield_projection".into(),
                    Box::new(Command::Cut(Term::Var("$yield_row".into()), CoTerm::Prj(index))),
                )
            };
            Term::Lam(
                "$yield_argument".into(),
                Box::new(Term::Mu(
                    "$yield_callback".into(),
                    Box::new(Command::Cut(
                        call_curried(callback, vec![Term::Var("$yield_argument".into())]),
                        CoTerm::Covar("$yield_out".into()),
                    )),
                )),
            )
        })
        .collect::<Vec<_>>();
    let exits =
        if count == 1 { callbacks.into_iter().next().unwrap() } else { Term::Tuple(callbacks) };
    Term::Mu(
        "$yield_out".into(),
        Box::new(Command::Cut(
            values,
            CoTerm::MuTilde(
                "$yield_values".into(),
                Box::new(Command::Cut(
                    row,
                    CoTerm::MuTilde(
                        "$yield_row".into(),
                        Box::new(Command::Cut(
                            call_curried(callee, vec![Term::Var("$yield_values".into()), exits]),
                            CoTerm::Covar("$yield_out".into()),
                        )),
                    ),
                )),
            ),
        )),
    )
}

fn pattern_descriptor(pattern: &Pattern) -> String {
    fn escape(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }
    fn write(pattern: &Pattern, out: &mut String) {
        match pattern {
            Pattern::Wildcard => out.push('*'),
            Pattern::Ident(name) => {
                if let Some(pattern) = lookup_constant(name) {
                    write(&pattern, out);
                } else if let Some(label) = lookup_variant(name) {
                    // A variant name is a variant pattern, not a binding.
                    write(
                        &Pattern::Enum {
                            name: label.split_once("::").map(|(e, _)| e).unwrap_or("").to_string(),
                            variant: label
                                .split_once("::")
                                .map(|(_, v)| v)
                                .unwrap_or(label.as_str())
                                .to_string(),
                            fields: Vec::new(),
                        },
                        out,
                    );
                } else {
                    out.push('$');
                    out.push_str(&escape(name));
                }
            }
            Pattern::Int(n) => {
                out.push('#');
                out.push_str(&n.to_string());
            }
            Pattern::Str(s) => {
                out.push('"');
                out.push_str(&escape(s));
                out.push('"');
            }
            Pattern::Char(c) => {
                out.push('\'');
                match c {
                    '\\' => out.push_str("\\\\"),
                    '\'' => out.push_str("\\'"),
                    c => out.push(*c),
                }
                out.push('\'');
            }
            Pattern::Float(n) => {
                out.push('%');
                out.push_str(&n.to_string());
            }
            Pattern::Or(alternatives) => {
                out.push('(');
                for (i, alternative) in alternatives.iter().enumerate() {
                    if i > 0 {
                        out.push('|');
                    }
                    write(alternative, out);
                }
                out.push(')');
            }
            Pattern::Range { start, end } => {
                write(start, out);
                out.push_str("..=");
                write(end, out);
            }
            Pattern::Binding { name, pattern } => {
                out.push_str(&escape(name));
                out.push('@');
                write(pattern, out);
            }
            Pattern::Rest => out.push_str(".."),
            // A request shape: its qualified label with the continuation as
            // the single bound field, exactly as an enum pattern encodes.
            Pattern::Dtor { dtor, arg } => {
                let label = lookup_dtor(dtor).unwrap_or_else(|| dtor.clone());
                out.push('"');
                out.push_str(&escape(&label));
                out.push('"');
                out.push('(');
                // The payload of a matched request is its continuation — a
                // live value with no shape to inspect — so anything but a
                // binder or a wildcard cannot match at run time.
                match arg.as_ref() {
                    Pattern::Ident(name) => {
                        out.push('$');
                        out.push_str(&escape(name));
                    }
                    _ => out.push('*'),
                }
                out.push(')');
            }
            // An alternative by its position's label, its payload inside.
            Pattern::Inject { index, pattern } => {
                out.push('"');
                out.push_str(&alternative_label(*index));
                out.push_str("\"(");
                write(pattern, out);
                out.push(')');
            }
            Pattern::Tuple(items) | Pattern::Bundle(items) => {
                // A bundle is the same right-nested pair a tuple is, so it
                // matches the same way: the checker tells `&` from `,`.
                out.push('(');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write(item, out);
                }
                out.push(')');
            }
            Pattern::Data { name, fields } => {
                // Fields are written in declaration order, so the pattern is
                // the labelled shape the value has, with positional fields.
                out.push('"');
                out.push_str(&escape(name));
                out.push('"');
                out.push('(');
                for (i, (_, pattern)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write(pattern, out);
                }
                out.push(')');
            }
            Pattern::Enum { name, variant, fields } => {
                // An unqualified constructor pattern carries the variant name
                // in `name`; resolve either spelling to the declared label.
                let written =
                    if variant.is_empty() { name.clone() } else { format!("{name}::{variant}") };
                let label = lookup_variant(&written).unwrap_or(written);
                out.push('"');
                out.push_str(&escape(&label));
                out.push('"');
                out.push('(');
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write(field, out);
                }
                out.push(')');
            }
        }
    }
    let mut out = String::new();
    write(pattern, &mut out);
    out
}

fn lookup_constant(name: &str) -> Option<Pattern> {
    CONSTANTS.with(|cell| cell.borrow().get(name).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn lower_str(s: &str) -> Vec<(String, Term)> {
        let toks = lex(s).unwrap();
        let prog = parse(toks).unwrap();
        lower_program(&prog).unwrap()
    }

    #[test]
    fn lower_block_sequence_names_cannot_capture_user_continuations() {
        let out = lower_str(
            "func f(__seq0: -i32 & __ret0: -i32) <- i32 {
                println(1);
                println(2);
                __seq0(1)
            }",
        );
        let printed = format!("{}", out[0].1);
        assert!(
            printed.contains("μ̃(__seq0, __ret0)"),
            "user continuation binders should remain distinct: {printed}"
        );
        assert!(
            printed.matches("__seq0").count() >= 2,
            "user continuation must still be referenced: {printed}"
        );
    }

    #[test]
    fn a_block_returns_through_its_own_bound_continuation() {
        let definitions = lower_str("func result() -> i64 { 1; 2 }");
        let Term::Lam(_, body) = &definitions[0].1 else { panic!("expected a function") };
        let Term::Mu(bound, command) = body.as_ref() else { panic!("expected a block") };
        let Command::Cut(_, CoTerm::MuTilde(_, rest)) = command.as_ref() else {
            panic!("expected a sequencing binder")
        };
        let Command::Cut(_, CoTerm::Covar(returned)) = rest.as_ref() else {
            panic!("expected a return through a continuation")
        };
        assert_eq!(bound, returned);
        let free = slc_core::substitution::free_vars_term(body);
        assert_eq!(free, ["$int_1".into(), "$int_2".into()].into_iter().collect());
    }

    #[test]
    fn empty_menu_lowering_retains_its_owner() {
        // Braces with no arms are the consumer of the written type.
        let out = lower_str("menu Top {} func top() -> Top { mu Top {} }");
        let (_, Term::Lam(_, body)) = &out[0] else { panic!("expected a nullary function") };
        let Term::Co(coterm) = body.as_ref() else { panic!("expected a consumer, got {body:?}") };
        assert!(matches!(
            coterm.as_ref(),
            slc_core::coterm::CoTerm::CoCase { owner, branches }
                if owner == "Top" && branches.is_empty()
        ));
    }

    #[test]
    fn a_non_nullary_unit_shadow_keeps_its_label() {
        let out = lower_str("data Unit { value: i64 } func unit() -> Unit { Unit { value: 1 } }");
        let unit = out.iter().find(|(name, _)| name == "unit").unwrap();
        let Term::Lam(_, body) = &unit.1 else { panic!("expected a nullary function") };
        assert!(matches!(body.as_ref(), Term::Tag(label, _) if label == "Unit"));
    }

    #[test]
    fn lower_block_sequence_handles_nested_shadowed_empty_and_single_forms() {
        // A positive function with no parameters binds the marker a call
        // with no arguments supplies, so it stays callable.
        let single = lower_str("func f() -> i32 { 1 }")[0].1.clone();
        assert_eq!(single, Term::Lam("__no_args".into(), Box::new(Term::Var("$int_1".into()))));

        let empty = lower_str("func f() -> unit { }")[0].1.clone();
        assert_eq!(empty, Term::Lam("__no_args".into(), Box::new(Term::Var("$unit".into()))));

        let nested = lower_str(
            "func f() -> i32 {
                let x = 1;
                {
                    let y = 2;
                    x
                }
            }",
        )[0]
        .1
        .clone();
        let printed = format!("{nested}");
        assert!(printed.contains("x"), "nested block lost binding: {printed}");
        assert!(printed.contains("y"), "nested block lost inner binding: {printed}");
        assert!(
            printed.matches("__seq").count() == printed.matches("__ret").count(),
            "sequence binder/co-variable names must pair: {printed}"
        );
    }

    #[test]
    fn lower_select_is_a_negative_additive_consumer() {
        // `mu` must lower to a genuine negative additive co-term — one
        // branch per variant, each cutting the arm value against the arm's
        // consumer — and not to an opaque builtin marker.
        let src = "enum Color { Red, Green, Blue } func k(return: -i32) <- Color { mu Color { Red => <0 | return>, Green => <1 | return>, Blue => <2 | return> } }";
        let out = lower_str(src);
        let k = out.iter().find(|(name, _)| name == "k").unwrap();

        // The declaration binds its continuation parameter as a co-abstraction.
        let Term::Lam(covar, body) = &k.1 else {
            panic!("negative function should lower to a co-abstraction: {}", k.1);
        };
        assert_eq!(covar, "return");
        let Term::Co(coterm) = body.as_ref() else {
            panic!("`mu` should lower to a reified co-term: {body}");
        };
        let CoTerm::CoCase { owner, branches } = coterm.as_ref() else {
            panic!("`mu` should lower to a negative additive consumer: {coterm}");
        };
        assert_eq!(owner, "Color");
        let labels: Vec<&str> = branches.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, ["Color::Red", "Color::Green", "Color::Blue"]);
        for (branch, value) in branches.iter().zip(["$int_0", "$int_1", "$int_2"]) {
            let Command::Cut(Term::Var(arg), CoTerm::Covar(consumer)) = branch.body.as_ref() else {
                panic!("branch should cut its value against its consumer: {}", branch.body);
            };
            assert_eq!(arg, value);
            assert_eq!(consumer, "return");
        }
    }

    #[test]
    fn lower_select_over_a_product_is_a_product_consumer() {
        // A product has one shape, so its consumer binds every component and
        // needs no label.
        let out = lower_str(
            "func total(out: -i64) <- (+i64, +i64) {
                 mu (+i64, +i64) { (left, right) => <(left, right) | __add | out> }
             }",
        );
        let Term::Lam(_, body) = &out[0].1 else { panic!("expected a co-abstraction") };
        let Term::Co(coterm) = body.as_ref() else { panic!("expected a reified co-term") };
        let CoTerm::MuTildeTensor(binders, command) = coterm.as_ref() else {
            panic!("expected a product consumer: {coterm}");
        };
        assert_eq!(binders, &["left".to_string(), "right".to_string()]);
        assert!(
            matches!(command.as_ref(), Command::Cut(_, CoTerm::Covar(name)) if name == "out"),
            "{command}"
        );
    }

    #[test]
    fn lower_select_over_a_struct_binds_its_fields() {
        // A struct is a labelled product: one branch, labelled by the
        // declaration, binding every field.
        let out = lower_str(
            "data R { value: i64, unit: String }
             func show(out: -String) <- R { mu R { R { value, unit } => <unit | out> } }",
        );
        let show = out.iter().find(|(name, _)| name == "show").unwrap();
        let printed = format!("{}", show.1);
        assert!(printed.contains("μ̃[R; R(value, unit)."), "{printed}");
    }

    #[test]
    fn lower_enum_value_is_a_labelled_injection() {
        let out = lower_str("enum Color { Red, Green } func main() -> i32 { 0 }");
        let red = out.iter().find(|(name, _)| name == "Color::Red").unwrap();
        assert_eq!(red.1, Term::Tag("Color::Red".into(), Box::new(Term::Var("$unit".into()))));
    }

    #[test]
    fn lower_select_rejects_an_arm_that_is_not_a_shape() {
        // An arm covers one shape of the type; a literal is not one.
        let src = "enum Color { Red, Green } func k(return: -i32) <- Color { mu Color { 1 => <0 | return>, Green => <1 | return> } }";
        let toks = crate::lexer::lex(src).unwrap();
        let prog = crate::parser::parse(toks).unwrap();
        assert!(
            matches!(lower_program(&prog), Err(LowerError::Unsupported(message))
                if message.contains("covers one shape")),
            "{:?}",
            lower_program(&prog)
        );
    }

    #[test]
    fn lower_struct_literal_is_a_labelled_product() {
        // A record literal is its declaration's name applied to the
        // right-nested tensor of its field values — the same labelled shape
        // an enum variant has.
        let out = lower_str(
            "data D { left: i64, right: i64 } func f() -> i64 { use_it(D { left: 1, right: 2 }) }",
        );
        let printed = format!("{}", out.iter().find(|(name, _)| name == "f").unwrap().1);
        assert!(
            printed.contains("D(($int_1 ⊗ $int_2))"),
            "record literal should lower to a labelled product: {printed}"
        );

        // One field needs no tensor, and none is unit.
        let one = lower_str("data One { only: i64 } func f() -> i64 { use_it(One { only: 1 }) }");
        let printed = format!("{}", one.iter().find(|(name, _)| name == "f").unwrap().1);
        assert!(printed.contains("One($int_1)"), "{printed}");
    }

    #[test]
    fn lower_cut_with_a_named_consumer_is_a_core_cut() {
        // `v | k` is the command ⟨v ∥ k⟩. The μ binder that wraps it is never
        // referenced — a command has no result — and must not be the
        // consumer's own name, or the cut would send the value to itself.
        let positive = lower_str("func f(k: -i32) <- i32 { <1 | k> }");
        assert_eq!(
            positive[0].1,
            Term::Lam(
                "k".into(),
                Box::new(Term::Mu(
                    "__cut".into(),
                    Box::new(Command::Cut(Term::Var("$int_1".into()), CoTerm::Covar("k".into())))
                ))
            )
        );

        // A consumer named `__cut` still receives the value.
        let shadowed = lower_str("func f(__cut: -i32) <- i32 { <1 | __cut> }");
        assert_eq!(
            shadowed[0].1,
            Term::Lam(
                "__cut".into(),
                Box::new(Term::Mu(
                    "__cut_".into(),
                    Box::new(Command::Cut(
                        Term::Var("$int_1".into()),
                        CoTerm::Covar("__cut".into())
                    ))
                ))
            )
        );
    }

    #[test]
    fn lower_flow_reads_its_brackets() {
        // The brackets say what a chain is, so lowering never guesses: the
        // same stages are a cut when closed and an application when not.
        let cut = lower_str("func f(ignored: +i32) -> i32 { <1 | pick(2)> }");
        let Term::Lam(_, body) = &cut[0].1 else { panic!("expected a value binder") };
        let Term::Mu(binder, command) = body.as_ref() else {
            panic!("a cut is wrapped in a μ binder: {body}");
        };
        assert_eq!(binder, "__cut");
        let Command::Cut(value, CoTerm::MuTilde(binding, apply)) = command.as_ref() else {
            panic!("the value is evaluated before its computed consumer: {command}");
        };
        let Command::Cut(consumer, CoTerm::App(argument, _)) = apply.as_ref() else {
            panic!("the computed consumer receives the saved value: {apply}");
        };
        assert_eq!(argument, &Term::Var(binding.clone()));
        assert!(format!("{consumer}").contains("pick"), "the consumer is evaluated: {consumer}");
        assert_eq!(value, &Term::Var("$int_1".into()));

        let open = lower_str("func f(ignored: +i32) -> i32 { <1 | pick(2) }");
        let Term::Lam(_, body) = &open[0].1 else { panic!("expected a value binder") };
        let Term::Mu(binder, command) = body.as_ref() else { panic!("an application: {body}") };
        assert_eq!(binder, "$flow_binding");
        let Command::Cut(value, CoTerm::MuTilde(_, application)) = command.as_ref() else {
            panic!("the value is bound before computing the stage: {command}");
        };
        assert_eq!(value, &Term::Var("$int_1".into()));
        assert!(format!("{application}").contains("pick"));
    }

    #[test]
    fn a_flat_match_lowers_to_a_branch_table() {
        // Every arm a shape: the match is a genuine cut against
        // μ̃[…], not a call into the dispatch builtin.
        let out = lower_str(
            "enum Colour { Red, Green } \
             func f(c: Colour) -> i32 { of c { Red => 1, Green(x) => 2 } }",
        );
        let f = out.iter().find(|(name, _)| name == "f").expect("f is lowered");
        let printed = format!("{}", f.1);
        assert!(!printed.contains("__match_dispatch"), "canonical, not dispatch: {printed}");
        assert!(printed.contains("μ̃[Colour; Colour::Red()."), "a labelled branch table: {printed}");
        assert!(printed.contains("∥ __match⟩"), "arm values reach the match: {printed}");
    }

    #[test]
    fn a_literal_match_still_dispatches() {
        let out = lower_str("func f(n: +i32) -> i32 { of n { 1 => 1, _ => 0 } }");
        let f = out.iter().find(|(name, _)| name == "f").expect("f is lowered");
        let printed = format!("{}", f.1);
        assert!(printed.contains("__match_dispatch"), "literals need equality: {printed}");
    }

    #[test]
    fn lower_int() {
        let out = lower_str("42");
        assert_eq!(out[0].1, Term::Lam("__no_args".into(), Box::new(Term::Var("$int_42".into()))));
    }

    #[test]
    fn lower_negative_fn_binds_its_exits_as_one_group() {
        // A negative function's parameters are its menu of exits, and a
        // group is one argument: it binds that, then destructures it into
        // the exits the body names.
        let out = lower_str("func k(return: -i32 & other: -Bool) <- Bool { return(0) }");
        let printed = format!("{}", out[0].1);
        assert!(printed.starts_with("λ__args."), "the group is one binder: {printed}");
        assert!(
            printed.contains("μ̃(return, other)"),
            "the group destructures into its exits: {printed}"
        );
    }

    #[test]
    fn lower_mu_binds_values_before_continuations() {
        // `mu f(values) | (continuations)` is called as
        // `f(values..., continuations...)`, so the λ binders come first.
        let out = lower_str("proc route(x: +i32) | (k: -i32) { k(x) }");
        let term = &out[0].1;
        let Term::Lam(value, rest) = term else {
            panic!("value parameter must be a λ binder: {term}");
        };
        assert_eq!(value, "x");
        assert!(
            matches!(rest.as_ref(), Term::Lam(k, _) if k == "k"),
            "continuation parameter must be a co-abstraction binder: {rest}"
        );
    }

    #[test]
    fn lower_lambda() {
        let out = lower_str("func id(x: +i32) -> i32 { x }");
        assert_eq!(out[0].1, Term::Lam("x".into(), Box::new(Term::Var("x".into()))));
    }

    #[test]
    fn lower_negative_fn() {
        let out = lower_str("func k(x: -i32) <- i32 { x }");
        assert_eq!(out[0].1, Term::Lam("x".into(), Box::new(Term::Var("x".into()))));
    }

    #[test]
    fn lower_uses_explicit_lexical_continuation_scopes() {
        // A local mu adds its binder only inside its own body.
        let out = lower_str(
            "func f(ok: -i32) <- i32 {
                mu i32 { inner <= ok(escape(1, inner)) }
            }",
        );
        let printed = format!("{}", out[0].1);
        assert!(printed.contains("inner"), "local mu binder missing: {printed}");
    }

    #[test]
    fn generic_positive_and_negative_functions_lower_structurally() {
        let positive = lower_str("func id<+T>(value: T) -> T { value }")[0].1.clone();
        assert!(
            matches!(&positive, Term::Lam(name, body) if name == "value" && matches!(&**body, Term::Var(v) if v == "value")),
            "positive generic parameter should lower as a lambda binder: {positive:?}"
        );

        let negative = lower_str("func k<+T>(ok: -T) <- T { ok(0) }")[0].1.clone();
        assert!(
            matches!(&negative, Term::Lam(name, _) if name == "ok"),
            "negative generic continuation parameter should lower as a co-abstraction binder: {negative:?}"
        );
        let printed = format!("{negative}");
        assert!(printed.starts_with("λok."), "continuation binder missing: {printed}");
    }

    #[test]
    fn lower_types() {
        let ty = lower_type(&TypeExpr::Base("i32".into())).unwrap();
        assert_eq!(ty, Type::Pos(Base::I32));
        let ty = lower_type(&TypeExpr::Base("char".into())).unwrap();
        assert_eq!(ty, Type::Pos(Base::Char));
        let ty = lower_type(&TypeExpr::Base("unit".into())).unwrap();
        assert_eq!(ty, Type::ONE);
    }

    #[test]
    fn lower_type_level_dual_remains_supported() {
        let inner = Node {
            span: crate::token::Span { start: 5, end: 8 },
            kind: TypeExpr::Base("i32".into()),
        };
        // `dual(A)` applies the involution rather than wrapping a node.
        let ty = lower_type(&TypeExpr::Dual(Box::new(inner))).unwrap();
        assert_eq!(ty, Type::Neg(Base::I32));
    }

    #[test]
    fn lower_negative_type() {
        // -i32 → Neg(I32)
        let inner = Node {
            span: crate::token::Span { start: 0, end: 4 },
            kind: TypeExpr::Base("i32".into()),
        };
        let ty = lower_type(&TypeExpr::Negative(Box::new(inner))).unwrap();
        assert_eq!(ty, Type::Neg(Base::I32));
    }
}
