/-
Subject reduction for the SLC core.

A command `⟨t ∥ e⟩` is well-typed when `t` proves `A` and `e` refutes `A`.
`step` returns a command (or a projected component) in the same contexts:
that return type is preservation. `progress` says a command with no free
variable is one of these redexes.

`dual(?i)` is not `?j`. A cut of two distinct inference variables is not a
command, because a command has one type.
-/

namespace Slc

inductive Ty where
  | atom (pos : Bool) (code : Nat)
  | tensor (ts : List Ty)
  | par (ts : List Ty)
  | sum (ts : List Ty)
  | withTy (ts : List Ty)
  | arr (dom cod : Ty)
  | stack (dom cod : Ty)
  | var (i : Nat)
  | dvar (i : Nat)
  | neg (inner : Ty)
  | aff (inner : Ty)
deriving Repr

namespace Ty

def bot : Ty := par []
def one : Ty := tensor []

mutual
def dual : Ty → Ty
  | .atom true c => .atom false c
  | .atom false c => .atom true c
  | .tensor ts => .par (mapDual ts)
  | .par ts => .tensor (mapDual ts)
  | .sum ts => .withTy (mapDual ts)
  | .withTy ts => .sum (mapDual ts)
  | .arr a b => .stack a b
  | .stack a b => .arr a b
  | .var i => .dvar i
  | .dvar i => .var i
  | .neg t => .aff t
  | .aff t => .neg t

def mapDual : List Ty → List Ty
  | [] => []
  | t :: ts => t.dual :: mapDual ts
end

def isBot : Ty → Bool
  | par [] => true
  | _ => false

/-- `A → B` as the design writes it: `(dual(A) ; B)`, and `dual(A)` when `B` is `(;)`. -/
def funEncoding (a b : Ty) : Ty :=
  match b.isBot with
  | true => a.dual
  | false => par [a.dual, b]

end Ty

mutual
theorem Ty.dual_dual : (t : Ty) → t.dual.dual = t
  | .atom true _ => rfl
  | .atom false _ => rfl
  | .tensor ts => by
      show (Ty.par (Ty.mapDual ts)).dual = Ty.tensor ts
      rw [show (Ty.par (Ty.mapDual ts)).dual = Ty.tensor (Ty.mapDual (Ty.mapDual ts)) from rfl]
      rw [map_dual_dual ts]
  | .par ts => by
      show (Ty.tensor (Ty.mapDual ts)).dual = Ty.par ts
      rw [show (Ty.tensor (Ty.mapDual ts)).dual = Ty.par (Ty.mapDual (Ty.mapDual ts)) from rfl]
      rw [map_dual_dual ts]
  | .sum ts => by
      show (Ty.withTy (Ty.mapDual ts)).dual = Ty.sum ts
      rw [show (Ty.withTy (Ty.mapDual ts)).dual = Ty.sum (Ty.mapDual (Ty.mapDual ts)) from rfl]
      rw [map_dual_dual ts]
  | .withTy ts => by
      show (Ty.sum (Ty.mapDual ts)).dual = Ty.withTy ts
      rw [show (Ty.sum (Ty.mapDual ts)).dual = Ty.withTy (Ty.mapDual (Ty.mapDual ts)) from rfl]
      rw [map_dual_dual ts]
  | .arr .. => rfl
  | .stack .. => rfl
  | .var _ => rfl
  | .dvar _ => rfl
  | .neg _ => rfl
  | .aff _ => rfl

theorem map_dual_dual : (ts : List Ty) → Ty.mapDual (Ty.mapDual ts) = ts
  | [] => rfl
  | t :: ts => by
      show t.dual.dual :: Ty.mapDual (Ty.mapDual ts) = t :: ts
      rw [Ty.dual_dual t, map_dual_dual ts]
end

theorem Ty.dual_inj {a b : Ty} (h : a.dual = b.dual) : a = b := by
  simpa [Ty.dual_dual] using congrArg Ty.dual h

theorem dual_var_ne_var (i j : Nat) : (Ty.var i).dual ≠ Ty.var j := by
  simp [Ty.dual]

theorem isBot_bot : Ty.bot.isBot = true := rfl

theorem funEncoding_bot (a : Ty) : Ty.funEncoding a Ty.bot = a.dual := by
  simp [Ty.funEncoding, Ty.isBot, Ty.bot]

theorem funEncoding_par (a b : Ty) (h : b.isBot = false) :
    Ty.funEncoding a b = .par [a.dual, b] := by
  simp [Ty.funEncoding, h]

inductive Var : List Ty → Ty → Type where
  | here {Γ A} : Var (A :: Γ) A
  | there {Γ A B} : Var Γ A → Var (B :: Γ) A

theorem Var.empty {A} (v : Var ([] : List Ty) A) : False := by cases v

mutual
inductive Term : List Ty → List Ty → Ty → Type where
  | var {Γ Δ A} : Var Γ A → Term Γ Δ A
  | lam {Γ Δ A B} : Term (A :: Γ) Δ B → Term Γ Δ (.arr A B)
  | mu {Γ Δ A} : Cmd Γ (A :: Δ) → Term Γ Δ A
  | tuple {Γ Δ As} : Terms Γ Δ As → Term Γ Δ (.tensor As)
  | tag {Γ Δ As} : (i : Fin As.length) → Term Γ Δ (As.get i) → Term Γ Δ (.sum As)
  | menu {Γ Δ As} : Menus Γ Δ As → Term Γ Δ (.withTy As)
  | co {Γ Δ A} : CoTerm Γ Δ A → Term Γ Δ (.neg A)
  | stackVal {Γ Δ A B} : Term Γ Δ A → CoTerm Γ Δ B → Term Γ Δ (.stack A B)

inductive CoTerm : List Ty → List Ty → Ty → Type where
  | covar {Γ Δ A} : Var Δ A → CoTerm Γ Δ A
  | app {Γ Δ A B} : Term Γ Δ A → CoTerm Γ Δ B → CoTerm Γ Δ (.arr A B)
  | muTilde {Γ Δ A} : Cmd (A :: Γ) Δ → CoTerm Γ Δ A
  | prj {Γ Δ As} : (i : Fin As.length) → CoTerm Γ Δ (.tensor As)
  | cases {Γ Δ As} : Cases Γ Δ As → CoTerm Γ Δ (.sum As)
  | muTensor {Γ Δ As} : Cmd (.tensor As :: Γ) Δ → CoTerm Γ Δ (.tensor As)
  | dtor {Γ Δ As} : (i : Fin As.length) → CoTerm Γ Δ (As.get i) → CoTerm Γ Δ (.withTy As)
  | unapp {Γ Δ A} : Term Γ Δ A → CoTerm Γ Δ (.neg A)

inductive Cmd : List Ty → List Ty → Type where
  | cut {Γ Δ A} : Term Γ Δ A → CoTerm Γ Δ A → Cmd Γ Δ

inductive Terms : List Ty → List Ty → List Ty → Type where
  | nil {Γ Δ} : Terms Γ Δ []
  | cons {Γ Δ A As} : Term Γ Δ A → Terms Γ Δ As → Terms Γ Δ (A :: As)

inductive Cases : List Ty → List Ty → List Ty → Type where
  | nil {Γ Δ} : Cases Γ Δ []
  | cons {Γ Δ A As} : Cmd (A :: Γ) Δ → Cases Γ Δ As → Cases Γ Δ (A :: As)

inductive Menus : List Ty → List Ty → List Ty → Type where
  | nil {Γ Δ} : Menus Γ Δ []
  | cons {Γ Δ A As} : Cmd Γ (A :: Δ) → Menus Γ Δ As → Menus Γ Δ (A :: As)
end

/-- Two distinct inference variables are not one type, so they are not the
two sides of a command. -/
theorem distinct_vars_are_not_a_cut (i j : Nat) (h : i ≠ j) :
    Ty.var i ≠ Ty.var j := by
  intro eq
  cases eq
  exact h rfl

abbrev Rename (Γ Γ' : List Ty) := ∀ {A}, Var Γ A → Var Γ' A
abbrev CoRename (Δ Δ' : List Ty) := ∀ {A}, Var Δ A → Var Δ' A

def Rename.lift {Γ Γ' A} (ρ : Rename Γ Γ') : Rename (A :: Γ) (A :: Γ') :=
  fun {B} v =>
    match v with
    | .here => .here
    | .there v => .there (ρ v)

def CoRename.lift {Δ Δ' A} (ρ : CoRename Δ Δ') : CoRename (A :: Δ) (A :: Δ') :=
  fun {B} v =>
    match v with
    | .here => .here
    | .there v => .there (ρ v)

mutual
def Term.rename {Γ Γ' Δ A} (ρ : Rename Γ Γ') : Term Γ Δ A → Term Γ' Δ A
  | .var v => .var (ρ v)
  | .lam body => .lam (body.rename ρ.lift)
  | .mu c => .mu (c.rename ρ)
  | .tuple ts => .tuple (ts.rename ρ)
  | .tag i p => .tag i (p.rename ρ)
  | .menu ms => .menu (ms.rename ρ)
  | .co e => .co (e.rename ρ)
  | .stackVal v e => .stackVal (v.rename ρ) (e.rename ρ)

def CoTerm.rename {Γ Γ' Δ A} (ρ : Rename Γ Γ') : CoTerm Γ Δ A → CoTerm Γ' Δ A
  | .covar v => .covar v
  | .app v e => .app (v.rename ρ) (e.rename ρ)
  | .muTilde c => .muTilde (c.rename ρ.lift)
  | .prj i => .prj i
  | .cases cs => .cases (cs.rename ρ)
  | .muTensor c => .muTensor (c.rename ρ.lift)
  | .dtor i e => .dtor i (e.rename ρ)
  | .unapp v => .unapp (v.rename ρ)

def Cmd.rename {Γ Γ' Δ} (ρ : Rename Γ Γ') : Cmd Γ Δ → Cmd Γ' Δ
  | .cut t e => .cut (t.rename ρ) (e.rename ρ)

def Terms.rename {Γ Γ' Δ As} (ρ : Rename Γ Γ') : Terms Γ Δ As → Terms Γ' Δ As
  | .nil => .nil
  | .cons t ts => .cons (t.rename ρ) (ts.rename ρ)

def Cases.rename {Γ Γ' Δ As} (ρ : Rename Γ Γ') : Cases Γ Δ As → Cases Γ' Δ As
  | .nil => .nil
  | .cons body rest => .cons (body.rename ρ.lift) (rest.rename ρ)

def Menus.rename {Γ Γ' Δ As} (ρ : Rename Γ Γ') : Menus Γ Δ As → Menus Γ' Δ As
  | .nil => .nil
  | .cons body rest => .cons (body.rename ρ) (rest.rename ρ)
end

mutual
def Term.renameΔ {Γ Δ Δ' A} (ρ : CoRename Δ Δ') : Term Γ Δ A → Term Γ Δ' A
  | .var v => .var v
  | .lam body => .lam (body.renameΔ ρ)
  | .mu c => .mu (c.renameΔ ρ.lift)
  | .tuple ts => .tuple (ts.renameΔ ρ)
  | .tag i p => .tag i (p.renameΔ ρ)
  | .menu ms => .menu (ms.renameΔ ρ)
  | .co e => .co (e.renameΔ ρ)
  | .stackVal v e => .stackVal (v.renameΔ ρ) (e.renameΔ ρ)

def CoTerm.renameΔ {Γ Δ Δ' A} (ρ : CoRename Δ Δ') : CoTerm Γ Δ A → CoTerm Γ Δ' A
  | .covar v => .covar (ρ v)
  | .app v e => .app (v.renameΔ ρ) (e.renameΔ ρ)
  | .muTilde c => .muTilde (c.renameΔ ρ)
  | .prj i => .prj i
  | .cases cs => .cases (cs.renameΔ ρ)
  | .muTensor c => .muTensor (c.renameΔ ρ)
  | .dtor i e => .dtor i (e.renameΔ ρ)
  | .unapp v => .unapp (v.renameΔ ρ)

def Cmd.renameΔ {Γ Δ Δ'} (ρ : CoRename Δ Δ') : Cmd Γ Δ → Cmd Γ Δ'
  | .cut t e => .cut (t.renameΔ ρ) (e.renameΔ ρ)

def Terms.renameΔ {Γ Δ Δ' As} (ρ : CoRename Δ Δ') : Terms Γ Δ As → Terms Γ Δ' As
  | .nil => .nil
  | .cons t ts => .cons (t.renameΔ ρ) (ts.renameΔ ρ)

def Cases.renameΔ {Γ Δ Δ' As} (ρ : CoRename Δ Δ') : Cases Γ Δ As → Cases Γ Δ' As
  | .nil => .nil
  | .cons body rest => .cons (body.renameΔ ρ) (rest.renameΔ ρ)

def Menus.renameΔ {Γ Δ Δ' As} (ρ : CoRename Δ Δ') : Menus Γ Δ As → Menus Γ Δ' As
  | .nil => .nil
  | .cons body rest => .cons (body.renameΔ ρ.lift) (rest.renameΔ ρ)
end

abbrev Subst (Γ Γ' Δ : List Ty) := ∀ {A}, Var Γ A → Term Γ' Δ A
abbrev CoSubst (Δ Δ' Γ : List Ty) := ∀ {A}, Var Δ A → CoTerm Γ Δ' A

def Subst.lift {Γ Γ' Δ B} (σ : Subst Γ Γ' Δ) : Subst (B :: Γ) (B :: Γ') Δ :=
  fun {A} v =>
    match v with
    | .here => .var .here
    | .there v => (σ v).rename (fun {C} w => .there w)

def Subst.liftΔ {Γ Γ' Δ C} (σ : Subst Γ Γ' Δ) : Subst Γ Γ' (C :: Δ) :=
  fun {A} v => (σ v).renameΔ (fun {B} w => .there w)

def CoSubst.lift {Δ Δ' Γ B} (σ : CoSubst Δ Δ' Γ) : CoSubst Δ Δ' (B :: Γ) :=
  fun {A} v => (σ v).rename (fun {C} w => .there w)

def CoSubst.liftΔ {Δ Δ' Γ C} (σ : CoSubst Δ Δ' Γ) : CoSubst (C :: Δ) (C :: Δ') Γ :=
  fun {A} v =>
    match v with
    | .here => .covar .here
    | .there v => (σ v).renameΔ (fun {B} w => .there w)

def Subst.single {Γ Δ A} (s : Term Γ Δ A) : Subst (A :: Γ) Γ Δ :=
  fun {B} v =>
    match v with
    | .here => s
    | .there v => .var v

def CoSubst.single {Γ Δ A} (e : CoTerm Γ Δ A) : CoSubst (A :: Δ) Δ Γ :=
  fun {B} v =>
    match v with
    | .here => e
    | .there v => .covar v

mutual
def Term.subst {Γ Γ' Δ A} (σ : Subst Γ Γ' Δ) : Term Γ Δ A → Term Γ' Δ A
  | .var v => σ v
  | .lam body => .lam (body.subst σ.lift)
  | .mu c => .mu (c.subst σ.liftΔ)
  | .tuple ts => .tuple (ts.subst σ)
  | .tag i p => .tag i (p.subst σ)
  | .menu ms => .menu (ms.subst σ)
  | .co e => .co (e.subst σ)
  | .stackVal v e => .stackVal (v.subst σ) (e.subst σ)

def CoTerm.subst {Γ Γ' Δ A} (σ : Subst Γ Γ' Δ) : CoTerm Γ Δ A → CoTerm Γ' Δ A
  | .covar v => .covar v
  | .app v e => .app (v.subst σ) (e.subst σ)
  | .muTilde c => .muTilde (c.subst σ.lift)
  | .prj i => .prj i
  | .cases cs => .cases (cs.subst σ)
  | .muTensor c => .muTensor (c.subst σ.lift)
  | .dtor i e => .dtor i (e.subst σ)
  | .unapp v => .unapp (v.subst σ)

def Cmd.subst {Γ Γ' Δ} (σ : Subst Γ Γ' Δ) : Cmd Γ Δ → Cmd Γ' Δ
  | .cut t e => .cut (t.subst σ) (e.subst σ)

def Terms.subst {Γ Γ' Δ As} (σ : Subst Γ Γ' Δ) : Terms Γ Δ As → Terms Γ' Δ As
  | .nil => .nil
  | .cons t ts => .cons (t.subst σ) (ts.subst σ)

def Cases.subst {Γ Γ' Δ As} (σ : Subst Γ Γ' Δ) : Cases Γ Δ As → Cases Γ' Δ As
  | .nil => .nil
  | .cons body rest => .cons (body.subst σ.lift) (rest.subst σ)

def Menus.subst {Γ Γ' Δ As} (σ : Subst Γ Γ' Δ) : Menus Γ Δ As → Menus Γ' Δ As
  | .nil => .nil
  | .cons body rest => .cons (body.subst σ.liftΔ) (rest.subst σ)
end

mutual
def Term.substΔ {Γ Δ Δ' A} (σ : CoSubst Δ Δ' Γ) : Term Γ Δ A → Term Γ Δ' A
  | .var v => .var v
  | .lam body => .lam (body.substΔ σ.lift)
  | .mu c => .mu (c.substΔ σ.liftΔ)
  | .tuple ts => .tuple (ts.substΔ σ)
  | .tag i p => .tag i (p.substΔ σ)
  | .menu ms => .menu (ms.substΔ σ)
  | .co e => .co (e.substΔ σ)
  | .stackVal v e => .stackVal (v.substΔ σ) (e.substΔ σ)

def CoTerm.substΔ {Γ Δ Δ' A} (σ : CoSubst Δ Δ' Γ) : CoTerm Γ Δ A → CoTerm Γ Δ' A
  | .covar v => σ v
  | .app v e => .app (v.substΔ σ) (e.substΔ σ)
  | .muTilde c => .muTilde (c.substΔ σ.lift)
  | .prj i => .prj i
  | .cases cs => .cases (cs.substΔ σ)
  | .muTensor c => .muTensor (c.substΔ σ.lift)
  | .dtor i e => .dtor i (e.substΔ σ)
  | .unapp v => .unapp (v.substΔ σ)

def Cmd.substΔ {Γ Δ Δ'} (σ : CoSubst Δ Δ' Γ) : Cmd Γ Δ → Cmd Γ Δ'
  | .cut t e => .cut (t.substΔ σ) (e.substΔ σ)

def Terms.substΔ {Γ Δ Δ' As} (σ : CoSubst Δ Δ' Γ) : Terms Γ Δ As → Terms Γ Δ' As
  | .nil => .nil
  | .cons t ts => .cons (t.substΔ σ) (ts.substΔ σ)

def Cases.substΔ {Γ Δ Δ' As} (σ : CoSubst Δ Δ' Γ) : Cases Γ Δ As → Cases Γ Δ' As
  | .nil => .nil
  | .cons body rest => .cons (body.substΔ σ.lift) (rest.substΔ σ)

def Menus.substΔ {Γ Δ Δ' As} (σ : CoSubst Δ Δ' Γ) : Menus Γ Δ As → Menus Γ Δ' As
  | .nil => .nil
  | .cons body rest => .cons (body.substΔ σ.liftΔ) (rest.substΔ σ)
end

def Cmd.substTerm {Γ Δ A} (c : Cmd (A :: Γ) Δ) (s : Term Γ Δ A) : Cmd Γ Δ :=
  c.subst (Subst.single s)

def Cmd.substCo {Γ Δ A} (c : Cmd Γ (A :: Δ)) (e : CoTerm Γ Δ A) : Cmd Γ Δ :=
  c.substΔ (CoSubst.single e)

def Terms.get {Γ Δ As} (ts : Terms Γ Δ As) (i : Fin As.length) : Term Γ Δ (As.get i) :=
  match As, ts, i with
  | _ :: _, .cons t _, ⟨0, _⟩ => t
  | _ :: tail, .cons _ rest, ⟨n + 1, h⟩ =>
      rest.get ⟨n, by simpa [List.length_cons] using h⟩
  | [], .nil, i => False.elim (Nat.not_lt_zero i.val i.isLt)

def Cases.get {Γ Δ As} (cs : Cases Γ Δ As) (i : Fin As.length) : Cmd ((As.get i) :: Γ) Δ :=
  match As, cs, i with
  | _ :: _, .cons body _, ⟨0, _⟩ => body
  | _ :: tail, .cons _ rest, ⟨n + 1, h⟩ =>
      rest.get ⟨n, by simpa [List.length_cons] using h⟩
  | [], .nil, i => False.elim (Nat.not_lt_zero i.val i.isLt)

def Menus.get {Γ Δ As} (ms : Menus Γ Δ As) (i : Fin As.length) : Cmd Γ ((As.get i) :: Δ) :=
  match As, ms, i with
  | _ :: _, .cons body _, ⟨0, _⟩ => body
  | _ :: tail, .cons _ rest, ⟨n + 1, h⟩ =>
      rest.get ⟨n, by simpa [List.length_cons] using h⟩
  | [], .nil, i => False.elim (Nat.not_lt_zero i.val i.isLt)

inductive Reduct (Γ Δ : List Ty) where
  | cmd (c : Cmd Γ Δ)
  | component {A} (t : Term Γ Δ A)

def Term.cast {Γ Δ A B} (h : A = B) (t : Term Γ Δ A) : Term Γ Δ B := h ▸ t
def CoTerm.cast {Γ Δ A B} (h : A = B) (e : CoTerm Γ Δ A) : CoTerm Γ Δ B := h ▸ e

/-- One reduction step. A `some` reduct is typed in the same contexts. -/
def step {Γ Δ} (c : Cmd Γ Δ) : Option (Reduct Γ Δ) :=
  match c with
  | .cut (.mu body) e => some (.cmd (body.substCo e))
  | .cut (.lam body) (.app v tail) =>
      some (.cmd (.cut v (.muTilde (.cut body (tail.rename fun {_C} w => .there w)))))
  | .cut t (.muTilde body) => some (.cmd (body.substTerm t))
  | .cut (.tuple ts) (.prj i) => some (.component (ts.get i))
  | .cut (.tuple ts) (.muTensor body) => some (.cmd (body.substTerm (.tuple ts)))
  | .cut (.tag i payload) (.cases cs) => some (.cmd ((cs.get i).substTerm payload))
  | .cut (.menu ms) (.dtor i e) => some (.cmd ((ms.get i).substCo e))
  | .cut (.co e') (.unapp v) => some (.cmd (.cut v e'))
  | _ => none

/-- A closed command reduces. The result is typed in the empty contexts,
which is preservation for that step. -/
def progress : Cmd [] [] → Reduct [] []
  | .cut (.var v) _ => nomatch (Var.empty v)
  | .cut (.mu body) e => .cmd (body.substCo e)
  | .cut _ (.covar a) => nomatch (Var.empty a)
  | .cut (.lam body) (.muTilde c) => .cmd (c.substTerm (.lam body))
  | .cut (.lam body) (.app v tail) =>
      .cmd (.cut v (.muTilde (.cut body (tail.rename fun {_C} w => .there w))))
  | .cut (.tuple ts) (.muTilde c) => .cmd (c.substTerm (.tuple ts))
  | .cut (.tuple ts) (.prj i) => .component (ts.get i)
  | .cut (.tuple ts) (.muTensor body) => .cmd (body.substTerm (.tuple ts))
  | .cut (.tag i payload) (.muTilde c) => .cmd (c.substTerm (.tag i payload))
  | .cut (.tag i payload) (.cases cs) => .cmd ((cs.get i).substTerm payload)
  | .cut (.menu ms) (.muTilde c) => .cmd (c.substTerm (.menu ms))
  | .cut (.menu ms) (.dtor i e) => .cmd ((ms.get i).substCo e)
  | .cut (.co e') (.muTilde c) => .cmd (c.substTerm (.co e'))
  | .cut (.co e') (.unapp v) => .cmd (.cut v e')
  | .cut (.stackVal v e) (.muTilde c) => .cmd (c.substTerm (.stackVal v e))

theorem step_eq_progress (c : Cmd [] []) : step c = some (progress c) := by
  match c with
  | .cut (.var v) _ => nomatch (Var.empty v)
  | .cut (.mu _body) _e => rfl
  | .cut _t (.covar a) => nomatch (Var.empty a)
  | .cut (.lam _body) (.muTilde _c) => rfl
  | .cut (.lam _body) (.app _v _tail) => rfl
  | .cut (.tuple _ts) (.muTilde _c) => rfl
  | .cut (.tuple _ts) (.prj _i) => rfl
  | .cut (.tuple _ts) (.muTensor _body) => rfl
  | .cut (.tag _i _payload) (.muTilde _c) => rfl
  | .cut (.tag _i _payload) (.cases _cs) => rfl
  | .cut (.menu _ms) (.muTilde _c) => rfl
  | .cut (.menu _ms) (.dtor _i _e) => rfl
  | .cut (.co _e') (.muTilde _c) => rfl
  | .cut (.co _e') (.unapp _v) => rfl
  | .cut (.stackVal _v _e) (.muTilde _c) => rfl

end Slc
