import Slc.Core

/-
Effect rows for the SLC core.

An effect application is a name plus its arguments, so `Echo<i64>` and
`Echo<String>` are different effects. A row has at most one rigid tail.
Discharging a handler's concrete capabilities removes only applications
that are equal to one it names, and it never removes the tail. A handler
therefore does not answer an effect that arrived through the row
parameter, and a clause fixed at one instantiation does not discharge a
use at another.

`step` on this fragment returns a command typed at the same row.
-/

namespace Slc.Effect

abbrev EName := Nat

/-- An argument of an effect application. `base` is a solved type.
`param` is the parameter of the effect, the one a clause can fix. -/
inductive Arg where
  | base (code : Nat)
  | param (i : Nat)
deriving DecidableEq, Repr

/-- One effect application, the member of a row. -/
structure App where
  name : EName
  args : List Arg
deriving DecidableEq, Repr

/-- Concrete applications, and at most one rigid tail. -/
structure Row where
  effects : List App
  tail : Option Nat
deriving DecidableEq, Repr

namespace Row

def pure : Row := { effects := [], tail := none }

end Row

/-- The clause answers this performed application. -/
def answers (clause performed : App) : Bool :=
  clause == performed

/-- Remove the applications the handler names. The tail stays: a rigid
row variable is not a capability the handler was given. -/
def discharge (handled : List App) (row : Row) : Row where
  effects := row.effects.filter fun performed =>
    !(handled.any fun clause => answers clause performed)
  tail := row.tail

theorem discharge_keeps_tail (handled : List App) (row : Row) :
    (discharge handled row).tail = row.tail := by
  rfl

theorem distinct_instantiations (n : EName) (clause use : List Arg) (h : clause ≠ use) :
    ({ name := n, args := clause } : App) ≠ { name := n, args := use } := by
  intro eq
  cases eq
  exact h rfl

theorem answers_eq (clause performed : App) :
    answers clause performed = true ↔ clause = performed := by
  simp [answers, beq_iff_eq]

theorem answers_ne {clause performed : App} (h : clause ≠ performed) :
    answers clause performed = false := by
  simpa [answers, beq_eq_false_iff_ne] using h

/-- A use whose arguments differ from the clause is still in the row
after the handler discharges what it names. -/
theorem different_use_survives (n : EName) (clause use : List Arg) (h : clause ≠ use) :
    let handled : List App := [{ name := n, args := clause }]
    let performed : App := { name := n, args := use }
    let row : Row := { effects := [performed], tail := none }
    (discharge handled row).effects = [performed] := by
  simp [discharge, answers_ne (distinct_instantiations n clause use h)]

/-- Same name is not enough. The arguments have to be the clause's. -/
theorem same_name_different_args_is_not_answered
    (n : EName) (clause use : List Arg) (h : clause ≠ use) :
    answers { name := n, args := clause } { name := n, args := use } = false :=
  answers_ne (distinct_instantiations n clause use h)

/-
A command owes a row. `perform` adds one application. `handle` owes
whatever `discharge` leaves of the body's row, tail included. `step`
preserves that row.
-/

inductive Cmd where
  | ret
  | perform (e : App) (k : Cmd)
  | handle (handled : List App) (body : Cmd)
deriving Repr

/-- `c` owes `ρ`. -/
inductive Typed : Cmd → Row → Prop where
  | ret : Typed .ret Row.pure
  | perform {e rest k} :
      Typed k rest →
      Typed (.perform e k) { effects := e :: rest.effects, tail := rest.tail }
  | handle {handled body bodyRow} :
      Typed body bodyRow →
      Typed (.handle handled body) (discharge handled bodyRow)

/-- One reduction. Catching drops an application the handler names. An
application it does not name is performed outside the handler. -/
inductive Step : Cmd → Cmd → Prop where
  | ret (handled : List App) : Step (.handle handled .ret) .ret
  | caught (handled : List App) (e : App) (k : Cmd)
      (h : handled.any (fun clause => answers clause e) = true) :
      Step (.handle handled (.perform e k)) (.handle handled k)
  | bubble (handled : List App) (e : App) (k : Cmd)
      (h : handled.any (fun clause => answers clause e) = false) :
      Step (.handle handled (.perform e k)) (.perform e (.handle handled k))

/-- Catching an application the handler names leaves the residual of the rest. -/
theorem catch_preserves_row (handled : List App) (e : App) (rest : Row)
    (h : handled.any (fun clause => answers clause e) = true) :
    discharge handled { effects := e :: rest.effects, tail := rest.tail } =
      discharge handled rest := by
  simp [discharge, h]

/-- An application the handler does not name stays owed, in front of the
residual of the rest. The tail is the body's tail. -/
theorem bubble_preserves_row (handled : List App) (e : App) (rest : Row)
    (h : handled.any (fun clause => answers clause e) = false) :
    discharge handled { effects := e :: rest.effects, tail := rest.tail } =
      { effects := e :: (discharge handled rest).effects, tail := rest.tail } := by
  simp [discharge, h]

theorem preservation {c c' : Cmd} {ρ : Row} (typed : Typed c ρ) (step : Step c c') :
    Typed c' ρ := by
  cases step with
  | ret handled =>
      cases typed with
      | handle bodyTyped =>
          cases bodyTyped
          have : discharge handled Row.pure = Row.pure := by
            unfold discharge Row.pure
            rfl
          rw [this]
          exact Typed.ret
  | caught handled e k h =>
      cases typed with
      | handle bodyTyped =>
          cases bodyTyped with
          | perform restTyped =>
              rw [catch_preserves_row handled e _ h]
              exact Typed.handle restTyped
  | bubble handled e k h =>
      cases typed with
      | handle bodyTyped =>
          cases bodyTyped with
          | perform restTyped =>
              rw [bubble_preserves_row handled e _ h]
              exact Typed.perform (Typed.handle restTyped)

/-- The clause's instantiation does not answer a different use, so that
use steps out of the handler. -/
theorem different_use_bubbles (n : EName) (clause use : List Arg) (k : Cmd)
    (h : clause ≠ use) :
    let handled : List App := [{ name := n, args := clause }]
    let performed : App := { name := n, args := use }
    Step (.handle handled (.perform performed k))
      (.perform performed (.handle handled k)) := by
  refine Step.bubble _ _ _ ?_
  simp [answers_ne (distinct_instantiations n clause use h)]

/-- A rigid tail on the body is a rigid tail on the handler's answer. -/
theorem handle_keeps_rigid_tail (handled : List App) (body : Row) (v : Nat)
    (h : body.tail = some v) :
    (discharge handled body).tail = some v := by
  simpa [h] using discharge_keeps_tail handled body

end Slc.Effect
