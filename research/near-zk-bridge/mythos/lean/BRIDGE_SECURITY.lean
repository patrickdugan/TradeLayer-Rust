/-
BRIDGE-SEC-02 / Mythos audit: machine-checked accounting and authorization lemmas
for the TradeLayer <-> NEAR stablecoin bridge (ZTB). Core Lean 4 only, no Mathlib.

Scope (read before citing):
* This is an abstract transition system. Event identities and amounts are `Nat`.
  It does NOT model NEAR consensus, Nova soundness, SHA-256, Borsh, or Bitcoin.
* Those are folded into five Boolean assumptions (`Assumptions`). The main theorem
  `secure_solvent` holds when all five are true. For each assumption there is an
  explicit counterexample trace (section 4) showing insolvency once that single
  assumption is dropped. That is the "smallest assumption set" claim, at this level
  of abstraction: each of A1..A5 is individually necessary for the model's guarantee.
* The source-chain history `src` is the set of finalized, economically retained
  deposits into the vault. In BRIDGE-SEC-01 that set is a trusted fixture. Here it is
  still an input: A1 says the admission relation only accepts members of `src`.
  Discharging A1 is the job of NEARHEAD01/NEARPAY01 (consensus + receipt proofs).

Assumptions:
  A1 auth        : admitted events are finalized retained source deposits
                   (consensus authentication + proof soundness + statement binding)
  A2 nullify     : each event identity is consumed at most once
  A3 finality    : finalized source history never reverts
  A4 custody     : vault reserves leave only through the payout relation
  A5 continuity  : verifier/key/checkpoint migrations preserve consumed identities
-/

namespace BridgeSecurity

/-! ## 1. List utilities (self-contained, no library lemmas) -/

def sumBy (f : Nat → Nat) : List Nat → Nat
  | [] => 0
  | a :: l => f a + sumBy f l

def rm (x : Nat) : List Nat → List Nat
  | [] => []
  | a :: l => if a = x then l else a :: rm x l

/-- No repeated identities. -/
def Fresh : List Nat → Prop
  | [] => True
  | a :: l => a ∉ l ∧ Fresh l

theorem sumBy_rm (f : Nat → Nat) :
    ∀ {l : List Nat} {x : Nat}, x ∈ l → sumBy f l = f x + sumBy f (rm x l)
  | [], _, h => by cases h
  | a :: l, x, h => by
    by_cases hax : a = x
    · subst hax; simp [sumBy, rm]
    · have hx : x ∈ l := by
        cases h with
        | head => exact absurd rfl hax
        | tail _ h' => exact h'
      have ih := sumBy_rm f hx
      simp [sumBy, rm, hax, ih]
      omega

theorem mem_rm {y x : Nat} : ∀ {l : List Nat}, y ∈ l → y ≠ x → y ∈ rm x l
  | [], h, _ => by cases h
  | a :: l, h, hne => by
    by_cases hax : a = x
    · subst hax
      simp only [rm, ↓reduceIte]
      cases h with
      | head => exact absurd rfl hne
      | tail _ h' => exact h'
    · simp only [rm, hax, ↓reduceIte]
      cases h with
      | head => exact List.Mem.head _
      | tail _ h' => exact List.Mem.tail _ (mem_rm h' hne)

/-- Lemma 1 (consumption bound). If the consumed identities are distinct and each is a
finalized source deposit, the credited total is at most the source total. The source
list itself may contain repeats; nothing about it is assumed. -/
theorem consumed_sum_le (f : Nat → Nat) :
    ∀ (c src : List Nat), Fresh c → (∀ x, x ∈ c → x ∈ src) → sumBy f c ≤ sumBy f src
  | [], _, _, _ => Nat.zero_le _
  | a :: c, src, hf, hs => by
    have ha : a ∈ src := hs a (List.Mem.head _)
    have htail : ∀ x, x ∈ c → x ∈ rm a src := by
      intro x hx
      have hxa : x ≠ a := by
        intro hxa; subst hxa; exact hf.1 hx
      exact mem_rm (hs x (List.Mem.tail _ hx)) hxa
    have ih := consumed_sum_le f c (rm a src) hf.2 htail
    rw [sumBy_rm f ha]
    simp only [sumBy]
    omega

/-! ## 2. Bridge state and transitions -/

/-- `src` finalized retained source deposits (event ids); `consumed` nullified ids;
`G` unsolicited transfers into the vault; `D` authenticated cumulative deposits credited;
`U` credited but unminted; `O` outstanding TL liabilities (ledger + UTXO-bound + reserved);
`Q` burned, payout pending or in flight; `R` completed payouts; `V` actual vault reserve. -/
structure St where
  src : List Nat
  consumed : List Nat
  G : Nat
  D : Nat
  U : Nat
  O : Nat
  Q : Nat
  R : Nat
  V : Nat

def init : St := ⟨[], [], 0, 0, 0, 0, 0, 0, 0⟩

inductive Op where
  | srcDeposit (e : Nat)     -- source chain finalizes a retained deposit (not adversary-controlled)
  | gift (x : Nat)           -- someone transfers tokens to the vault outside the protocol
  | admit (e : Nat)          -- TL accepts a deposit claim (relay/prover submitted)
  | mint (a : Nat)           -- credited atoms become TL liabilities
  | burn (a : Nat)           -- TL burn creates a pending withdrawal
  | pay (a : Nat)            -- vault pays a pending withdrawal
  | reorg (e : Nat)          -- finalized source deposit disappears from canonical history
  | adminWithdraw (x : Nat)  -- operational key moves vault funds outside the payout relation
  | migrate                  -- verifier / key / checkpoint upgrade

structure Assumptions where
  auth : Bool
  nullify : Bool
  finality : Bool
  custody : Bool
  continuity : Bool

def Secure : Assumptions := ⟨true, true, true, true, true⟩

section
variable (amt : Nat → Nat)

def exec (A : Assumptions) (s : St) : Op → Option St
  | .srcDeposit e => some { s with src := e :: s.src, V := s.V + amt e }
  | .gift x => some { s with G := s.G + x, V := s.V + x }
  | .admit e =>
      if (A.auth = false ∨ e ∈ s.src) ∧ (A.nullify = false ∨ e ∉ s.consumed) then
        some { s with consumed := e :: s.consumed, D := s.D + amt e, U := s.U + amt e }
      else none
  | .mint a => if a ≤ s.U then some { s with U := s.U - a, O := s.O + a } else none
  | .burn a => if a ≤ s.O then some { s with O := s.O - a, Q := s.Q + a } else none
  | .pay a =>
      if a ≤ s.Q ∧ a ≤ s.V then some { s with Q := s.Q - a, R := s.R + a, V := s.V - a }
      else none
  | .reorg e =>
      if A.finality = false ∧ e ∈ s.src ∧ amt e ≤ s.V then
        some { s with src := rm e s.src, V := s.V - amt e }
      else none
  | .adminWithdraw x =>
      if A.custody = false ∧ x ≤ s.V then some { s with V := s.V - x } else none
  | .migrate => some (if A.continuity = true then s else { s with consumed := [] })

def run (A : Assumptions) : St → List Op → Option St
  | s, [] => some s
  | s, o :: os =>
    match exec amt A s o with
    | none => none
    | some s' => run A s' os

/-- The inductive invariant. -/
structure Inv (s : St) : Prop where
  fresh : Fresh s.consumed
  sub : ∀ x, x ∈ s.consumed → x ∈ s.src
  dsum : s.D = sumBy amt s.consumed
  acct : s.U + s.O + s.Q + s.R = s.D
  vault : s.V + s.R = sumBy amt s.src + s.G

theorem init_inv : Inv amt init :=
  ⟨trivial, (fun _ h => by cases h), rfl, rfl, rfl⟩

/-- Lemma 2 (one authenticated transition preserves the invariant). -/
theorem exec_inv {s s' : St} (o : Op) (hs : Inv amt s)
    (h : exec amt Secure s o = some s') : Inv amt s' := by
  cases o with
  | srcDeposit e =>
    simp only [exec] at h
    injection h with h; subst h
    refine ⟨hs.fresh, fun x hx => List.Mem.tail _ (hs.sub x hx), hs.dsum, hs.acct, ?_⟩
    have := hs.vault
    show s.V + amt e + s.R = sumBy amt (e :: s.src) + s.G
    simp only [sumBy]; omega
  | gift x =>
    simp only [exec] at h
    injection h with h; subst h
    refine ⟨hs.fresh, hs.sub, hs.dsum, hs.acct, ?_⟩
    have := hs.vault
    show s.V + x + s.R = sumBy amt s.src + (s.G + x)
    omega
  | admit e =>
    simp only [exec] at h
    split at h
    · rename_i hc
      injection h with h; subst h
      have hc' : e ∈ s.src ∧ e ∉ s.consumed := by simpa [Secure] using hc
      refine ⟨⟨hc'.2, hs.fresh⟩, ?_, ?_, ?_, hs.vault⟩
      · intro x hx
        cases hx with
        | head => exact hc'.1
        | tail _ hx' => exact hs.sub x hx'
      · show s.D + amt e = sumBy amt (e :: s.consumed)
        have := hs.dsum
        simp only [sumBy]; omega
      · show s.U + amt e + s.O + s.Q + s.R = s.D + amt e
        have := hs.acct; omega
    · cases h
  | mint a =>
    simp only [exec] at h
    split at h
    · rename_i hc
      injection h with h; subst h
      refine ⟨hs.fresh, hs.sub, hs.dsum, ?_, hs.vault⟩
      show s.U - a + (s.O + a) + s.Q + s.R = s.D
      have := hs.acct; omega
    · cases h
  | burn a =>
    simp only [exec] at h
    split at h
    · rename_i hc
      injection h with h; subst h
      refine ⟨hs.fresh, hs.sub, hs.dsum, ?_, hs.vault⟩
      show s.U + (s.O - a) + (s.Q + a) + s.R = s.D
      have := hs.acct; omega
    · cases h
  | pay a =>
    simp only [exec] at h
    split at h
    · rename_i hc
      injection h with h; subst h
      refine ⟨hs.fresh, hs.sub, hs.dsum, ?_, ?_⟩
      · show s.U + s.O + (s.Q - a) + (s.R + a) = s.D
        have := hs.acct; omega
      · show s.V - a + (s.R + a) = sumBy amt s.src + s.G
        have := hs.vault; omega
    · cases h
  | reorg e =>
    simp [exec, Secure] at h
  | adminWithdraw x =>
    simp [exec, Secure] at h
  | migrate =>
    simp only [exec, Secure, ↓reduceIte] at h
    injection h with h; subst h
    exact hs

theorem run_inv : ∀ (ops : List Op) (s s' : St),
    Inv amt s → run amt Secure s ops = some s' → Inv amt s'
  | [], s, s', hs, h => by
    simp only [run] at h
    injection h with h; subst h; exact hs
  | o :: os, s, s', hs, h => by
    simp only [run] at h
    split at h
    · cases h
    · rename_i s₁ h₁
      exact run_inv os s₁ s' (exec_inv amt o hs h₁) h

/-- Lemma 3 (authorization bound). Credited deposits never exceed finalized source deposits. -/
theorem credited_le_source (s : St) (hs : Inv amt s) : s.D ≤ sumBy amt s.src := by
  rw [hs.dsum]
  exact consumed_sum_le amt s.consumed s.src hs.fresh hs.sub

/-- Lemma 4 (counter order). R ≤ D: completed payouts never exceed authenticated deposits. -/
theorem paid_le_credited (s : St) (hs : Inv amt s) : s.R ≤ s.D := by
  have := hs.acct; omega

/-- Theorem 1 (reserve conservation). Under A1..A5, every reachable state is solvent: the
actual vault reserve covers unminted credits, outstanding liabilities and pending payouts. -/
theorem secure_solvent (ops : List Op) (s : St)
    (h : run amt Secure init ops = some s) : s.U + s.O + s.Q ≤ s.V := by
  have hs := run_inv amt ops init s (init_inv amt) h
  have h1 := credited_le_source amt s hs
  have h2 := hs.acct
  have h3 := hs.vault
  omega

/-- Corollary: the surplus `V - (U+O+Q)` is exactly unsolicited transfers plus finalized
deposits that were never credited (unclaimed or censored). It is never negative. -/
theorem surplus_exact (ops : List Op) (s : St)
    (h : run amt Secure init ops = some s) :
    s.V + s.D = s.U + s.O + s.Q + sumBy amt s.src + s.G := by
  have hs := run_inv amt ops init s (init_inv amt) h
  have h2 := hs.acct
  have h3 := hs.vault
  omega

end

/-! ## 3. Exactly-once payout (vault side) -/

inductive PS where
  | pending
  | inflight
  | paid
  deriving DecidableEq

/-- One registered exit. `sent` is ground truth: how many external transfers actually executed.
`xfer`/`ok` record whether the current attempt's transfer ran and whether it succeeded. -/
structure Exit where
  status : PS
  attempt : Nat
  xfer : Bool
  ok : Bool
  sent : Nat

def exit0 : Exit := ⟨.pending, 0, false, false, 0⟩

inductive POp where
  | begin
  | transfer (success : Bool)
  | callback (attempt : Nat) (success : Bool)
  | timeoutRetry
  | forgedFailure (attempt : Nat)

/-- P1 `authCallback`: callbacks carry the true promise result of the exact attempt.
P2 `noTimeout`: no retry of an in-flight attempt without an authenticated failure. -/
structure PayAssumptions where
  authCallback : Bool
  noTimeout : Bool

def PSecure : PayAssumptions := ⟨true, true⟩

def pexec (P : PayAssumptions) (x : Exit) : POp → Option Exit
  | .begin =>
      if x.status = .pending then
        some { x with status := .inflight, attempt := x.attempt + 1, xfer := false, ok := false }
      else none
  | .transfer b =>
      if x.status = .inflight ∧ x.xfer = false then
        some { x with xfer := true, ok := b, sent := if b then x.sent + 1 else x.sent }
      else none
  | .callback a b =>
      if x.status = .inflight ∧ a = x.attempt ∧ x.xfer = true ∧ b = x.ok then
        some (if b then { x with status := .paid } else { x with status := .pending, xfer := false })
      else none
  | .timeoutRetry =>
      if P.noTimeout = false ∧ x.status = .inflight then
        some { x with status := .pending, xfer := false }
      else none
  | .forgedFailure a =>
      if P.authCallback = false ∧ x.status = .inflight ∧ a = x.attempt then
        some { x with status := .pending, xfer := false }
      else none

def prun (P : PayAssumptions) : Exit → List POp → Option Exit
  | x, [] => some x
  | x, o :: os =>
    match pexec P x o with
    | none => none
    | some x' => prun P x' os

structure PInv (x : Exit) : Prop where
  pend : x.status = .pending → x.sent = 0 ∧ x.xfer = false
  paid : x.status = .paid → x.sent = 1
  infl : x.status = .inflight → x.sent = (if x.xfer = true ∧ x.ok = true then 1 else 0)

theorem pinv_init : PInv exit0 :=
  ⟨fun _ => ⟨rfl, rfl⟩, (fun h => by simp [exit0] at h), (fun h => by simp [exit0] at h)⟩

theorem pexec_inv {x x' : Exit} (o : POp) (hx : PInv x)
    (h : pexec PSecure x o = some x') : PInv x' := by
  cases o with
  | begin =>
    simp only [pexec] at h
    split at h
    · rename_i hc
      injection h with h; subst h
      have := hx.pend hc
      exact ⟨(fun h => by simp at h), (fun h => by simp at h), (fun _ => by simp [this.1])⟩
    · cases h
  | transfer b =>
    simp only [pexec] at h
    split at h
    · rename_i hc
      injection h with h; subst h
      have hs := hx.infl hc.1
      have hxf : x.xfer = false := hc.2
      simp [hxf] at hs
      refine ⟨(fun h => by simp [hc.1] at h), (fun h => by simp [hc.1] at h), (fun _ => ?_)⟩
      cases b <;> simp [hs]
    · cases h
  | callback a b =>
    simp only [pexec] at h
    split at h
    · rename_i hc
      obtain ⟨hi, _, hxf, hb⟩ := hc
      have hs := hx.infl hi
      cases b with
      | true =>
        simp only [↓reduceIte] at h
        injection h with h; subst h
        refine ⟨(fun h => by simp at h), (fun _ => ?_), (fun h => by simp at h)⟩
        simp [hxf, ← hb] at hs
        exact hs
      | false =>
        simp only [Bool.false_eq_true, ite_false] at h
        injection h with h; subst h
        refine ⟨(fun _ => ⟨?_, rfl⟩), (fun h => by simp at h), (fun h => by simp at h)⟩
        simp [hxf, ← hb] at hs
        exact hs
    · cases h
  | timeoutRetry =>
    simp [pexec, PSecure] at h
  | forgedFailure a =>
    simp [pexec, PSecure] at h

theorem prun_inv : ∀ (ops : List POp) (x x' : Exit),
    PInv x → prun PSecure x ops = some x' → PInv x'
  | [], x, x', hx, h => by
    simp only [prun] at h
    injection h with h; subst h; exact hx
  | o :: os, x, x', hx, h => by
    simp only [prun] at h
    split at h
    · cases h
    · rename_i x₁ h₁
      exact prun_inv os x₁ x' (pexec_inv o hx h₁) h

/-- Theorem 2 (exactly-once payout). Under P1 and P2 an exit is externally paid at most once,
across any interleaving of attempts, failures, stale or replayed callbacks. -/
theorem at_most_once (ops : List POp) (x : Exit)
    (h : prun PSecure exit0 ops = some x) : x.sent ≤ 1 := by
  have hx := prun_inv ops exit0 x pinv_init h
  cases hst : x.status with
  | pending => have := (hx.pend hst).1; omega
  | paid => have := hx.paid hst; omega
  | inflight =>
    have := hx.infl hst
    split at this <;> omega

/-- Paid is recorded only after a real transfer. -/
theorem paid_implies_sent (ops : List POp) (x : Exit)
    (h : prun PSecure exit0 ops = some x) (hp : x.status = .paid) : x.sent = 1 :=
  (prun_inv ops exit0 x pinv_init h).paid hp

/-! ## 4. Counterexamples: drop one assumption, get insolvency or a double payout -/

/-- Fixture amounts calibrated to BRIDGE-SEC-01: honest event 0 retains 5,213,905 units;
event 331 is a phantom (never in `src`) claiming 5,213,000. Event 7 retains 100. -/
def amtC (e : Nat) : Nat :=
  if e = 0 then 5213905 else if e = 331 then 5213000 else if e = 7 then 100 else 0

def honestSetup : List Op := [.srcDeposit 0, .admit 0, .mint 5213905]

/-- Insolvency predicate evaluated on a run result. -/
def insolvent : Option St → Bool
  | some s => decide (s.V < s.U + s.O + s.Q)
  | none => false

/-- C1: drop A1 (relay-signed claim admitted without source proof). Reproduces the
BRIDGE-SEC-01 accounting: reserve 905, honest liabilities 5,213,905. -/
def phantomDrain : List Op :=
  honestSetup ++ [.admit 331, .mint 5213000, .burn 5213000, .pay 5213000]

theorem c1_no_auth_insolvent :
    insolvent (run amtC ⟨false, true, true, true, true⟩ init phantomDrain) = true := by decide

theorem c1_secure_rejects : run amtC Secure init phantomDrain = none := by decide

theorem c1_reserve_905 :
    (run amtC ⟨false, true, true, true, true⟩ init phantomDrain).map St.V = some 905 := by decide

/-- C2: drop A2 (same event credited twice, e.g. a second receipt in the same graph). -/
def doubleCredit : List Op := [.srcDeposit 7, .admit 7, .admit 7, .mint 200]

theorem c2_no_nullifier_insolvent :
    insolvent (run amtC ⟨true, false, true, true, true⟩ init doubleCredit) = true := by decide

theorem c2_secure_rejects : run amtC Secure init doubleCredit = none := by decide

/-- C3: drop A3 (finalized deposit reverts after credit). -/
def revertedCredit : List Op := [.srcDeposit 7, .admit 7, .mint 100, .reorg 7]

theorem c3_no_finality_insolvent :
    insolvent (run amtC ⟨true, true, false, true, true⟩ init revertedCredit) = true := by decide

theorem c3_secure_rejects : run amtC Secure init revertedCredit = none := by decide

/-- C4: drop A4 (operational key with a transfer path out of the vault). -/
def keyDrain : List Op := honestSetup ++ [.adminWithdraw 5213000]

theorem c4_no_custody_insolvent :
    insolvent (run amtC ⟨true, true, true, false, true⟩ init keyDrain) = true := by decide

theorem c4_secure_rejects : run amtC Secure init keyDrain = none := by decide

/-- C5: drop A5 (upgrade resets the consumed set; the same event is replayed in the new epoch). -/
def epochReplay : List Op := [.srcDeposit 7, .admit 7, .migrate, .admit 7, .mint 200]

theorem c5_no_continuity_insolvent :
    insolvent (run amtC ⟨true, true, true, true, false⟩ init epochReplay) = true := by decide

theorem c5_secure_rejects : run amtC Secure init epochReplay = none := by decide

/-- P2 dropped: timeout-based retry of an ambiguous in-flight attempt pays twice. -/
def timeoutDouble : List POp :=
  [.begin, .transfer true, .timeoutRetry, .begin, .transfer true]

theorem p2_timeout_double_pay :
    (prun ⟨true, false⟩ exit0 timeoutDouble).map Exit.sent = some 2 := by decide

theorem p2_secure_rejects : prun PSecure exit0 timeoutDouble = none := by decide

/-- P1 dropped: a forged failure callback after a successful transfer pays twice. -/
def forgedDouble : List POp :=
  [.begin, .transfer true, .forgedFailure 1, .begin, .transfer true]

theorem p1_forged_double_pay :
    (prun ⟨false, true⟩ exit0 forgedDouble).map Exit.sent = some 2 := by decide

theorem p1_secure_rejects : prun PSecure exit0 forgedDouble = none := by decide

/-- Stale callback of attempt 1 is refused after attempt 2 begins (secure model). -/
theorem stale_callback_rejected :
    prun PSecure exit0 [.begin, .transfer false, .callback 1 false, .begin, .callback 1 true]
      = none := by decide

/-! ## 5. Containment arithmetic (used when A1 fails undetected) -/

def sumL : List Nat → Nat
  | [] => 0
  | a :: l => a + sumL l

/-- Theorem 3 (rate-limit containment). If every admission window credits at most `cap`,
then over `k = l.length` windows before detection at most `k * cap` can be credited,
whatever the adversary controls. This bounds loss; it does not prevent it. -/
theorem window_loss_bound (cap : Nat) :
    ∀ l : List Nat, (∀ x, x ∈ l → x ≤ cap) → sumL l ≤ l.length * cap
  | [], _ => Nat.zero_le _
  | a :: l, h => by
    have ha := h a (List.Mem.head _)
    have ih := window_loss_bound cap l (fun x hx => h x (List.Mem.tail _ hx))
    simp only [sumL, List.length_cons, Nat.succ_mul]
    omega

end BridgeSecurity
