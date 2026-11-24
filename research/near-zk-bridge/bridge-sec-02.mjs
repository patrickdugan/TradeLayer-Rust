// BRIDGE-SEC-02: compromised-authority benchmark for the ZTB bridge.
// Isolated synthetic fixtures only: no network, no live chain, no real keys or funds.
//
// What is real here: Ed25519 signatures (node:crypto), SHA-256, double-SHA-256 proof of work,
// Merkle folds, and the unchanged ZTB-01 accounting model / ZTB-02 vault mock.
// What is NOT real: the source chain is a NEAR-*like* light-client profile ("NLC"), not NEAR's
// header/approval format; the Nova proof is replaced by native evaluation of the V2 relation
// (sound only under knowledge soundness of a real circuit for exactly that relation); TL state
// validity on the reverse path is still a trusted fixture (TLSTATE_FIXTURE), flagged in output.
import { createHash, createPrivateKey, createPublicKey, sign, verify } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import assert from "node:assert/strict";
import { makeModel } from "./model.mjs";
import { codecFactory } from "./codec.mjs";
import { vaultFactory } from "./vault_mock.mjs";

// ---------- encoding ----------
const sha = (...b) => createHash("sha256").update(Buffer.concat(b)).digest();
const s = x => { const a = Buffer.from(x, "ascii"); const n = Buffer.alloc(4); n.writeUInt32LE(a.length); return Buffer.concat([n, a]); };
const n = x => { const a = Buffer.alloc(8); a.writeBigUInt64LE(BigInt(x)); return a; };
const d = x => { if (!/^[0-9a-f]{64}$/.test(x)) throw Error("digest"); return Buffer.from(x, "hex"); };
const T = (tag, ...p) => sha(Buffer.from(tag, "ascii"), Buffer.of(0), ...p).toString("hex");
const req = (x, m) => { if (!x) throw Error(m); };

// ---------- Ed25519 from deterministic fixture seeds ----------
const PKCS8 = Buffer.from("302e020100300506032b657004220420", "hex");
const SPKI = Buffer.from("302a300506032b6570032100", "hex");
function key(label) {
  const priv = createPrivateKey({ key: Buffer.concat([PKCS8, d(T("BSEC02/SEED", s(label)))]), format: "der", type: "pkcs8" });
  const pk = createPublicKey(priv).export({ format: "der", type: "spki" }).subarray(12).toString("hex");
  return { label, priv, pk };
}
const sig = (k, m) => sign(null, m, k.priv).toString("hex");
const sigOk = (pk, m, x) => { try { return verify(null, m, createPublicKey({ key: Buffer.concat([SPKI, Buffer.from(pk, "hex")]), format: "der", type: "spki" }), Buffer.from(x, "hex")); } catch { return false; } };

// ---------- Merkle (odd node promoted, index-derived directions) ----------
const node = (a, b) => T("NLC/NODE/V1", d(a), d(b));
function mroot(leaves) { let l = [...leaves]; if (!l.length) return T("NLC/EMPTY/V1"); while (l.length > 1) { const x = []; for (let i = 0; i < l.length; i += 2) x.push(i + 1 < l.length ? node(l[i], l[i + 1]) : l[i]); l = x; } return l[0]; }
function mpath(leaves, i) { let l = [...leaves], out = []; while (l.length > 1) { const sib = i ^ 1; if (sib < l.length) out.push(l[sib]); const x = []; for (let j = 0; j < l.length; j += 2) x.push(j + 1 < l.length ? node(l[j], l[j + 1]) : l[j]); l = x; i >>= 1; } return out; }
function mfold(leaf, i, total, path) {
  req(Number.isSafeInteger(i) && Number.isSafeInteger(total) && i >= 0 && i < total && total <= 4096, "merkle index");
  let r = leaf, used = 0, w = total;
  while (w > 1) { const sib = i ^ 1; if (sib < w) { req(used < path.length, "missing sibling"); r = i & 1 ? node(path[used++], r) : node(r, path[used++]); } i >>= 1; w = Math.ceil(w / 2); }
  req(used === path.length, "extra sibling"); return r;
}

// ---------- NLC source chain: stake-weighted finality, epoch hand-off ----------
const CHAIN = "nlc-fixture-1";
const bpHash = set => T("NLC/BP/V1", n(set.length), ...set.flatMap(v => [d(v.pk), n(v.stake)]));
const headerHash = h => T("NLC/HEADER/V1", n(h.height), n(h.epoch), d(h.prev), d(h.outcomeRoot), d(h.nextBpHash));
const approvalMsg = (chain, hash) => Buffer.from(T("NLC/APPROVAL/V1", s(chain), d(hash)), "hex");
class LightClient {
  // Pinned at construction: chain id, genesis hash, epoch-0 producer set. No setter exists:
  // relayers, admins and RPCs can only offer headers, never assert a head.
  constructor(genesis) {
    this.chain = genesis.chain; this.epochs = new Map([[0, genesis.bp]]); this.nextBp = new Map();
    this.headers = new Map([[genesis.hash, { height: 0, epoch: 0, outcomeRoot: T("NLC/EMPTY/V1"), hash: genesis.hash }]]);
    this.byHeight = new Map([[0, genesis.hash]]); this.head = this.headers.get(genesis.hash); this.frozen = false; this.evidence = [];
  }
  accept(h, approvals, revealNext) {
    req(!this.frozen, "light client frozen on equivocation");
    const hash = headerHash(h); req(hash === h.hash, "header hash");
    req(h.height > 0 && (h.epoch === this.head.epoch || h.epoch === this.head.epoch + 1), "epoch continuity");
    const set = this.epochs.get(h.epoch); req(set, "unknown producer set");
    const known = this.nextBp.get(h.epoch); req(!known || known === h.nextBpHash, "next-set commitment changed within epoch");
    const total = set.reduce((a, v) => a + v.stake, 0n), seen = new Set(); let signed = 0n;
    for (const a of approvals) {
      const v = set.find(x => x.pk === a.pk); if (!v || seen.has(a.pk)) continue;
      if (sigOk(a.pk, approvalMsg(this.chain, hash), a.sig)) { seen.add(a.pk); signed += v.stake; }
    }
    req(signed * 3n > total * 2n, "insufficient finality stake");
    const prior = this.byHeight.get(h.height);
    if (prior && prior !== hash) { this.frozen = true; this.evidence.push({ height: h.height, a: prior, b: hash }); throw Error("conflicting finalized checkpoints"); }
    req(h.height > this.head.height, "height regression");
    this.nextBp.set(h.epoch, h.nextBpHash);
    if (revealNext) { req(bpHash(revealNext) === h.nextBpHash, "next producer set reveal"); this.epochs.set(h.epoch + 1, revealNext); }
    this.headers.set(hash, h); this.byHeight.set(h.height, hash); this.head = h; return hash;
  }
  header(hash) { return this.headers.get(hash); }
}

// ---------- NEAR-like outcomes for the ft_transfer_call graph ----------
const outcomeLeaf = o => T("NLC/OUTCOME/V1", d(o.id), s(o.executor), s(o.status.kind), s(o.status.value), n(o.children.length), ...o.children.map(d), n(o.logs.length), ...o.logs.map(l => d(T("NLC/LOG/V1", s(l)))));
const ftLog = (from, to, amount, memo) => "EVENT_JSON:" + JSON.stringify({ standard: "nep141", version: "1.0.0", event: "ft_transfer", data: [{ old_owner_id: from, new_owner_id: to, amount: String(amount), ...(memo ? { memo } : {}) }] });

// ---------- domain ----------
const hx = k => T("BSEC02/FIXTURE", s(k));
const codec = codecFactory(b => createHash("sha256").update(b).digest());
const POLICY = { nearGenesis: hx("near-genesis"), bitcoinGenesis: hx("btc-genesis"), activation: hx("activation"), policy: hx("policy"), token: "usdc.fixture", vault: "bridge.fixture", property: 77n };
POLICY.domain = codec.domain(POLICY);
const DOMAIN = POLICY.domain;
const depositIdOf = rid => T("TLBRIDGE/DEPOSIT-ID/V1", d(DOMAIN), d(rid), Buffer.alloc(4));
const candidateOf = seq => T("TLBRIDGE/CANDIDATE-ID/V1", d(DOMAIN), n(seq));

// ---------- the V2 deposit relation (what the Nova circuit must enforce; see NOVA_STATEMENT_V2.md) ----------
// Pure function of (statement, witness). Throws on violation. The verifier separately checks that
// statement.headerHash/outcomeRoot belong to a header the light client finalized.
function NEARPAY_V2(st, w) {
  const keys = ["program", "domain", "headerHash", "outcomeRoot", "depositId", "candidate", "amount", "recipient", "sender", "token", "vault"];
  req(Object.keys(st).length === keys.length && keys.every(k => Object.hasOwn(st, k)), "statement schema");
  req(st.domain === DOMAIN && st.token === POLICY.token && st.vault === POLICY.vault, "domain/token/vault binding");
  const roles = ["transfer", "receiver", "resolver"];
  for (const r of roles) { const x = w[r]; req(x && mfold(outcomeLeaf(x.outcome), x.index, x.total, x.path) === st.outcomeRoot, "outcome membership: " + r); }
  const t = w.transfer.outcome, v = w.receiver.outcome, r = w.resolver.outcome;
  req(new Set([t.id, v.id, r.id]).size === 3, "distinct receipts");
  req(t.executor === POLICY.token && v.executor === POLICY.vault && r.executor === POLICY.token, "executor");
  req(t.status.kind === "receipt" && t.status.value === r.id && t.children.includes(v.id) && t.children.includes(r.id), "promise graph");
  req(v.status.kind === "value" && r.status.kind === "value", "terminal success");
  const a = w.args; req(a && /^[0-9]+$/.test(a.requested) && /^[0-9]+$/.test(a.seq), "argument ABI");
  const requested = BigInt(a.requested), unused = BigInt(v.status.value), used = BigInt(r.status.value);
  req(requested > 0n && unused <= requested && used === requested - unused, "terminal amount");
  req(t.logs.length === 1 && t.logs[0] === ftLog(a.sender, POLICY.vault, requested), "transfer log");
  req(v.logs.length === 1 && v.logs[0] === `ZTB_DEPOSIT_V1:${DOMAIN}:${a.seq}:${a.sender}:${a.script}:${a.requested}:${unused}`, "candidate log");
  req(unused === 0n ? r.logs.length === 0 : r.logs.length === 1 && r.logs[0] === ftLog(POLICY.vault, a.sender, unused, "refund"), "refund log");
  req(used > 0n, "nothing retained");
  req(st.depositId === depositIdOf(r.id) && st.candidate === candidateOf(BigInt(a.seq)), "event identity");
  req(st.amount === used && st.recipient === a.script && st.sender === a.sender, "economic binding");
  return true;
}
const PERMISSIVE = () => true; // what a compromised admin would like to register
// Program identity = hash of the relation's source text (line endings normalized), standing in
// for a verifying key derived reproducibly from reviewed circuit source.
const programId = f => T("BSEC02/PROGRAM/V1", s(f.toString().replace(/\r\n/g, "\n")));
const PROGRAM = programId(NEARPAY_V2);

// ---------- the fixture world ----------
function world({ cap = 1n << 62n } = {}) {
  const V = Array.from({ length: 6 }, (_, i) => key("validator-" + i));
  const set0 = V.slice(0, 4).map(v => ({ pk: v.pk, stake: 25n }));
  const set1 = V.slice(2, 6).map(v => ({ pk: v.pk, stake: 25n }));
  const genesis = { chain: CHAIN, bp: set0, hash: POLICY.nearGenesis };
  const lc = new LightClient(genesis);
  const src = { height: 0, epoch: 0, prev: genesis.hash, sets: [set0, set1], keys: V, seq: 0n };
  const vault = vaultFactory(codec, POLICY);
  // Produce a block of outcomes, sign it with `signers` (default: the epoch's honest producers).
  function block(outcomes, { signers, epoch = src.epoch, height = src.height + 1, prev = src.prev, reveal = false } = {}) {
    const set = src.sets[epoch], nextSet = src.sets[epoch + 1] ?? set;
    const leaves = outcomes.map(outcomeLeaf);
    const h = { height, epoch, prev, outcomeRoot: mroot(leaves), nextBpHash: bpHash(nextSet) };
    h.hash = headerHash(h);
    const who = signers ?? V.filter(k => set.some(x => x.pk === k.pk));
    const approvals = who.map(k => ({ pk: k.pk, sig: sig(k, approvalMsg(CHAIN, h.hash)) }));
    return { h, approvals, reveal: reveal ? nextSet : undefined, outcomes, leaves };
  }
  function publish(b) { const hash = lc.accept(b.h, b.approvals, b.reveal); src.height = b.h.height; src.prev = hash; src.epoch = b.h.epoch; return hash; }
  // A genuine ft_transfer_call; the vault mock records physical holdings exactly as retained.
  function deposit({ sender = "alice.fixture", script = hx("alice-script"), requested, unused = 0n, publishBlock = true, signers }) {
    const seq = ++src.seq, id = r => T("NLC/RECEIPT/V1", s(r), n(seq), n(src.height));
    const tid = id("t"), vid = id("v"), rid = id("r"), used = requested - unused;
    const t = { id: tid, executor: POLICY.token, status: { kind: "receipt", value: rid }, children: [vid, rid], logs: [ftLog(sender, POLICY.vault, requested)] };
    const v = { id: vid, executor: POLICY.vault, status: { kind: "value", value: String(unused) }, children: [], logs: [`ZTB_DEPOSIT_V1:${DOMAIN}:${seq}:${sender}:${script}:${requested}:${unused}`] };
    const r = { id: rid, executor: POLICY.token, status: { kind: "value", value: String(used) }, children: [], logs: unused ? [ftLog(POLICY.vault, sender, unused, "refund")] : [] };
    const noise = { id: id("noise"), executor: "other.fixture", status: { kind: "value", value: "0" }, children: [], logs: [] };
    const b = block([noise, t, v, r], { signers });
    if (publishBlock) {
      publish(b);
      const pid = vault.receive({ predecessor: POLICY.token, receipt: rid, event: 0n, sender, recipientScriptHash: script, requested });
      vault.finalizeDeposit(vault.issueResolution(pid, used));
    }
    const wit = { transfer: { outcome: t, index: 1, total: 4, path: mpath(b.leaves, 1) }, receiver: { outcome: v, index: 2, total: 4, path: mpath(b.leaves, 2) }, resolver: { outcome: r, index: 3, total: 4, path: mpath(b.leaves, 3) }, args: { sender, script, requested: String(requested), seq: String(seq) } };
    const st = { program: PROGRAM, domain: DOMAIN, headerHash: b.h.hash, outcomeRoot: b.h.outcomeRoot, depositId: depositIdOf(rid), candidate: candidateOf(seq), amount: used, recipient: script, sender, token: POLICY.token, vault: POLICY.vault };
    return { proof: { program: PROGRAM, st, wit }, block: b };
  }
  const tl = bridge({ lc, cap });
  return { V, set0, set1, lc, src, vault, block, publish, deposit, tl };
}

// ---------- TL-side admission: permissionless, no relayer or admin authority ----------
function bridge({ lc, cap }) {
  const { fresh, apply, invariant } = makeModel();
  const registry = new Map([[PROGRAM, NEARPAY_V2]]);
  const consumed = new Set(); // persistent across relayer, key, verifier and checkpoint epochs
  const st = { s: fresh(), paused: false, quarantined: false, program: PROGRAM, lc, window: 0n, cap, verifierEpoch: 1, pendingUpgrade: null, now: 0 };
  function admit(proof, relayer = "anyone") { // relayer identity is ignored on purpose
    req(!st.paused && !st.quarantined, "admission closed");
    req(proof.program === st.program, "program not pinned by domain");
    const rel = registry.get(proof.program); req(rel, "unknown program");
    const h = st.lc.header(proof.st.headerHash);
    req(h && h.outcomeRoot === proof.st.outcomeRoot, "head not finalized by light client");
    rel(proof.st, proof.wit);
    req(!consumed.has(proof.st.depositId) && !consumed.has(proof.st.candidate), "event already consumed");
    req(st.window + proof.st.amount <= st.cap, "window cap");
    let next = apply(st.s, { kind: "deposit", id: proof.st.depositId, owner: proof.st.recipient, amount: proof.st.amount });
    next = apply(next, { kind: "mint", id: proof.st.depositId });
    consumed.add(proof.st.depositId); consumed.add(proof.st.candidate); st.window += proof.st.amount; st.s = next;
    return proof.st.depositId;
  }
  const nullifierRoot = () => T("BSEC02/NULLIFIERS/V1", ...[...consumed].sort().map(d));
  // Upgrade boundary: an admin may PROPOSE; acceptance needs delay + continuity commitments.
  function propose(cert) { st.pendingUpgrade = { ...cert, at: st.now }; }
  function activate(DELAY = 10) {
    const u = st.pendingUpgrade; req(u, "no upgrade");
    req(st.now - u.at >= DELAY, "upgrade delay");
    req(u.nullifierRoot === nullifierRoot() && u.consumedCount === consumed.size, "nullifier continuity");
    req(u.lcHeadHash === st.lc.head.hash, "checkpoint continuity");
    req(u.statementVersionBumped === true && u.relationSourceReviewed === true, "relation identity");
    registry.set(u.program, u.relation); st.program = u.program; st.verifierEpoch++; st.pendingUpgrade = null;
  }
  return { st, admit, apply: e => { st.s = apply(st.s, e); }, invariant: () => invariant(st.s), nullifierRoot, propose, activate, registry, consumed, tick: () => { st.window = 0n; st.now++; } };
}

// ---------- Bitcoin-like anchor chain (real SHA-256d PoW, fixed easy target) ----------
const dsha = (...b) => createHash("sha256").update(createHash("sha256").update(Buffer.concat(b)).digest()).digest();
function mine(prev, height, packets, salt = "") {
  const merkle = mroot(packets.map(p => T("BTC/PACKET/V1", s(p))));
  for (let nonce = 0; ; nonce++) { const h = dsha(d(prev), d(merkle), n(nonce), s(salt)); if (h[0] === 0) return { hash: h.toString("hex"), prev, height, merkle, nonce, salt, packets }; }
}
class BtcLC {
  constructor(checkpoint) { this.blocks = new Map([[checkpoint.hash, { ...checkpoint, work: 0n }]]); this.tip = checkpoint.hash; }
  accept(b) {
    const p = this.blocks.get(b.prev); req(p, "unknown parent"); req(b.height === p.height + 1, "height");
    req(b.merkle === mroot(b.packets.map(x => T("BTC/PACKET/V1", s(x)))), "merkle");
    const h = dsha(d(b.prev), d(b.merkle), n(b.nonce), s(b.salt)); req(h.toString("hex") === b.hash && h[0] === 0, "proof of work");
    this.blocks.set(b.hash, { ...b, work: p.work + 256n }); if (this.blocks.get(b.hash).work > this.blocks.get(this.tip).work) this.tip = b.hash;
  }
  confirmations(hash) { const b = this.blocks.get(hash); if (!b) return 0; for (let x = this.blocks.get(this.tip); x; x = this.blocks.get(x.prev)) if (x.hash === hash) return this.blocks.get(this.tip).height - b.height + 1; return 0; }
}
const exitLeaf = x => T("TLBRIDGE/EXIT-LEAF/V1", d(DOMAIN), s(x.id), s(x.destination), n(x.amount));
const tlDigest = t => T("TLBRIDGE/TLSTMT/V2", d(DOMAIN), d(t.prevRoot), d(t.nextRoot), d(t.exitRoot), n(t.exitCount));
const packet = t => "TLBRDG02" + tlDigest(t);

// ---------- harness ----------
function run() {
  let checks = 0; const tally = {};
  const ok = (x, label, cat = "control") => { if (!x) throw Error("FAILED: " + label); checks++; tally[cat] = (tally[cat] ?? 0) + 1; };
  const rejects = (f, pattern, label, cat) => { let m = null; try { f(); } catch (e) { m = e.message; } ok(m !== null && m.includes(pattern), `${label} (got: ${m})`, cat); };
  const counterexamples = [];
  const W = world();
  const solvent = (w = W) => { const i = w.tl.invariant(), v = w.vault.state(); return v.holdings - v.reserved >= i.outstanding + (i.pending - v.reserved) + i.unminted; };

  // ===== benign controls =====
  const g1 = W.deposit({ requested: 10_000_000n });
  W.tl.admit(g1.proof, "relayer-epoch-A"); ok(W.tl.invariant().outstanding === 10_000_000n, "genuine full-keep deposit minted", "benign");
  const g2 = W.deposit({ requested: 10_000_000n, unused: 2_000_000n, script: hx("bob-script"), sender: "bob.fixture" });
  W.tl.admit(g2.proof, "relayer-epoch-B"); ok(W.tl.invariant().outstanding === 18_000_000n, "partial refund credits only retained atoms", "benign");
  ok(solvent(), "solvent after genuine deposits", "benign");

  // ===== compromised authorities (all operational keys held by the adversary) =====
  const relayer = key("relayer-epoch-A"), relayerB = key("relayer-epoch-B"), admin = key("admin");
  // A1: relayer-signed phantom (BRIDGE-SEC-01 path). The relayer signature has no verifier.
  const phantom = W.deposit({ requested: 5_213_000n, sender: "mallory.fixture", script: hx("mallory"), publishBlock: false });
  phantom.relaySig = sig(relayer, Buffer.from(phantom.proof.st.depositId, "hex"));
  rejects(() => W.tl.admit(phantom.proof, "relayer-epoch-A"), "head not finalized", "A1 valid relayer, absent source event", "authority");
  rejects(() => W.tl.admit(phantom.proof, "relayer-epoch-B"), "head not finalized", "A1b rotated relayer, absent source event", "authority");
  // A2: admin/RPC offers a header signed by keys outside the pinned producer set.
  const fake = W.block(phantom.block.outcomes, { signers: [admin, relayer, relayerB, key("rpc")] });
  rejects(() => W.lc.accept(fake.h, fake.approvals), "insufficient finality stake", "A2 admin+relayer+RPC signed header", "authority");
  // A3: RPC serves a real-producer header with only 50% stake.
  const half = W.block(phantom.block.outcomes, { signers: W.V.slice(0, 2) });
  rejects(() => W.lc.accept(half.h, half.approvals), "insufficient finality stake", "A3 sub-quorum header", "authority");
  rejects(() => W.lc.accept(half.h, [...half.approvals, ...half.approvals]), "insufficient finality stake", "A3b duplicated approvals", "authority");
  // A4: admin registers a permissive program and submits under it.
  W.tl.registry.set(programId(PERMISSIVE), PERMISSIVE);
  rejects(() => W.tl.admit({ ...phantom.proof, program: programId(PERMISSIVE) }), "program not pinned by domain", "A4 admin-registered permissive program", "authority");
  // A5: admin tries the upgrade path with a reset nullifier set, a fake checkpoint, or no delay.
  const cert = { program: programId(PERMISSIVE), relation: PERMISSIVE, nullifierRoot: W.tl.nullifierRoot(), consumedCount: W.tl.consumed.size, lcHeadHash: W.lc.head.hash, statementVersionBumped: true, relationSourceReviewed: true };
  W.tl.propose({ ...cert, nullifierRoot: T("BSEC02/NULLIFIERS/V1"), consumedCount: 0 }); for (let i = 0; i < 10; i++) W.tl.tick();
  rejects(() => W.tl.activate(), "nullifier continuity", "A5 upgrade resets consumed set", "upgrade");
  W.tl.propose({ ...cert, lcHeadHash: fake.h.hash }); for (let i = 0; i < 10; i++) W.tl.tick();
  rejects(() => W.tl.activate(), "checkpoint continuity", "A5b upgrade swaps checkpoint", "upgrade");
  W.tl.propose(cert);
  rejects(() => W.tl.activate(), "upgrade delay", "A5c upgrade before delay", "upgrade");
  W.tl.st.pendingUpgrade = null;
  // A6: malicious prover with a genuine finalized event but substituted public fields.
  const g3 = W.deposit({ requested: 3_000_000n, sender: "carol.fixture", script: hx("carol") });
  for (const [f, v, pat] of [["recipient", hx("mallory"), "economic binding"], ["amount", 3_000_001n, "economic binding"], ["depositId", hx("other"), "event identity"],
    ["candidate", hx("other"), "event identity"], ["domain", hx("dom"), "domain/token/vault binding"], ["token", "usdt.fixture", "domain/token/vault binding"],
    ["vault", "evil.fixture", "domain/token/vault binding"], ["outcomeRoot", g1.block.h.outcomeRoot, "head not finalized"], ["headerHash", g1.block.h.hash, "head not finalized"]])
    rejects(() => W.tl.admit({ ...g3.proof, st: { ...g3.proof.st, [f]: v } }), pat, "A6 binding substitution: " + f, "binding");
  rejects(() => W.tl.admit({ ...g3.proof, st: { ...g3.proof.st, extra: 1 } }), "statement schema", "A6 extra statement field", "binding");
  // witness-level forgeries
  const cloneW = () => JSON.parse(JSON.stringify(g3.proof.wit));
  let w = cloneW(); w.args.script = hx("mallory");
  rejects(() => W.tl.admit({ ...g3.proof, wit: w }), "candidate log", "A6 witness recipient swap", "binding");
  w = cloneW(); w.resolver.path = [...w.resolver.path, hx("x")];
  rejects(() => W.tl.admit({ ...g3.proof, wit: w }), "extra sibling", "A6 padded Merkle path", "binding");
  w = cloneW(); w.receiver = w.transfer;
  rejects(() => W.tl.admit({ ...g3.proof, wit: w }), "distinct receipts", "A6 receipt role substitution", "binding");
  W.tl.admit(g3.proof); ok(W.tl.invariant().outstanding === 21_000_000n, "unmodified carol proof still admits", "benign");

  // ===== incomplete or non-terminal execution evidence =====
  const mk = o => W.deposit(o);
  const g4 = mk({ requested: 4_000_000n, sender: "dave.fixture", script: hx("dave") });
  w = JSON.parse(JSON.stringify(g4.proof.wit)); w.resolver.outcome.status = { kind: "failure", value: "" };
  rejects(() => W.tl.admit({ ...g4.proof, wit: w }), "outcome membership", "E1 resolver failure substituted (leaf mismatch)", "execution");
  w = JSON.parse(JSON.stringify(g4.proof.wit)); delete w.resolver;
  rejects(() => W.tl.admit({ ...g4.proof, wit: w }), "outcome membership: resolver", "E2 receiver success without resolver", "execution");
  const g5 = mk({ requested: 5_000_000n, unused: 5_000_000n, sender: "erin.fixture", script: hx("erin") });
  rejects(() => W.tl.admit(g5.proof), "nothing retained", "E3 full refund grants nothing", "execution");
  // Partial refund: claiming the requested amount instead of the retained amount.
  const g6 = mk({ requested: 6_000_000n, unused: 1_000_000n, sender: "fay.fixture", script: hx("fay") });
  rejects(() => W.tl.admit({ ...g6.proof, st: { ...g6.proof.st, amount: 6_000_000n } }), "economic binding", "E4 claim requested instead of retained", "execution");
  w = JSON.parse(JSON.stringify(g6.proof.wit)); w.resolver.outcome.logs = [];
  rejects(() => W.tl.admit({ ...g6.proof, wit: w }), "outcome membership", "E5 refund log stripped", "execution");
  W.tl.admit(g4.proof); W.tl.admit(g6.proof);
  ok(W.tl.invariant().outstanding === 30_000_000n && solvent(), "complete evidence admits; solvent", "benign");

  // ===== replay across relayers, epochs and verifier versions =====
  rejects(() => W.tl.admit(g1.proof, "relayer-epoch-B"), "event already consumed", "R1 replay after relayer rotation", "replay");
  // honest producer-set rotation: epoch 0 -> 1 (validators 2..5)
  const rot = W.block([], { reveal: true }); W.publish(rot);
  const e1 = W.block([], { epoch: 1 }); W.publish(e1); ok(W.lc.head.epoch === 1, "honest epoch hand-off", "benign");
  rejects(() => W.tl.admit(g1.proof), "event already consumed", "R2 replay after producer-set rotation", "replay");
  // retired epoch-0 keys cannot finalize an epoch-1 header
  const stale = W.block([], { epoch: 1, signers: W.V.slice(0, 3) });
  rejects(() => W.lc.accept(stale.h, stale.approvals), "insufficient finality stake", "R3 retired producers sign new epoch", "replay");
  // same terminal receipt, different candidate sequence: deposit id still consumed
  rejects(() => W.tl.admit({ ...g1.proof, st: { ...g1.proof.st, candidate: candidateOf(999n) } }), "event identity", "R4 alternate candidate for same receipt", "replay");
  // benign verifier upgrade with continuity
  const V2b = (st, wit) => NEARPAY_V2(st, wit); // same relation, new identity (re-keyed circuit)
  W.tl.propose({ program: programId(V2b), relation: V2b, nullifierRoot: W.tl.nullifierRoot(), consumedCount: W.tl.consumed.size, lcHeadHash: W.lc.head.hash, statementVersionBumped: true, relationSourceReviewed: true });
  for (let i = 0; i < 10; i++) W.tl.tick();
  W.tl.activate(); ok(W.tl.st.verifierEpoch === 2, "benign upgrade activates after delay with continuity", "benign");
  rejects(() => W.tl.admit({ ...g3.proof, program: programId(V2b) }), "event already consumed", "R5 replay under upgraded verifier", "replay");
  rejects(() => W.tl.admit(g3.proof), "program not pinned by domain", "R6 retired program refused", "replay");

  // ===== conflicting checkpoints =====
  const W2 = world();
  const a = W2.block([], {}), bConf = W2.block([{ id: hx("x"), executor: "x.fixture", status: { kind: "value", value: "1" }, children: [], logs: [] }], {});
  W2.publish(a);
  // two quorums at one height need >= 1/3 double signers: validators 0,1,2 sign both
  rejects(() => W2.lc.accept(bConf.h, bConf.approvals), "conflicting finalized checkpoints", "K1 equivocating quorum at same height", "checkpoint");
  ok(W2.lc.frozen && W2.lc.evidence.length === 1, "equivocation recorded as slashable evidence", "checkpoint");
  const late = W2.deposit({ requested: 1n, publishBlock: false });
  rejects(() => W2.lc.accept(late.block.h, late.block.approvals), "light client frozen", "K2 frozen client refuses all heads", "checkpoint");

  // ===== reverse path: burn -> TL statement -> Bitcoin anchor -> vault payout =====
  const K = 3, genesisBtc = { hash: POLICY.bitcoinGenesis, height: 0, prev: hx("none") };
  const btc = new BtcLC(genesisBtc);
  W.tl.apply({ kind: "burn", id: "exit-1", owner: hx("alice-script"), amount: 2_000_000n, destination: "alice.fixture" });
  const exits = [{ id: "exit-1", destination: "alice.fixture", amount: 2_000_000n }];
  const TL0 = T("TLBRIDGE/TLROOT/V1", s("activation"));
  const stmt1 = { prevRoot: TL0, nextRoot: T("TLBRIDGE/TLROOT/V1", s("after-exit-1")), exitRoot: mroot(exits.map(exitLeaf)), exitCount: 1n };
  const TLSTATE_FIXTURE = new Set([tlDigest(stmt1)]); // TRUSTED FIXTURE: TLSTATE01 validity is unproven
  const vaultSide = { root: TL0, accepted: new Map(), exitNullifiers: new Set(), quarantined: false };
  function acceptStatement(t, blockHash) {
    const b = btc.blocks.get(blockHash); req(b && b.packets.includes(packet(t)), "anchor inclusion");
    req(btc.confirmations(blockHash) >= K, "anchor not final on best chain");
    req(t.prevRoot === vaultSide.root, "TL root continuity");
    req(TLSTATE_FIXTURE.has(tlDigest(t)), "TL state validity");
    vaultSide.root = t.nextRoot; vaultSide.accepted.set(tlDigest(t), t);
  }
  function claimExit(digest, x, index, path) {
    req(!vaultSide.quarantined, "vault quarantined");
    const t = vaultSide.accepted.get(digest); req(t, "statement not accepted");
    req(mfold(exitLeaf(x), index, Number(t.exitCount), path) === t.exitRoot, "exit membership");
    const nul = T("TLBRIDGE/EXIT-ID/V1", d(DOMAIN), s(x.id)); req(!vaultSide.exitNullifiers.has(nul), "exit already registered");
    W.vault.registerExit(W.vault.issueExit(x.id, x.destination, x.amount)); vaultSide.exitNullifiers.add(nul);
  }
  let tip = genesisBtc.hash, h = 0;
  const anchor = mine(tip, ++h, [packet(stmt1)]); btc.accept(anchor); tip = anchor.hash;
  rejects(() => acceptStatement(stmt1, anchor.hash), "anchor not final", "B1 anchor with 1 confirmation", "anchor");
  for (let i = 0; i < K - 1; i++) { const x = mine(tip, ++h, []); btc.accept(x); tip = x.hash; }
  const forged = { ...stmt1, exitRoot: mroot([exitLeaf({ id: "exit-1", destination: "mallory.fixture", amount: 2_000_000n })]) };
  const fb = mine(tip, h + 1, [packet(forged)], "fork");
  rejects(() => acceptStatement(forged, fb.hash), "anchor inclusion", "B2 statement anchored in an unknown block", "anchor");
  rejects(() => acceptStatement({ ...stmt1, prevRoot: hx("other") }, anchor.hash), "anchor inclusion", "B3 non-anchored predecessor", "anchor");
  acceptStatement(stmt1, anchor.hash); ok(vaultSide.root === stmt1.nextRoot, "anchored TL statement advances vault root", "benign");
  rejects(() => acceptStatement(stmt1, anchor.hash), "TL root continuity", "B4 statement replay (CAS)", "anchor");
  rejects(() => claimExit(tlDigest(stmt1), { ...exits[0], destination: "mallory.fixture" }, 0, []), "exit membership", "B5 exit destination substitution", "payout");
  claimExit(tlDigest(stmt1), exits[0], 0, []);
  rejects(() => claimExit(tlDigest(stmt1), exits[0], 0, []), "exit already registered", "D1 same exit claimed twice", "payout");
  // exactly-once payout under failures (ZTB-02 vault mock semantics)
  const p1 = W.vault.begin("exit-1"); W.vault.callback(W.vault.simulateTransfer("exit-1", p1.attempt, false));
  ok(W.vault.status("exit-1") === "pending", "authenticated failure returns to pending", "benign");
  const p2 = W.vault.begin("exit-1"); const ticket = W.vault.simulateTransfer("exit-1", p2.attempt, true);
  rejects(() => W.vault.begin("exit-1"), "exit not pending", "D2 crash after transfer: no timeout resend", "payout");
  rejects(() => W.vault.simulateTransfer("exit-1", p2.attempt, true), "invalid transfer attempt", "D3 duplicate transfer for one attempt", "payout");
  W.vault.callback(ticket); rejects(() => W.vault.callback(ticket), "unauthorized callback", "D4 callback replay", "payout");
  W.tl.apply({ kind: "begin_payout", id: "exit-1" }); W.tl.apply({ kind: "payout_result", id: "exit-1", attempt: 1, success: true });
  ok(W.vault.state().R === 2_000_000n && solvent(), "exactly one payout recorded; solvent", "benign");

  // ===== counterexamples: remove one assumption, observe the failure =====
  // X1 consensus: adversary holds >= 2/3 of epoch-1 stake (validators 2,3,4). Forgery is ACCEPTED.
  const W3 = world({ cap: 1_000_000n });
  W3.publish(W3.block([], { reveal: true })); W3.publish(W3.block([], { epoch: 1 }));
  W3.tl.admit(W3.deposit({ requested: 1_000_000n }).proof); W3.tl.tick();
  // Block signed by validators 2,3,4 only (75% of epoch-1 stake); no tokens ever reach the vault.
  const forge = seq => { const f = W3.deposit({ requested: 900_000n, sender: "mallory.fixture", script: hx("mallory-" + seq), publishBlock: false, signers: W3.V.slice(2, 5) }); W3.publish(f.block); return f; };
  W3.tl.admit(forge(1).proof);
  rejects(() => W3.tl.admit(forge(2).proof), "window cap", "C1 cap bounds supermajority forgery per window", "containment");
  ok(!solvent(W3), "X1 supermajority forgery breaks solvency (assumption: honest 2/3 stake)", "counterexample");
  counterexamples.push({ id: "X1", assumption: "NEAR finality: < 1/3 Byzantine stake (2/3 honest quorum)", accepted_forgery: true, loss_atoms: "900000", bounded_by_window_cap: "1000000" });
  // reconciliation on independent physical holdings detects the gap and quarantines
  const reconcile = w => { const i = w.tl.invariant(), v = w.vault.state(); const liab = i.outstanding + i.pending + i.unminted; if (v.holdings < liab) { w.tl.st.quarantined = true; return { quarantined: true, shortfall: liab - v.holdings, haircut_ppm: v.holdings * 1_000_000n / liab }; } return { quarantined: false }; };
  const rec = reconcile(W3);
  ok(rec.quarantined && rec.shortfall === 900_000n, "C2 reserve reconciliation quarantines on shortfall", "containment");
  rejects(() => W3.tl.admit(W3.deposit({ requested: 1n }).proof), "admission closed", "C3 quarantine halts admissions (no bypass of proof checks)", "containment");
  ok(reconcile(W).quarantined === false, "C4 reconciliation does not trip on the solvent world", "benign");
  // X2 custody: a legacy vault with an operational transfer key (not present in vault_mock).
  const legacy = { holdings: W.vault.state().holdings }; const adminTransfer = x => { legacy.holdings -= x; };
  adminTransfer(legacy.holdings - 905n);
  ok(legacy.holdings === 905n, "X2 admin-key vault drains to 905 regardless of proofs", "counterexample");
  counterexamples.push({ id: "X2", assumption: "custody: no vault release path outside the payout relation", accepted_forgery: true, remaining: "905" });
  // X3 anchor: heavier Bitcoin fork (> K deep) after payout removes the burn from TL history.
  const fork = []; let ft = genesisBtc.hash;
  for (let i = 1; i <= h + 2; i++) { const x = mine(ft, i, [], "attacker"); fork.push(x); ft = x.hash; }
  for (const x of fork) btc.accept(x);
  ok(btc.confirmations(anchor.hash) === 0, "X3 deep reorg orphans the anchor after payout", "counterexample");
  counterexamples.push({ id: "X3", assumption: "Bitcoin: no reorg deeper than K after payout (hash-majority honest)", accepted_forgery: true, K, effect: "burn erased from TL history after the vault paid; liability returns while reserve is gone" });
  // X4 upgrade: a migration that snapshots balances but not consumed identities. Clearing only the
  // bridge nullifier set is NOT enough: the ZTB-01 ledger's deposit map refuses the replay too.
  const W4 = world(); const d4 = W4.deposit({ requested: 7n }); W4.tl.admit(d4.proof);
  W4.tl.consumed.clear();
  rejects(() => W4.tl.admit(d4.proof), "deposit replay", "X4a ledger deposit map is a second nullifier", "upgrade");
  W4.tl.st.s = makeModel().fresh();
  W4.tl.apply({ kind: "deposit", id: "migration-snapshot", owner: hx("alice-script"), amount: 7n });
  W4.tl.apply({ kind: "mint", id: "migration-snapshot" });
  W4.tl.admit(d4.proof);
  ok(!solvent(W4), "X4 balance-only state migration replays a deposit into insolvency", "counterexample");
  counterexamples.push({ id: "X4", assumption: "upgrade continuity: consumed identities (bridge set AND ledger deposit map) persist", accepted_forgery: true });

  // ===== BitAgent response policy (propose-only) under inconsistent collateral evidence =====
  function bitagent(ev) {
    const out = { signs: false, broadcasts: false, proposals: [] };
    const authenticated = ev.lightClientFrozen === false && ev.reconciled === true;
    if (!authenticated) out.proposals.push("treat bridged asset as unauthenticated collateral: no new exposure, no collateral admission");
    if (!ev.transfersOpen) out.proposals.push("nominal cross-venue spread is not executable: matched transfer-dependent volume 0");
    const share = ev.buyCapacity / BigInt(Math.max(1, ev.estimatedAgents));
    out.maxSell = ev.transfersOpen || ev.venueLocalSellOk ? (ev.position < share ? ev.position : share) : 0n;
    out.proposals.push("stagger, do not synchronize: per-agent sell <= capacity / estimated agents");
    return out;
  }
  const ba = bitagent({ lightClientFrozen: false, reconciled: false, transfersOpen: false, venueLocalSellOk: true, buyCapacity: 10_000n, estimatedAgents: 1000, position: 100n });
  ok(!ba.signs && !ba.broadcasts, "BitAgent never signs or broadcasts", "agent");
  ok(ba.maxSell === 10n && 1000n * ba.maxSell <= 10_000n, "BitAgent sell share fits venue capacity at N=1000", "agent");
  ok(ba.proposals[0].startsWith("treat bridged asset as unauthenticated"), "BitAgent refuses inconsistent collateral", "agent");
  const ba2 = bitagent({ lightClientFrozen: false, reconciled: false, transfersOpen: false, venueLocalSellOk: false, buyCapacity: 10_000n, estimatedAgents: 1, position: 100n });
  ok(ba2.maxSell === 0n, "BitAgent proposes no sale where no venue can execute", "agent");

  return {
    experiment: "BRIDGE-SEC-02", checks, categories: tally, counterexamples,
    pins: { domain: DOMAIN, program: PROGRAM, epoch0_bp: bpHash(W.set0), epoch1_bp: bpHash(W.set1), btc_anchor: anchor.hash, nullifier_root_after: W.tl.nullifierRoot() },
    final: { outstanding: String(W.tl.invariant().outstanding), vault_holdings: String(W.vault.state().holdings), paid: String(W.vault.state().R) },
    source_event_oracle: "replaced by fixture light client (Ed25519 2/3 stake, epoch hand-off); NOT NEAR's header format",
    tl_state_validity: "TRUSTED FIXTURE (TLSTATE01 unproven)",
    native_finality_verified: false, nova_verified: false, near_decider_verified: false, btc_anchor_verified: false, economic_authorized: false, production_ready: false,
  };
}

const result = run();
const evidenceUrl = new URL("./bridge_sec_02_evidence.json", import.meta.url);
if (process.argv.includes("--freeze")) writeFileSync(evidenceUrl, JSON.stringify(result, null, 2) + "\n");
assert.deepEqual(JSON.parse(JSON.stringify(result)), JSON.parse(readFileSync(evidenceUrl, "utf8")));
console.log(JSON.stringify(result, null, 2));
