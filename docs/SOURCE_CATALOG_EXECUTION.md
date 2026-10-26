# Actual catalogs in sequential source execution

`EconomicState::from_joint_with_source_catalogs_unverified` installs the actual
`PropertyCatalog` and `ClearlistSourceOwner` alongside the existing sole Ledger.
It requires independently expected commitments for both the complete joint
bootstrap and the complete catalog bootstrap. Catalog circulation declarations
never create bootstrap balances. The caller supplies actual initial custody
through the existing joint owner.

Source blocks run the pinned Types decoder and validity rules against the real
pending economic owner. Property and clearlist dependency reads use those owned
catalogs, including genuine cache and query effects. No test `valid=true` reply
or caller-selected property row supplies transaction admission. The raw block,
actor addresses and transaction identities are still unverified L1 claims.

After a freshly computed successful source evaluation, shared binding checks
match original transaction identity, actor, decoder route and payload, validator
route, and block height before catalog mutation. These checks are not an
authentication capability: the stage itself must compute the evaluation.

Type1 first allocates its ID through actual `PropertyCatalog.createToken` and
then issues the corresponding exact atoms on the existing Ledger under the
same original transaction identity. For a catalog whose largest key is 7, the
next successful source issuance uses property 8. The generic public diagnostic
economic mapper continues to reject type1 because it has no catalog allocation.
The crate-private allocated-ID mapper preserves source class precedence:
NFT selects Non-Fungible, otherwise managed selects Managed or Fixed; a nonnull
procedural subtype combined with managed overrides NFT and selects Procedural.
Non-Fungible initialization retains source class6 metadata and the same ordinary
balance-credit path, backed by existing Fixed custody. The pinned issuance
source does not define a separate per-item NFT mechanism, so none is invented
as a completion requirement. Procedural initialization uses Managed custody;
its later procedural contract-state issuance/redemption remains unsupported.

Ordinary managed grants and issuer self-redemptions call the actual catalog
helpers. Their `Tally.updateBalance` capability is bound to the expected source
arguments and applies the corresponding operation to the same sole Ledger.
Supply metadata, available custody and processed IDs are therefore changed by
one successful candidate. Redemption validity reads actual available Ledger
balance, rather than treating declared circulation as issuer funds.

Types7 and9 write the actual clearlist owner rather than creating substitute
Registry rows. Source address-only attestation keys remain significant: a
list0 self-certification can overwrite a list1 attestation at the same address.
Token admin execution preserves the captured Logic reference defect
`PropertyList is not defined`; the raw catalog `updateAdmin` method remains
available as source helper behavior. Procedural grant/redemption context and
other unsupported dependencies remain explicit failures.

## Deliberate execution policy differences

`ConservingSequentialV1` is an explicit repaired execution policy, not a claim of
complete source parity. It requires prior activation for the typed economic
operation, stages each candidate atomically, preserves rejected-candidate replay
evidence, and publishes the finished block once. The source's raw property
helpers can leave changed singleton state or tally effects after later failure;
this outer candidate policy discards those candidate business mutations.

For example, an activation and issuance in the same block can produce a source
validity success followed by an economic prior-activation rejection. The failed
candidate does not consume the catalog ID or create Ledger funds. Its txid is
still retained in raw source history and cannot be retried as a new transaction.
Genuine validator query/cache observations can also remain in the catalog
journal after rejection. Atomic business state therefore does not mean that the
complete diagnostic/history state remains unchanged.

The raw property owner, bounded clearlist owner, BIP32 public derivation and
existing strict Ledger each retain their documented domains. This integration
does not authenticate signatures or L1 extraction, implement all transaction
types or procedural dependencies, or establish an exact historical source
bootstrap from declarations alone.

## Checkpoint and test evidence

The `TLEC` version10 wrapper stores the independently pinned initial catalogs,
their complete current state, the original raw source-block history and the
existing economic owner checkpoint. Restore checks caller-supplied expected
configuration, validates both complete catalog codecs, reruns source history
against actual restored owners, and compares the resulting complete state.
Checksums and self-consistent histories do not authenticate an attacker-selected
configuration.

`tests/economic_source_catalog_execution.rs` exercises the real constructor and
source block API, without a substituted validity backend:

- Bootstrap property7, actual automatic allocation8, source metadata and exact
  issuance custody under the original txid.
- Managed grant and redemption changing both actual catalog circulation and the
  sole Ledger; checkpoint restore and identical continuation.
- Duplicate ticker, wrong actor, insufficient available balance and the token
  admin source defect retaining business state; rejected and successful txid
  replay blocking; the next successful allocation remains9.
- Same-block activation rejection discarding candidate allocation while
  retaining its rejected txid in raw source history.
- Independent bootstrap/configuration pins and corruption rejection.
- Actual type7/type9 clearlist writes and the source's cross-list overwrite
  behavior, with no fabricated Registry whitelist.

Run `cargo test --test economic_source_catalog_execution`. Catalog-only source
fidelity and numerical evidence are documented separately in
`PROPERTY_CATALOG.md` and `CLEARLIST_SOURCE_OWNER.md`; this integrated test suite
does not generalize their bounded evidence into whole-protocol parity.
