# Literal ClearList lifecycle (work-only MainCallableV8)

This stage composes the unchanged pinned ClearList, Types, Validity and Logic
methods over the existing Core's sole `clearlists` and `attestations` collections.
It adds four true singleton caches (`clearlists`, `banlist`, `xpubCache`,
`merkleXpubMap`), plus the explicitly selected BIP32 module availability and its startup lock. It does
not instantiate the standalone ClearlistSourceOwner as another catalog, copy its
DB rows, create another Ledger, or accept a validation/financial result.

The source is `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`; full ClearList body SHA256
is `fbdb702e5e89bac113226223f42bf762af13fff40dee4e0c4ab6cd4a50d451e1`.
The whole-module oracle loads the same pinned Types/Validity/Logic/Tally/Property
and related original modules as the existing composition harness. Wrappers only
record arguments and return the original value or Promise. Result, state and
effect snapshots are copied at observation time. Console records diagnostics and
returns undefined, preserving branches which literally return console.log(...).

## Scope and retained behavior

The new Core methods cover all 25 ClearList static methods, including creation
aliases, loads/ID allocation, admin, attestations/revocation/history, country and
banlist reads/writes, root registration/proofs and public-key derivation. New
Logic routes are createClearList, issueOrRevokeAttestation, updateBannedCountries.
The existing Logic.updateAdmin branch now reaches the actual ClearList owner
under the new policy. Raw7 and raw9 use original Types/Validity plus exact Logic
arguments; raw8 retains the already composed generic admin dispatch.

Source defects and failure boundaries stay observable:

- createClearlist ignores url in its stored object, loads IDs only for falsey
  supplied IDs, writes the DB, and does not cache the created row.
- Loads merge into the cache and retain stale entries. Their caught failure
  returns undefined. Typed Map keys remain distinct; parseInt allocates IDs.
- getList reads `attestations`, not `clearlists`. Raw8 can therefore reject a
  perfectly real list until a suitable attestation row exists. No validator
  answer is supplied to bypass this path.
- updateAdmin mutates the cached object before either awaited DB operation. A
  failed factory/write retains that mutation. It writes a string `_id`, even if
  the cache key was numeric, so numeric and string shadow rows can coexist. A
  later reload can restore the old numeric administrator.
- Attestations use only address as `_id`. Adding list 0 self-certification can
  overwrite a row from another list. Raw9 actually supplies params.sender;
  this working field plumbing is preserved.
- Decoder9 uppercases metaData. A raw `xpub:` prefix becomes `XPUB:` and is not
  recognized by addAttestation's case-sensitive detection. Direct mixed-case
  xpub inputs keep their original behavior.
- Root storage follows the ordinary attestation write and can fail afterward.
  Revoking an address does not erase separately stored root records or the
  volatile root registry. Multi-row revocation can persist earlier rows before
  a later write rejects; its legacy fallback ignores canonical list matching.
- List 0 BANLIST uses the actual protocol-admin check and normalizes metadata
  using the source separators/whitespace. updateBannedCountries accepts only
  an array whose elements each have two UTF-16 code units; invalid input returns
  the console result (undefined) without writing. Successful setBanlist updates
  the cache only after the DB write. getBanlist retains the source array.data
  mistake and returns undefined even when the persisted banlist row exists.
- Country reads accept current/legacy list-id fields, default status, trimmed
  uppercase string/object metadata, and the original blockHeight/timestamp
  fallback. Number(listIdRaw) executes before the status guard, including for
  revoked rows. A malformed revoked row therefore returns null via the source
  catch instead of being skipped in favor of a later valid country row.
- Derivation ignores the network argument. It derives the genuine first 20
  receive and 20 change public addresses and caches successful arrays by xpub.
  Invalid keys return [] without caching. Missing Litecoin import selects the
  original Bitcoin fallback; invalid keys do not change the selected library.

Pure Merkle verification/tree/proof helpers and the verified V8 history sort are
reused through one new stateless `source_clearlist_pure_unverified` bridge in the
existing owner module. It calls those pure functions directly; it never allocates
an owner or collections. BIP32 uses existing `bip32_public` source-compatible
public derivation. New Core caches are the sole live cache authority.

## Versioned admission and explicit boundaries

The work-only MainCallableV8 fragment is gated by main_callable_enabled. It
inherits V7 capabilities without changing any V1-V7 method gate or diagnostic
shape. Its four caches and selected library appear only in clearlistLifecycle
under V8. Parent/Main owns policy/TLCR8/bootstrap framing and independent V7
fixture preservation. The supplied actual V7 release capture is independently
pinned to commit `cd9dd14c68192e77ffd230531505c9fe45ba2585`, 37 commands, 70,918 bytes,
SHA256 `55e364a0f2bfa7626bd58d08347695f77a08478b5ccace9ee6f69ff9c95ecf54`,
head `e5337955bfaeca0102ed77f8b5b7f8ced805e03df7d02f77e0fc075eefffc66d`.

Standalone oracle tests explicitly configure SourceMain's replay-bound
`activationProfile: declared-answer` before the first Activation/Main use. The
unchanged Validity body still executes all checks; the activation/RPC environment
is declared input, not a supplied validity result. Main requires the actual
source-registry profile and refuses the standalone profile. Profile locking and
shared activation state are owned by the Main fragment.

SourceClearList.configureLibrary is an explicit, replay-bound startup dependency
input selecting litecoin or bitcoin-fallback before the first derivation attempt
(including an invalid key). Its explicit startup lock is bound in diagnostics and
replay and rejects later configuration changes. It supplies no
key/address/proof outcome; the actual algorithm runs. The default is genuine
Litecoin availability. Both libraries are exercised in separate Node processes
because the packages patch shared elliptic prototypes. Both modules unavailable,
private-key expansion and unrepresented procedural capabilities remain outside
this bounded public-key transport; no usable address is invented.

Map key/object reference identities, JS accessors/functions/Proxy/Symbol values,
array non-index properties, Buffers as caller crypto inputs and out-of-bound
numeric/storage domains are explicit local refusals. They roll back the local
command and are not labeled source throws or counted as matching source effects.
The object-address probe executes the original JS, records its real prefix, then
separately verifies the Rust refusal/rollback. Supported source exceptions retain
prior mutations and writes; no blanket transaction or conserving repair applies.

## Evidence and current status

Before implementation, ten actual composed source observations passed under both
archived BigNumber 9.3.1 and lock-pinned 9.1.2. Their JSONL SHA256 is
`407002a358d187a133bbf5803ef6a08d9ecc8b20214a18f9eb3d917c5438ea26`.

The expanded source corpus has 204 observations per real library: 27 resets,
7 explicit row imports, 59 raw calls, 110 intended-comparable direct calls, and
1 explicit object-key local-refusal probe. Both genuine numeric versions produce
identical bytes within each library. Litecoin corpus SHA256 is
`51d889827d1a5d863db7a08bd4864c30affefac4eb9d292e7bc5c9f5a93750f0`;
Bitcoin-fallback corpus SHA256 is
`9db013875de392cd5ecb4617d8abc9ca999714d30770063da43e9dbbf6e8a133`.
This is tested corpus equality, not a whole-version equivalence claim.

Existing standalone ClearList-owner and public BIP32 evidence remain separate:
those earlier owner/200+800-case checks do not by themselves prove this Core
composition. The three staged Rust tests compare full source outcomes, async
transports, actual maps/flags/shared rows and complete ordered DB/dependency
traces for both libraries/numeric versions; replay all 20 selected failure/cache
sequences, including source write prefixes; and preserve earlier-policy refusal.
The public-address/root cache sequence checkpoints at five actual continuation
points instead of redundantly re-deriving every intermediate row.

Current checks: all four genuine Node executions pass; Rust 1.90 formatting
passes; fresh committed-V7 Main policy/runtime then ClearList runtime/owner pure
bridge text transforms pass all unique anchors. Rust compilation/differential
execution is intentionally pending the parent-granted coherent build slot.
No root files were edited and no whole library was built by this lane.
