# Immutable economic service views and the interface facade

`service_view::EconomicReadBackend` implements the shared `ServiceBackend` by
borrowing one `EconomicChain`. `from_chain` verifies the economic invariants;
`from_durable` first checks the durable owner's journal health through `chain()`.
The borrow prevents changing the owner while a view is alive. Queries allocate
response/history metadata, never another mutable Ledger. The runtime wrapper
owns the server, its durable head and any control capability.

`ImmutableEconomicReadV1` is a useful read policy over the current typed owner.
It is separate from historical JavaScript facade behavior. Chain inputs remain
unverified assertions: `runtime.status.authenticated` is false. The backend
does not claim authenticated transactions, UTXOs, scripts, wallet control,
historical consensus hashes or a complete external database history.

## Available state

| Targets | Actual owner data and limits |
| --- | --- |
| `runtime.status`, `runtime.heights` | Network, known economic tip/block ID and rollback floor. `maxProcessedHeight` is the owner's finalized economic height; parsed/track heights are null because this owner has no parser/index provenance. `durableSequence` is null: the facade does not rehash or invent a durable head per request. |
| `ledger.balances`, `wallet.balances` | Per-property public available atoms, contract margin/reserve, resting spot reserve, channel escrow, pending withdrawal and their checked `ownedAtoms` sum. These categories remain distinct; locked amounts are not spendable available. |
| `ledger.properties`, `ledger.property`, `ledger.total_tokens` | Only real Ledger property ID, issuer, fixed/managed kind and exact supply. No guessed ticker, divisibility, grants or issuance metadata. |
| `ledger.addresses_with_property` | Known public owners with any actual ownership category, including zero available but live reserve/margin/channel/pending custody. |
| `ledger.audit`, `ledger.all_balances` | Deterministically sorted nonzero public owner/property rows with the same custody categories. This means known owner accounts, not a WalletCache or proof that keys belong to a local wallet. |
| `ledger.custody` | Separate private account custody plus the actual loss pool, insurance custody and historical gross unfunded-loss counter. Private accounts beginning `~` are never returned as public available balance owners. |
| `registry.activations`, `registry.is_active`, `registry.feature_status` | Actual recorded activation heights and code hashes; the feature-status message agrees with the source for bounded integer IDs. |
| `registry.oracles`, `registry.clear_lists`, `registry.clear_list`, `registry.attestations` | Actual accepted Registry metadata, latest oracle quote/closure and attestation state. Oracle prices retain exact 1e4 integers and Decimal display. |
| `contract.list`, `contract.info` | Actual admitted definitions: issuer/ticker, oracle/native/inverse, notional/collateral IDs, leverage, expiry lists, fee/whitelist, on-chain pairs and catalog AMM configuration. Legacy configurations without admitted catalog evidence return `Unavailable`; their parameters are separately exposed by `contract.runtime_config`. |
| `contract.initial_margin` | An admitted selected contract uses the catalog's source price/leverage calculation and Number boundary. Legacy fixed-margin configuration returns its explicitly configured fixed value. Price is a bounded positive Decimal string. |
| `contract.book`, `spot.book` | Actual queued/resting metadata, exact remaining amounts and reserves. Contract order IDs remain external order identities; AMM quotes are separate. |
| `contract.position`, `wallet.position`, `wallet.positions` | Actual selected-contract quantities, trade average, margin, realized/unrealized PnL, last mark, exact settled basis/carry ratios and unpaid loss. An absent position is null (or an empty positions array). These are the one owner's positions, not every position from external storage. |
| `contract.amm` | Actual funded pool account, available cash, margin, LP share units, generation and funded quote metadata. Share units are reporting claims, not an extra token balance. Disabled pools return `Unavailable`. |
| `history.contract`, `history.contract_for_address` | Rebuild `EconomicHistory` from verified economic events. Preserve actual `orders`, `forced_book` and `forced_amm` origins, buyer/seller, exact quantity/price, participant booked PnL and event-wide paid/collected/unfunded amounts. Older paid claims are never assigned to invented participants. Receipts without owner evidence return `Unavailable`. |
| `history.spot` | Actual retained spot fill IDs/height/amounts. Spot fills do not retain owner evidence, so address-filtered spot history returns `Unavailable`. `history.address` also refuses an incomplete mixed history when any such spot fill exists. |
| `history.funding` | Retained epochs, actual status/index/window, exact positions and available-balance captures, rates, debits and credits. Optional inclusive height bounds never imply an external historical database. Disabled funding returns `Unavailable`. |
| `history.oracle` | The selected contract's actual retained funding observations/epoch windows and Mark observations, deduplicated by height/ID. Mark-only configurations work. This is the known retained corpus; source publication/upsert and historical completeness are not inferred. |
| `history.clearing`, `interface.audit_data` | Actual Mark events and ordinary/forced settlement entries, including price, delta, booked atoms, available/margin captures, collections and claim/default offsets. The latter filters the requested known height/contract. |
| `fees.all`, `fees.property`, `iou.claims`, `iou.buckets` | Actual fee value/stash custody and exact IOU bucket/claim metadata. |
| `channel.get`, `channel.balance_for_committer` | Scoped `ledger_channel` owner/column/amount facts if present. They do not fabricate the external channel lifecycle, commitments or column-prediction policy. The current Joint executor rejects unsupported legacy channel liabilities. |
| `interface.insurance_balance` | Explicitly labelled economic insurance custody, not a fabricated external insurance fund account/history. |

IDs/heights use unsigned JSON numbers. Atoms, Decimal quantities/prices and
rational components use exact JSON strings, including values above 2^53.
Amounts are never routed through binary64. Strings use the existing strict
`RpcValue` JSON encoder. Every response respects its 16 MiB, depth and node
bounds; audit/public-owner rows are additionally limited to 65,536. Invalid
arity/types, reversed ranges and malformed prices fail explicitly. Unknown
owned properties return `NotFound`; another contract runtime is unavailable.
No response is a silently truncated success.

External transaction lookup/block ranges, mempool, wallet load/signing/OP_RETURN,
runtime mutation, issuance intent storage/preparation, BitVM, synthetic/vaults,
options chains, property grants/pay-to-token history, consensus-hash storage,
column prediction, wallet-cache mutation and insurance payout history return a
named `ServiceError::Unavailable`. They are not guessed empty arrays. The
generic facade below lets an application attach actual capabilities separately.

## Complete source interface surface

`InterfaceFacade<B>` exposes all 25 typed query/audit capabilities through
`InterfaceQuery`, delegating to `ServiceCall`. `audit_lines` supplies the remaining
source stream utility. Constructor setup is an empty backend borrow; no singleton,
disk write, wallet cache or external capability is initialized implicitly.

| Active source method | Useful typed call/behavior |
| --- | --- |
| `JSONAuditTallyMap` | `AuditTally`; `tally_audit_bytes` returns bounded audit bytes. Caller owns any file destination. |
| `createReadableStreamFromIterable` | `audit_lines` over strict JSON values. |
| `getConsensusHashForBlock` | `ConsensusHash(height)`; external historical hash capability. |
| `getFeatureActivationStatus` | `FeatureStatus(id)`. |
| `getAllBalancesForAddress` | `Balances(owner)`. |
| `getTotalTokens` | `TotalTokens(property)`. |
| `getBalancesAcrossAllWallets` | `AllBalances`; actual owner/property rows under the named repair. |
| `isTransactionTypeActive` | `IsActive(type)`. |
| `getAllActiveTransactionTypes` | `ActiveTypes`; includes actual activation metadata. |
| `getAddressesWithBalanceForProperty` | `Addresses(property)`. |
| `getTransaction` | `Transaction(id)`; external transaction-index capability. |
| `getProperty`, `listProperties` | `Property(id)`, `Properties`. |
| `getGrants`, `getPayToToken` | `Grants(id)`, `PayToToken(id)`; external property-history capabilities. |
| `listBlockTransactions`, `listBlocksTransactions` | `BlockTransactions(height)`, `BlockRange(first,last)`. |
| `listPendingTransactions` | `Pending(address_filter)`; external mempool capability. |
| `updateWalletCache` | `UpdateWalletCache`; mutation capability unavailable on the immutable backend. |
| `listOracles`, `listWhitelists` | `Oracles`, `Whitelists`. |
| `listVaultsBySyntheticProperty` | `Vaults(property)`; external vault capability. |
| `getOptionsChainBySeriesId` | `OptionsChain(series)`; external options capability. |
| `getInsuranceFundBalance` | `InsuranceBalance(contract)`; explicitly scoped custody projection. |
| `getInsuranceFundPayoutHistory` | `InsurancePayouts {contract,start,end}`; external history capability, optional end retained. |
| `getAuditData` | `AuditData {height,contract}`; actual retained economic clearing projection. |

## Unchanged-source evidence

The complete 8,152-byte checkout source is archived with LF normalization as
`fixtures/legacy_interface_source.txt`, from commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, SHA-256
`d2887a94f2e6770194aa976b91a155fe534b14f96dc4ba99c91f00dcff8181bb`.
The source has 28 method declarations including constructor and the duplicate
wallet method; its runtime prototype has constructor plus 26 active methods.
The second `getBalancesAcrossAllWallets` overwrites the first.

`scripts/service_view_vectors.cjs` executes that entire source without rewriting
expressions. Built-in `stream`, `util.promisify` and real pipeline behavior run;
an injected writable records exact filename and bytes instead of writing into
the repository. Imported dependency adapters and optional globals record exact
target/argument traces. These nominal adapters are explicit observations of
delegation, not claims that actual legacy exports implement the requested APIs.
Empty-import and missing-global scenes record the failures separately.

The deterministic corpus contains one full inventory, 90 call scenes and five
exact stream-byte cases. Missing `WalletCache`, `BlockHistory`, `OracleManager`,
`WhitelistManager`, `VaultManager`, `OptionsChainManager`, `BlockChainInfo`, and
unset `this.clearing` are concrete failure observations. In particular:

- The source constructor never assigns `this.clearing`; `getAuditData` logs and
  rethrows its failure, and interpolates unsanitized/coercive keys if supplied.
- An omitted/null insurance-history end first calls the undeclared latest-height
  provider. Explicit zero avoids that branch.
- Pending filter defaults only for undefined; explicit null passes through.
- Audit `load` and iterable construction happen before the try/catch, so their
  failures escape and no file is opened. Pipeline/write/iterator/stringify
  failures occur inside the try and are logged and swallowed.
- The source stream writes independently pretty-printed items plus LF, not one
  JSON array. Undefined becomes literal `undefined` plus LF; nonfinite numbers
  become `null`, negative zero becomes `0`; bigint/cycles fail during streaming.

`LosslessJsonAuditV1` deliberately accepts strict `RpcValue` values and preserves
their numeric lexemes/key order. It adds the same four-space/LF framing, agrees
with the five JSON-safe source cases, and keeps exact large numeric text rather
than coercing it through Number. It is not a general emulation of JavaScript
object enumeration, coercion, undefined or nonfinite values. No audit write is
performed by the immutable backend.

Regenerate with `node scripts/service_view_vectors.cjs --write`; verify with
`--check`. `--source PATH` executes another full source file only if its normalized
SHA matches. Rust tests cover the exact source stream corpus, all facade calls,
real post-fill state on three networks, real AMM/book forced origins, paid
funding, admitted dynamic-margin catalog, custody-only spot owners, u128 supply,
durable reads, explicit failures and unchanged checkpoints.
