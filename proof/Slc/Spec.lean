import Slc.Core

/-
Type soundness for `spec`.

A specification declares method types in terms of `Self`, trait parameters,
and associated types. An impl inhabits those types with `Self` and the
parameters instantiated, and with each associated type replaced by the type
the impl chose. A coherent set of impls contains one impl per key. A call
is a core term of the instantiated result type, and its command reduces by
the core's application rule.
-/

namespace Slc

inductive STy where
  | base (code : Nat)
  | self
  | param (index : Nat)
  | assoc (index : Nat)
  | arrow (dom cod : STy)
deriving DecidableEq, Repr

namespace STy

/-- Instantiate `Self`, trait parameters, and associated types. -/
def subst (selfCode : Nat) (param assocVal : Nat → Nat) : STy → STy
  | .base c => .base c
  | .self => .base selfCode
  | .param i => .base (param i)
  | .assoc i => .base (assocVal i)
  | .arrow a b => .arrow (a.subst selfCode param assocVal) (b.subst selfCode param assocVal)

/-- The core type of an instantiated surface type. `Self` and parameters
have already been replaced, so the leftover cases are unused. -/
def core : STy → Ty
  | .base c => .atom true c
  | .arrow a b => .arr a.core b.core
  | .self => .atom true 0
  | .param _ => .atom true 0
  | .assoc _ => .atom true 0

end STy

theorem assoc_becomes_impl_choice (i : Nat) (selfCode : Nat) (param assocVal : Nat → Nat) :
    (STy.assoc i).subst selfCode param assocVal = .base (assocVal i) := rfl

/-- A pin `Item = c` holds when the impl chose `c`. -/
theorem pinned_assoc (i : Nat) (pin : Nat) (selfCode : Nat) (param assocVal : Nat → Nat)
    (h : assocVal i = pin) :
    (STy.assoc i).subst selfCode param assocVal = .base pin := by
  simp [STy.subst, h]

structure Sig where
  arg : STy
  ret : STy
deriving Repr

def Sig.argCore (sig : Sig) (selfCode : Nat) (param assocVal : Nat → Nat) : Ty :=
  (sig.arg.subst selfCode param assocVal).core

def Sig.retCore (sig : Sig) (selfCode : Nat) (param assocVal : Nat → Nat) : Ty :=
  (sig.ret.subst selfCode param assocVal).core

structure Spec where
  id : Nat
  methods : List Sig
deriving Repr

/-- An impl method is a core term of the specification's type at this
`Self`, these parameters, and this choice of associated types. -/
abbrev MethodImpl (sig : Sig) (selfCode : Nat) (param assocVal : Nat → Nat) : Type :=
  Term [sig.argCore selfCode param assocVal] [] (sig.retCore selfCode param assocVal)

/-- The dictionary entry is that method, as a function. Its type is the
instantiated specification. -/
def dictionary (sig : Sig) (selfCode : Nat) (param assocVal : Nat → Nat)
    (body : MethodImpl sig selfCode param assocVal) :
    Term [] [] (.arr (sig.argCore selfCode param assocVal) (sig.retCore selfCode param assocVal)) :=
  .lam body

theorem dictionary_is_the_impl (sig : Sig) (selfCode : Nat) (param assocVal : Nat → Nat)
    (body : MethodImpl sig selfCode param assocVal) :
    dictionary sig selfCode param assocVal body = .lam body := rfl

def vacLift (B : Ty) : CoRename [] [B] :=
  fun {_A} v => nomatch (Var.empty v)

def Term.liftEmptyΔ {Γ A B} (t : Term Γ [] A) : Term Γ [B] A :=
  t.renameΔ (vacLift B)

def CoTerm.liftEmptyΔ {Γ A B} (e : CoTerm Γ [] A) : CoTerm Γ [B] A :=
  e.renameΔ (vacLift B)

/-- `⟨f ∥ arg · α⟩`, the core command of a call. `α` is the result continuation. -/
def callCmd {A B : Ty} (f : Term [] [] (.arr A B)) (arg : Term [] [] A) : Cmd [] [B] :=
  .cut (f.liftEmptyΔ (B := B)) (.app (arg.liftEmptyΔ (B := B)) (.covar .here))

/-- The call as a term of the result type. -/
def applyFun {A B : Ty} (f : Term [] [] (.arr A B)) (arg : Term [] [] A) : Term [] [] B :=
  .mu (callCmd f arg)

/-- A concrete impl call reduces by the core application rule, so the core
preservation theorem applies to it. -/
theorem impl_call_steps {A B : Ty} (body : Term [A] [] B) (arg : Term [] [] A) :
    step (callCmd (.lam body) arg) =
      some (.cmd (.cut (arg.liftEmptyΔ (B := B))
        (.muTilde (.cut (body.liftEmptyΔ (B := B))
          ((CoTerm.covar (.here : Var [B] B)).rename fun {_C} w => .there w))))) := by
  rfl

/-- Passing a dictionary of the spec's type into a bounded call preserves
the command's type. The dictionary is the hidden argument of `<T: Spec>`. -/
def instantiate {A B : Ty} (call : Cmd [.arr A B] [B]) (dict : Term [] [] (.arr A B)) :
    Cmd [] [B] :=
  call.substTerm (dict.liftEmptyΔ (B := B))

structure ImplKey where
  traitId : Nat
  selfCode : Nat
  args : List Nat
deriving DecidableEq, Repr

inductive Selected : List ImplKey → ImplKey → Type where
  | here {k ks} : Selected (k :: ks) k
  | there {k k' ks} : Selected ks k → Selected (k' :: ks) k

/-- One impl per key: each new key is absent from the keys already chosen. -/
inductive Coherent : List ImplKey → Prop where
  | nil : Coherent []
  | cons {head : ImplKey} {tail : List ImplKey}
      (fresh : Selected tail head → False)
      (tailOk : Coherent tail) : Coherent (head :: tail)

theorem selected_unique {keys : List ImplKey} {k : ImplKey}
    (h : Coherent keys) (s1 s2 : Selected keys k) : s1 = s2 := by
  induction s1 with
  | here =>
      cases s2 with
      | here => rfl
      | there s2 =>
          cases h with
          | cons fresh _ => exact False.elim (fresh s2)
  | there s1 ih =>
      cases s2 with
      | here =>
          cases h with
          | cons fresh _ => exact False.elim (fresh s1)
      | there s2 =>
          cases h with
          | cons _ tailOk => exact congrArg Selected.there (ih tailOk s2)

theorem not_selected {keys : List ImplKey} {k : ImplKey}
    (h : ∀ i : Fin keys.length, keys.get i ≠ k) : Selected keys k → False := by
  intro s
  induction s with
  | here =>
      exact h ⟨0, by simp⟩ rfl
  | there s ih =>
      exact ih (fun i => by simpa [List.get_cons_succ] using h i.succ)

/-- A child impl is accepted only when the parent is implemented for the
same key. The witness is the parent's selection. -/
structure Covered (parent child : List ImplKey) where
  parentOf : ∀ k, Selected child k → Selected parent k

def child_has_parent {parent child : List ImplKey} (h : Covered parent child)
    {k : ImplKey} (s : Selected child k) : Selected parent k :=
  h.parentOf k s

/-- Method names belong to one specification. -/
def nodup : List Nat → Bool
  | [] => true
  | n :: ns => !ns.contains n && nodup ns

theorem nodup_tail {n : Nat} {ns : List Nat} (h : nodup (n :: ns) = true) : nodup ns = true := by
  simp [nodup] at h
  exact h.2

theorem nodup_head {n : Nat} {ns : List Nat} (h : nodup (n :: ns) = true) : ¬ n ∈ ns := by
  simp [nodup] at h
  exact h.1

end Slc
