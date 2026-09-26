//! Surface AST for SLC.

use crate::token::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Node<T> {
    pub kind: T,
    pub span: Span,
}

/// What a `base.key` projection selects: a tuple position, or a record field
/// by name.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjKey {
    Index(usize),
    Field(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Ident(String),
    Lambda {
        param: String,
        param_type: Option<TypeExpr>,
        return_type: Option<TypeExpr>,
        body: Box<Node<Expr>>,
    },
    Call {
        callee: Box<Node<Expr>>,
        args: Vec<Node<Expr>>,
    },
    Pair(Vec<Node<Expr>>),
    /// `(k1 & k2 & …)` — a bundle of exits, the anonymous menu's
    /// introduction: every component is supplied, and whoever holds it
    /// takes exactly one. Its type is the `&` of its components'.
    Bundle(Vec<Node<Expr>>),
    /// `::0(v)` — an alternative of an anonymous sum, by position: an enum
    /// value with the enum's name left out. Which sum it belongs to, and so
    /// how many alternatives follow, is its context's to say.
    Inject {
        index: usize,
        value: Box<Node<Expr>>,
    },
    /// `(k1 ; k2 ; …)` — a form value: one continuation per component of a
    /// `;`. Fed the product of what they want, it hands each its part, left
    /// to right.
    Par(Vec<Node<Expr>>),
    Match {
        scrutinee: Box<Node<Expr>>,
        arms: Vec<MatchArm>,
    },
    /// A record literal: `Direction { left: 0, right: 1 }`.
    Data {
        name: String,
        fields: Vec<(String, Node<Expr>)>,
    },
    /// `select T { pattern <= command, … }` — the consumer of a positive
    /// type, given by cases on it. An `enum` has one arm per variant; a
    /// product has exactly one, binding its components.
    /// The type may be omitted when an arm's pattern names it: `Red` names
    /// its enum, `S { … }` names its struct. A product or an atom has no
    /// such name, so it is written.
    Select {
        ty: Option<Box<Node<TypeExpr>>>,
        arms: Vec<SelectArm>,
    },
    /// `let p = v; rest` — a binder, which is a pattern. A bare name is
    /// the trivial one; anything else must be irrefutable for the value's
    /// type, which the checker enforces.
    Let {
        pattern: Pattern,
        ty: Option<TypeExpr>,
        value: Box<Node<Expr>>,
        body: Option<Box<Node<Expr>>>,
        /// `let`, `let+` or `let-`: when the value is computed.
        mode: LetMode,
    },
    /// `base.0` / `base.field`: project one component of a product. The
    /// component's index and the product's arity are resolved from `base`'s
    /// type in the checker (the right-nested encoding needs the arity), so the
    /// key here is only what was written.
    Project {
        base: Box<Node<Expr>>,
        key: ProjKey,
    },
    /// A request literal: `.item(k)` — one demand on a `menu` type, carrying
    /// the continuation `k` that wants the answer. The mirror of an enum
    /// variant expression: a variant is data the producer tags, a request is
    /// a demand the consumer tags.
    Request {
        dtor: String,
        arg: Box<Node<Expr>>,
    },
    /// A flow: `a | b | c` — everything moves left to right, and every
    /// step composes unless a bracket says otherwise. `<` marks the left
    /// end closed, so the stage beside it is a value; `>` marks the right
    /// end closed, so the stage beside it is a consumer. Closed at both
    /// ends, the chain is a cut: `<v | f | k>`.
    Flow {
        stages: Vec<Node<Expr>>,
        /// `<`: the first stage is a value, not a function.
        from_value: bool,
        /// `>`: the last stage consumes, so the chain delivers.
        into_consumer: bool,
    },
    /// `mu T { item: k <= c, … }` — the copattern form of `mu`: a menu
    /// value, branching on the demand the ambient consumer turns out to be.
    /// The mirror of `select` over a data type: `select` answers data, `mu`
    /// answers demands. The type may be left out when an arm's destructor
    /// names its menu unambiguously.
    CoMatch {
        ty: Option<Box<Node<TypeExpr>>>,
        arms: Vec<SelectArm>,
    },
    /// A local μ abstraction, the binder arm of `mu`: `mu { k <= c }`
    /// captures the continuation the expression is cut against and runs the
    /// command with it bound. It has no value parameters — abstracting over
    /// a value is what `fn` does — and the produced type in front,
    /// `mu i64 { k <= c }`, may be left off when the command says it.
    Mu {
        continuation_params: Vec<Param>,
        body: Box<Node<Expr>>,
    },
    /// `handle body { op(p): k => b, …, return(x) => r }`: run `body`,
    /// answering each performed operation with its clause and its normal
    /// result with the `return` clause. The handled effect is determined by
    /// the clause operations — operation names are unique across effects — so
    /// it is not written.
    Handle {
        body: Box<Node<Expr>>,
        clauses: Vec<HandleClause>,
        forward: bool,
        /// The `return(x) => r` clause: its binder and body.
        ret: Option<(String, Box<Node<Expr>>)>,
    },
    Handler {
        effects: Vec<String>,
        clauses: Vec<HandleClause>,
        forward: bool,
        ret: Option<(String, Box<Node<Expr>>)>,
    },
    WithHandler {
        handler: Box<Node<Expr>>,
        body: Box<Node<Expr>>,
    },
    /// A sequence of expressions; the value of the last one.
    Block(Vec<Node<Expr>>),
}

impl Expr {
    /// Every immediate sub-expression, in source order. A walk over an
    /// expression needs no case for each node this way.
    pub fn children(&self) -> Vec<&Node<Expr>> {
        match self {
            Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Char(_) | Expr::Ident(_) => {
                Vec::new()
            }
            Expr::Lambda { body, .. } | Expr::Mu { body, .. } => {
                vec![body]
            }
            Expr::Call { callee, args } => std::iter::once(&**callee).chain(args).collect(),
            Expr::Inject { value, .. } => vec![value],
            Expr::Pair(items)
            | Expr::Bundle(items)
            | Expr::Par(items)
            | Expr::Block(items)
            | Expr::Flow { stages: items, .. } => items.iter().collect(),
            Expr::Match { scrutinee, arms } => {
                std::iter::once(&**scrutinee).chain(arms.iter().map(|a| &a.body)).collect()
            }
            Expr::Data { fields, .. } => fields.iter().map(|(_, value)| value).collect(),
            Expr::Select { arms, .. } | Expr::CoMatch { arms, .. } => {
                arms.iter().map(|arm| &arm.command).collect()
            }
            Expr::Let { value, body, .. } => {
                std::iter::once(&**value).chain(body.iter().map(|b| &**b)).collect()
            }
            Expr::Project { base, .. } => vec![base],
            Expr::Request { arg, .. } => vec![arg],
            Expr::Handle { body, clauses, ret, .. } => std::iter::once(&**body)
                .chain(clauses.iter().map(|c| &c.body))
                .chain(ret.iter().map(|(_, b)| &**b))
                .collect(),
            Expr::Handler { clauses, ret, .. } => clauses
                .iter()
                .map(|clause| &clause.body)
                .chain(ret.iter().map(|(_, body)| &**body))
                .collect(),
            Expr::WithHandler { handler, body } => vec![handler, body],
        }
    }
}

/// What a `cite` declaration brings into scope.
#[derive(Debug, Clone, PartialEq)]
pub enum UseImports {
    /// `use module::name;` — the member the path ends in.
    Member,
    /// `use Enum::*;` — every variant, bare.
    Glob,
    /// `use Enum::{A, B};` — the listed variants, bare.
    Names(Vec<String>),
}

/// One operation clause of a handler: `op(params): k => body`, the
/// carried continuation bound after the colon — or omitted, for a clause
/// that never resumes.
#[derive(Debug, Clone, PartialEq)]
pub struct HandleClause {
    pub op: String,
    pub params: Vec<String>,
    pub resume: String,
    pub body: Node<Expr>,
}

/// The polarity of a function declaration.
///
/// A positive function consumes values and produces a value. A negative
/// function consumes continuations and produces a continuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionPolarity {
    Positive,
    Negative,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Base(String),
    /// A declaration applied to type arguments: `List<i64>`, `Pair<A, B>`.
    Apply(String, Vec<Node<TypeExpr>>),
    Positive(Box<Node<TypeExpr>>),
    Negative(Box<Node<TypeExpr>>),
    /// `(A, B, …)`: a product of any number of components; `(,)` is the
    /// product of none. Nesting is significant: `(A, (B, C))` has two.
    Tensor(Vec<Node<TypeExpr>>),
    /// `(A ; B ; …)`: the negative product; `(;)`, of none, is what a command is.
    Par(Vec<Node<TypeExpr>>),
    /// `(A & B & …)`: a menu of anonymous items; `(&)` has none. A continuation
    /// row is one of these.
    With(Vec<Node<TypeExpr>>),
    /// `(A | B | …)`: an enum of anonymous alternatives; `(|)` is 0.
    Sum(Vec<Node<TypeExpr>>),
    Fun(Box<Node<TypeExpr>>, Box<Node<TypeExpr>>),
    /// A function type carrying an effect row: `(A -> B / {Exn, ..E})`.
    Effectful(Box<Node<TypeExpr>>, EffectRow),
    Dual(Box<Node<TypeExpr>>),
    /// A row given as a declaration's argument, `..E` or `{IO, ..E}`: the
    /// `..E` of `Seq<T, ..E>`.
    Row(EffectRow),
}

impl TypeExpr {
    /// Is this `(;)`, what a command is?
    pub fn is_bottom(&self) -> bool {
        matches!(self, TypeExpr::Par(items) if items.is_empty())
    }
}

/// An effect row: the concrete effects, and the declared row variables —
/// `{Exn, ..E}` extends the row variable `E` with `Exn`. A row variable is
/// declared like any generic parameter and written with `..`, the "rest"
/// spelling.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectRow {
    pub effects: Vec<Node<TypeExpr>>,
    pub tails: Vec<String>,
}

impl EffectRow {
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty() && self.tails.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// What the parameter binds. A bare name is the trivial pattern; a value
    /// parameter may be any irrefutable one, since a group *is* a pattern
    /// with typed leaves. A continuation parameter is a name: control leaves
    /// through it, and a name is what it leaves through.
    pub pattern: Pattern,
    /// `None` where the type was left out. A declaration's parameters always
    /// carry one — a declaration is an interface — so this is `None` only for
    /// the parameters of a local `mu`.
    pub ty: Option<TypeExpr>,
    pub is_continuation: bool,
}

impl Param {
    /// A parameter written as a bare name — the ordinary case.
    pub fn named(name: impl Into<String>, ty: Option<TypeExpr>, is_continuation: bool) -> Self {
        Param { pattern: Pattern::Ident(name.into()), ty, is_continuation }
    }

    /// The single name this parameter binds, when it binds one.
    pub fn name(&self) -> Option<&str> {
        self.pattern.binder_name()
    }

    /// How to speak of the parameter in a diagnostic.
    pub fn describe(&self) -> String {
        match self.name() {
            Some(name) => format!("`{name}`"),
            None => "this parameter".into(),
        }
    }
}

/// One arm of a `select`, written `pattern <= command`: the shape that
/// selects it, and the command that runs when it arrives. The pattern's
/// binders scope over the command.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectArm {
    pub pattern: Pattern,
    pub command: Node<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Node<Expr>,
}

/// What a pattern names: a declaration outright, or a variant whose
/// declaration the caller looks up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Named<'a> {
    Declaration(&'a str),
    Variant(&'a str),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Wildcard,
    Ident(String),
    Int(i64),
    Str(String),
    Char(char),
    Float(f64),
    Or(Vec<Pattern>),
    Range {
        start: Box<Pattern>,
        end: Box<Pattern>,
    },
    Binding {
        name: String,
        pattern: Box<Pattern>,
    },
    Rest,
    Tuple(Vec<Pattern>),
    /// `(p & q)` — the bundle copattern: the anonymous menu's counterpart
    /// to the tuple pattern, binding each exit.
    Bundle(Vec<Pattern>),
    /// `::0(p)` — the alternative at a position of a sum, its payload matched
    /// by `p`.
    Inject {
        index: usize,
        pattern: Box<Pattern>,
    },
    Data {
        name: String,
        fields: Vec<(String, Pattern)>,
    },
    Enum {
        name: String,
        variant: String,
        fields: Vec<Pattern>,
    },
    /// A request shape: the destructor it demands, and a
    /// pattern for the continuation the request carries. In `match` the
    /// pattern is a binder naming that continuation; in `mu` it may itself
    /// be a request shape — a nested copattern, `tail: head: out`.
    Dtor {
        dtor: String,
        arg: Box<Pattern>,
    },
}

impl Pattern {
    /// The single name a trivial binder introduces. A `let` or a parameter
    /// written as a bare name is this pattern, and the paths that only ever
    /// saw a name take it directly.
    pub fn binder_name(&self) -> Option<&str> {
        match self {
            Pattern::Ident(name) => Some(name),
            _ => None,
        }
    }

    /// The type this pattern names, when it names one. A `select` whose type
    /// is left out reads it off its arms: `S { … }` names its struct,
    /// `Color::Red(x)` its enum, and a bare `Red` its variant, whose
    /// declaration the caller resolves. A tuple or a plain binder names
    /// nothing — a product and an atom have no name of their own.
    pub fn names(&self) -> Option<Named<'_>> {
        match self {
            Pattern::Data { name, .. } => Some(Named::Declaration(name)),
            Pattern::Enum { name, variant, .. } if variant.is_empty() => Some(Named::Variant(name)),
            Pattern::Enum { name, .. } => Some(Named::Declaration(name)),
            Pattern::Ident(name) => Some(Named::Variant(name)),
            Pattern::Binding { pattern, .. } => pattern.names(),
            _ => None,
        }
    }
}

/// The polarity a generic parameter declares: a `+T` stands for positive
/// types, a `-T` for negative ones, and a `*T` for either. A type variable
/// carries no polarity of its own, so a generic parameter states its scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamPolarity {
    Positive,
    Negative,
    Any,
}

impl ParamPolarity {
    /// The other polarity: what the dual of such a type has.
    pub fn flipped(self) -> Self {
        match self {
            ParamPolarity::Positive => ParamPolarity::Negative,
            ParamPolarity::Negative => ParamPolarity::Positive,
            ParamPolarity::Any => ParamPolarity::Any,
        }
    }

    /// The mark a declaration writes for it.
    pub fn mark(self) -> char {
        match self {
            ParamPolarity::Positive => '+',
            ParamPolarity::Negative => '-',
            ParamPolarity::Any => '*',
        }
    }
}

/// When a `let` computes its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LetMode {
    /// `let`: where it is written.
    Follow,
    /// `let+`: now, whatever the type — the way to perform a computation's
    /// effects under the handler in scope.
    Now,
    /// `let-`: not here. The binding holds the computation, which runs each
    /// time its result is demanded — applied, cut into, or asked for an item.
    Delay,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decl {
    Data {
        name: String,
        /// `pub` — visible outside the module that declares it. A
        /// declaration is private by default, reachable by its own module
        /// and the modules nested inside it; a top-level declaration, which
        /// is in no module, is visible everywhere.
        is_public: bool,
        type_params: Vec<String>,
        /// The polarity each type parameter declares, `<+T>` or `<-T>`.
        /// A row variable declares none.
        type_param_signs: Vec<(String, ParamPolarity)>,
        fields: Vec<(String, TypeExpr)>,
    },
    Enum {
        name: String,
        /// `pub` — visible outside the module that declares it. A
        /// declaration is private by default, reachable by its own module
        /// and the modules nested inside it; a top-level declaration, which
        /// is in no module, is visible everywhere.
        is_public: bool,
        type_params: Vec<String>,
        /// The polarity each type parameter declares, `<+T>` or `<-T>`.
        /// A row variable declares none.
        type_param_signs: Vec<(String, ParamPolarity)>,
        variants: Vec<(String, Vec<TypeExpr>)>,
    },
    /// `menu Name { item: Type, … }` — the negative additive: the mirror of
    /// `enum`. An enum value is one variant the producer chose; a menu value
    /// answers one item the consumer demands. Each item names a destructor
    /// and the type of the answer it delivers.
    Menu {
        name: String,
        /// `pub` — visible outside the module that declares it. A
        /// declaration is private by default, reachable by its own module
        /// and the modules nested inside it; a top-level declaration, which
        /// is in no module, is visible everywhere.
        is_public: bool,
        type_params: Vec<String>,
        /// The polarity each type parameter declares, `<+T>` or `<-T>`.
        /// A row variable declares none.
        type_param_signs: Vec<(String, ParamPolarity)>,
        /// The latent row: what a demand on a value of this menu may
        /// perform. Arms of a `mu` over the menu are checked against it,
        /// and every demand incurs it — the work of codata runs on the
        /// demander's schedule, so the row belongs to the type.
        effects: EffectRow,
        items: Vec<(String, TypeExpr)>,
    },
    /// `form Name { field: Type, … }` — the negative multiplicative: the
    /// mirror of `data`. A record carries every field at once; a form wants
    /// every field at once. Each field names what flows in, so a form
    /// denotes `(-A ; -B)`, and its demand is the record its fields describe.
    Form {
        name: String,
        /// `pub` — visible outside the module that declares it. A
        /// declaration is private by default, reachable by its own module
        /// and the modules nested inside it; a top-level declaration, which
        /// is in no module, is visible everywhere.
        is_public: bool,
        type_params: Vec<String>,
        /// The polarity each type parameter declares, `<+T>` or `<-T>`.
        /// A row variable declares none.
        type_param_signs: Vec<(String, ParamPolarity)>,
        /// The latent row: what feeding a value of this form may perform.
        effects: EffectRow,
        fields: Vec<(String, TypeExpr)>,
    },
    Fn {
        name: String,
        /// `pub` — visible outside the module that declares it. A
        /// declaration is private by default, reachable by its own module
        /// and the modules nested inside it; a top-level declaration, which
        /// is in no module, is visible everywhere.
        is_public: bool,
        type_params: Vec<String>,
        /// The polarity each type parameter declares, `<+T>` or `<-T>`.
        /// A row variable declares none.
        type_param_signs: Vec<(String, ParamPolarity)>,
        /// Trait bounds on the type parameters: `(T, Show)` for `<T: Show>`.
        bounds: Vec<TraitBound>,
        polarity: FunctionPolarity,
        params: Vec<Param>,
        return_type: Option<TypeExpr>,
        /// The effect row: operations this function may perform. Empty (a
        /// bare arrow) means pure.
        effects: EffectRow,
        body: Node<Expr>,
    },
    /// A declaration whose body is a command: it takes values and
    /// continuations and never returns. `mu` is the expression that captures
    /// the current continuation; this abstracts over one instead.
    Command {
        name: String,
        /// `pub` — visible outside the module that declares it. A
        /// declaration is private by default, reachable by its own module
        /// and the modules nested inside it; a top-level declaration, which
        /// is in no module, is visible everywhere.
        is_public: bool,
        type_params: Vec<String>,
        /// The polarity each type parameter declares, `<+T>` or `<-T>`.
        /// A row variable declares none.
        type_param_signs: Vec<(String, ParamPolarity)>,
        bounds: Vec<TraitBound>,
        value_params: Vec<Param>,
        continuation_params: Vec<Param>,
        return_type: Option<TypeExpr>,
        /// The effect row this command may perform.
        effects: EffectRow,
        body: Node<Expr>,
    },
    /// A module: a named scope of declarations. Resolution flattens it,
    /// qualifying each declaration as `module::name`.
    Mod {
        name: String,
        /// `pub` — visible outside the module that declares it. A
        /// declaration is private by default, reachable by its own module
        /// and the modules nested inside it; a top-level declaration, which
        /// is in no module, is visible everywhere.
        is_public: bool,
        decls: Vec<Node<Decl>>,
    },
    /// `use a::b::name;` — brings `name` into scope for the enclosing
    /// module.
    Use {
        path: Vec<String>,
        /// What the path brings in: the path's last member itself, every
        /// variant of the enum it names (`use List::*;`), or the listed
        /// variants (`use List::{Nil, Cons};`).
        imports: UseImports,
    },
    Const {
        name: String,
        /// `pub` — visible outside the module that declares it. A
        /// declaration is private by default, reachable by its own module
        /// and the modules nested inside it; a top-level declaration, which
        /// is in no module, is visible everywhere.
        is_public: bool,
        ty: TypeExpr,
        value: Node<Expr>,
    },
    /// A trait: a named set of method signatures over an implicit `Self`,
    /// and any type parameters the trait itself takes (`trait Into<+U>`).
    Trait {
        name: String,
        /// `pub` — visible outside the module that declares it. A
        /// declaration is private by default, reachable by its own module
        /// and the modules nested inside it; a top-level declaration, which
        /// is in no module, is visible everywhere.
        is_public: bool,
        /// `Into<+U>`'s `U`, in order. Empty when the trait is only `Self`.
        type_params: Vec<String>,
        /// The polarity each type parameter declares, `<+U>` or `<-U>`.
        type_param_signs: Vec<(String, ParamPolarity)>,
        /// `trait Ord: Eq + Hash` — parents, in the order written. A bound
        /// on the child carries a dictionary for each parent too.
        supers: Vec<SuperTrait>,
        /// Associated types, `type Item;`. Each impl writes the type.
        assocs: Vec<String>,
        methods: Vec<TraitMethod>,
    },
    /// An `impl Trait for Type { … }`: the methods that make `Type` satisfy
    /// `Trait`. Each method is a `Fn` or `Command` declaration with a body.
    Impl {
        trait_name: String,
        /// `Into<i64>`'s arguments. Empty when the trait takes none.
        trait_args: Vec<TypeExpr>,
        /// `impl<+T: Show>` type parameters and bounds, shared by the methods.
        type_params: Vec<String>,
        /// The polarity each type parameter declares, `<+T>` or `<-T>`.
        /// A row variable declares none.
        type_param_signs: Vec<(String, ParamPolarity)>,
        bounds: Vec<TraitBound>,
        for_type: TypeExpr,
        /// `type Item = i64;`, in the order written.
        assocs: Vec<(String, TypeExpr)>,
        methods: Vec<Node<Decl>>,
    },
    /// A named handler: `hand name / {IO} { clauses }`. Installed with
    /// `do expr name`. Each installation answers with these clauses the way
    /// an inline handler does, so the answer is the body's.
    Hand {
        name: String,
        /// `pub` — visible outside the module that declares it.
        is_public: bool,
        /// What the clauses perform on their own. `None` leaves that row to
        /// each installation, which is what a clause that runs a callback needs.
        effects: Option<EffectRow>,
        clauses: Vec<HandleClause>,
        forward: bool,
        ret: Option<(String, Box<Node<Expr>>)>,
    },
    /// An effect: a named set of operations a computation may perform.
    Effect {
        name: String,
        type_params: Vec<String>,
        type_param_signs: Vec<(String, ParamPolarity)>,
        /// `pub` — visible outside the module that declares it. A
        /// declaration is private by default, reachable by its own module
        /// and the modules nested inside it; a top-level declaration, which
        /// is in no module, is visible everywhere.
        is_public: bool,
        operations: Vec<EffectOp>,
    },
}

/// One operation of an effect: a value-returning signature. `choose() -> bool`.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectOp {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
}

/// A parent trait: `trait Ord: Eq`. Arguments are the child's type
/// parameters applied to the parent, `trait Foo<+U>: Bar<U>`.
#[derive(Debug, Clone, PartialEq)]
pub struct SuperTrait {
    pub trait_name: String,
    pub args: Vec<TypeExpr>,
}

/// One bound on a type parameter: `T: Into<String>` is `Into` applied to
/// `String`, and `T: Show` is `Show` applied to nothing. `pins` names
/// associated types the bound fixes, `T: Walk<Item = i64>`.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitBound {
    pub param: String,
    pub trait_name: String,
    pub args: Vec<TypeExpr>,
    pub pins: Vec<AssocPin>,
}

/// `Item = i64` inside a trait bound. The dictionary is still the trait's;
/// the pin is an equality checked where the parameter becomes a real type.
#[derive(Debug, Clone, PartialEq)]
pub struct AssocPin {
    pub name: String,
    pub ty: TypeExpr,
}

/// One method of a trait. A signature ends in `;`. A body is the default an
/// impl gets when it does not write the method. `Self` stands for the
/// implementing type.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitMethod {
    pub name: String,
    /// `true` for a `command` method (value and continuation groups); `false`
    /// for a `fn` method (one group, a return type).
    pub is_command: bool,
    pub polarity: FunctionPolarity,
    pub value_params: Vec<Param>,
    pub continuation_params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    /// The default body. `None` means each impl writes the method.
    pub body: Option<Node<Expr>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub decls: Vec<Node<Decl>>,
}
