# Full pinned validity rules: bounded diagnostic execution

This module implements all **43 active exported functions** of complete pinned `src/validity.js` at `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The full LF source is archived without edits; SHA256 is `ed9b7fed5b733740c26033030379abe0f4a63d488ff431dfc7b1f10eb4f93b00`. The existing Type0/7/8/9 audit remains unchanged. Method coverage is not arbitrary JavaScript-value coverage or historical acceptance.

The port is a real predicate evaluator. It returns original source values, source exceptions, ordered argument mutations, dependency calls, deferred Promise rejections and source global writes. It owns no Ledger and accepts no caller-supplied authentication flag. `require_authenticated_admission()` always returns `UnauthenticatedInputAndState`.

## API and dependency contract

`SourceCall { method, arguments }` contains the actual ordered source arguments. `evaluate_unverified(&call, &mut backend)` clones bounded arguments and returns `SourceEvaluation { outcome, arguments_after, dependencies, deferred_rejections, global_writes }`. A source throw preserves mutations and calls that occurred earlier. The original call remains unchanged. A local capacity/domain/backend failure is a typed `SourceRuleError`, not a fabricated source rejection.

`SourceValue` distinguishes undefined, null, Boolean, exact binary64 bits (including negative zero and tagged nonfinite values), String, ordered object, array, opaque Promise and BigNumber decimal text. `get` returns an owned undefined value for an absent field. `insert` overwrites an existing object slot in place. `from_json` accepts the existing bounded finite JSON representation; `from_diagnostic_json` also preserves undefined, negative zero, nonfinite, Promise and BigNumber tags. The shared representation permits own `toString`/`valueOf` data so other source ports can model their behavior; this validity evaluator explicitly restricts that coercion domain below.

`SourceBackend::invoke(&DependencyCall { target, arguments })` supplies one of:

- `Immediate(value)`: a synchronous dependency result.
- `Pending(value)`: a fulfilled asynchronous result. The evaluator unwraps it only where the source actually awaits it. Otherwise it uses a truthy Promise whose ordinary properties are undefined.
- `Throw(exception)`: a synchronous source exception.
- `PendingThrow(exception)`: a rejected asynchronous result. An awaited rejection throws at that source expression. An unawaited rejection leaves a Promise in the source computation and is recorded in `deferred_rejections`.

The backend is an explicit fact/capability boundary. It is not an authenticated database by construction. Root integration must recompute answers from an actual immutable healthy owner; replaying caller-selected dependency answers cannot admit a transaction. Source factories (`Margin.getInstance`, `loadMarginMap`, orderbook instance, database and vesting instance) are recorded, followed by their actual read calls. Their snapshot return value is an opaque null marker; the oracle supplies the corresponding adapter instance. No missing global is injected.

Guest-argument functions (`validatePayToTokens`, Bai Urbun, Murabaha and batch proof checks) have functional calls with `arguments: [params]` and explicitly named `GuestTally`, `GuestChannels`, `Bai`, `Murabaha` and `Zk` capabilities. Supplying extra source argument values preserves their actual method-access failure rather than silently normalizing Types' wrong `(sender, params, txid)` signatures.

## Every active validator

| Type | Export | Concrete behavior retained |
| --- | --- | --- |
| 0 | `validateActivateTradeLayer` | Number coercion/filtering, wrong singular/plural scalar guard, fractional IDs allowed in range, admin and bounds checks, early returns and mutation order |
| 1 | `validateTokenIssue` | Integer amount, ticker length/existence/lowercase-s prefix, always-invalid native/vesting OR conditions, procedural-managed checks, undefined reason concatenation |
| 2 | `validateSend` | Address fallback, each property read, restricted procedural checks, country/threshold exception, tally and clearlist checks, actual undefined `isValidAmount` array failure |
| 3 | `validateTradeTokenForUTXO` | Amount reduction by shortfall, later checks setting valid back to true, empty-output undefined return, satoshi/unit inconsistency, misspelled payment field producing NaN, undefined `reference` failure |
| 4 | `validateCommit` | Reference resolution, address and available balance, vesting/restricted flags, country and list checks, real source pubkey/multisig dependency order, pay/list metadata |
| 5 | `validateOnChainTokenForToken` | Amounts, vesting/self-pair, country, offered balance and both property/list rules, source conjunction requiring both properties' nonzero list fields |
| 6 | `validateCancelOrder` | Property/contract/order queries, source `fromAddress` mismatch, cancel parameter checks, deprecated txid rejection and null cancel object failure |
| 7 | `validateCreateWhitelist` | Activation, required backup/name, native address helper then RPC camel-or-lowercase fallback |
| 8 | `validateUpdateAdmin` | Exactly one target, each source whitelist/oracle/managed-token admin/backup lookup, nested oracle fields, lowercase-only RPC address fallback |
| 9 | `validateIssueOrRevokeAttestation` | Ref resolution (including null-to-zero), primary-admin positive lists, list-zero self-cert/banlist paths, exact code parsing and revoke membership |
| 10 | `validateAMMPool` | Contract/property variants, amounts, source property-object-versus-number vesting comparison, every declared list, optional strategy JSON and integer option IDs |
| 11 | `validateGrantManagedToken` | Admin/type rules, reference/default destination, procedural destination/DLC gate and address dependencies |
| 12 | `validateRedeemManagedToken` | Procedural/admin exception, tally available comparison and redemption gate |
| 13 | `validateCreateOracle` | The source only checks activation; additional field/admin success criteria are not invented |
| 14 | `validatePublishOracleData` | Awaited admin check, price and present optional fields, activation and oracle lookup, primitive-params `in`-operator throw |
| 15 | `validateCloseOracle` | Source's unawaited admin Promise in `params.valid`, activation15 and historical unconditional isActivated call; Types actually invokes type14's validator instead |
| 16 | `validateCreateContractSeries` | Oracle/native/collateral/notional checks, awaited duplicate lookup, unawaited native property promises, empty pairs, source vesting expression and scalar field rules |
| 17 | `validateExerciseDerivative` | Activation then genuine `ReferenceError: derivativeRegistry is not defined`; no fake derivative manager is supplied |
| 18 | `validateTradeContractOnchain` | Actual reference/band/order, country, reduce/flip margin requirement, shortfall quantity reduction (including source negative results), ignored final balance result and undeclared global assignment |
| 19 | `validateTradeContractChannel` | Amount floor, reference/band/expiry, channel participants/countries/lists, channel-versus-effective balance checks, source side labels, reduce/flip branches and shortfall arithmetic |
| 20 | `validateTradeTokensChannel` | Activation early return, expiry and pair/procedural checks, channel/country/list policy, wrong missing-await sufficiency results, column checks and genuine missing `isListed` call |
| 21 | `validateWithdrawal` | Reference/channel/participant checks, actual column semantics, missing column and balance comparison; no absent source amount-sign check is added |
| 22 | `validateTransfer` | Strict reference comparison, source channel Promise, selected custody column, destination occupancy, pay defaults and payRef/list/output dependencies |
| 23 | `validateSettleChannelPNL` | All four settle types, early activation/channel/neutralization rules, mark band, net balance, king metadata and expiry |
| 24 | `validateMintSynthetic` | ROUND_DOWN request/hedge caps, inverse/native flags, entry-price target with ROUND_UP, margin slice, source BN argument, true negative-zero clamp and missing `initMargin` failure |
| 25 | `validateRedeemSynthetic` | Synthetic ID rewriting, floor, long-position block, existence/balance reads and available-based request reduction |
| 26 | `validatePayToTokens` | Actual synchronous property read from the returned capability value; an unawaited Promise yields undefined; wrong Types argument order throws |
| 27 | `validateOptionTrade` | Primary/combo parsing and series/channel, spread width/premium/leftover requirement, reduce/flip and BigNumber realized PnL, protected-wing unwind, portfolio maintenance transition, effective balance and expiry |
| 28 | `validateTradeBaiUrbun` | Both external calls execute before the source logical-and result; wrong Types arguments remain a method-access error |
| 29 | `validateTradeMurabaha` | Exact source parameter order and both capability calls, logical-and value result and signature error |
| 30 | `validateStakeFraudProof` | Stake/fraud/relay branches, oracle/admin metadata, balance, relay fields and explicit DLC parse/verify dependency results |
| 31 | `validateKingSettle` | Ordered early checks, payer balance, all three scaling arrays and exact inclusive active-item count |
| 32 | `validateBatchMoveZkRollup` | Both proof/payment capability calls execute, then source logical-and returns their actual value; this is not a locally verified proof |

The other **10 exports** are functional: `isActivated` (unconditional true), `isValidNumber`, `isRestrictedProceduralToken`, `isTradePriceWithinLeverageBounds`, `hasReferencePrice`, `hasSufficientChannelMargin`, `checkFlipMargin`, `isListed`, `checkWhitelist` and `parseTicker`. `checkWhitelist` genuinely throws on its missing unqualified `isListed`; exported `isListed` itself works. The permissive validity ticker parser differs from Options' stricter parser.

The complete source also contains private Base58/Bech32/address helpers and unused xpub helpers. The port reproduces the active address helper's prefix selection/checksum path and delegates Bech32 decoding explicitly. The source's malformed testnet selection and leading-zero Base58 padding remain observable through actual fallback calls. Node crypto is genuine. Unused bitcoin/bip32 dependencies are inert and throw if their unreachable derivation adapter is called; no xpub derivation or address authorization is claimed.

## Oracle evidence and deliberate limits

The complete-source VM uses genuine archived BigNumber9.3.1, unchanged genuine `options.js` and built-in Node crypto. Other dependencies are named recorded snapshots, including address RPC, country/list data, contract and margin rows, guest proof/payment capabilities and DLC verification. A supplied `valid` dependency answer is a diagnostic input, not a verified cryptographic fact. Factory handles, initialization and snapshots are explicit adapters; this corpus does not execute a real DB/RPC stack.

`validity_vectors.tsv` contains **562 exact differential cases** across all 43 exports, plus **two separately labeled Q observations** outside the Rust validity domain. It includes normal/empty/inactive/negative/zero/missing-row cases for every export, nonadmin and primitive/null arguments, floating/radix/whitespace edges, exact 20dp band thresholds, signed zero, source failures, Promise versus immediate behavior, rejected asynchronous reads, channel flips, option portfolio unwind and settlement counting. Every V row compares the complete result/error, all final argument values, exact ordered dependency trace, deferred rejections and newly created global properties. No returned valid flag is treated as permission.

The 11 columns are `V|Q`, name, method, tagged input-arguments JSON hex, backend-overrides JSON hex, `ok|throw`, tagged result/error JSON hex, tagged final-arguments JSON hex, ordered dependency trace JSON hex, deferred-rejections JSON hex and global-writes JSON hex. Header rows supply explicit default snapshots and which reads are asynchronous. Finite numbers use exact 16-digit binary64 bits; NaN/infinities are named, negative zero is distinct, Promise and BigNumber are tagged. Q rows preserve genuine source results and assert the Rust typed boundary separately; they are not counted as exact parity.

Local limits are at most eight arguments, 8,192 value nodes, depth24, 1 MiB aggregate value text, 1,024 entries per array/object and 1,024 dependency calls, with a 4 MiB cumulative dependency diagnostic budget. The JSON converter also inherits the existing JsonValue parser's 65,536-byte, 4,096-node, depth16 limits. Values are safe plain data; no symbols, functions, proxies, accessors or unpaired UTF16 surrogates are representable. Mutable array-as-params and Promise/BigNumber-as-params are explicitly rejected. Scalar params retain non-strict source writes/throws.

BigNumber math uses exact Decimal operations, source 20dp division and explicit source rounding; actual Number bridges remain binary64. Decimal operands are at most512 text bytes and obey the shared Decimal work limits. Nondecimal prefixed BigNumber strings are an explicit `UnsupportedNumericDomain`, retained by a genuine Q observation. Number(string) hexadecimal/binary/octal coercions are distinct and supported. Own `toString`/`valueOf` inputs or dependency rows are rejected by validity-specific preflight, with their genuine source coercion throw retained as a Q observation; the shared SourceValue representation remains usable by Types. These are local bounds, not historical consensus rules. Unsupported numeric work returns an error rather than a fake invalid/valid source verdict.

## Running and integration

In the staging layout:

```text
node work/full-validity-port/scripts/validity_vectors.cjs --repo outputs/TradeLayer-Rust --write
node work/full-validity-port/scripts/validity_vectors.cjs --repo outputs/TradeLayer-Rust --check
```

In the integrated repository, `node scripts/validity_vectors.cjs --check` uses the unchanged dependency archives in that repository. `require(...)` exports `environment`, `tag`, `defaults` and `asyncTargets` for the full Types oracle; importing the generator does not regenerate files. `environment` also exposes deferred exceptions and `globalWrites()` from the actual source VM.

The module is exported and integrated with the actual-owner backend and sequential consensus runner. The unchanged-source oracle runs on Node v24.19.0; Rust differential and bounded-domain tests run through Cargo with the pinned Rust1.90.0 toolchain. The source archive, fixture and exact domain restrictions remain reviewable independently of economic authority.

Future integration must supply actual healthy immutable facts and explicitly report unavailable dependencies. Accepted property/procedural metadata, country/clearlist and derived membership, parent-UTXO/script/signing evidence, historical channel revisions and external proof/capability results cannot be inferred from a caller's params or this transcript. Reference-price source bugs (latest/oracle and bootstrap true) are retained diagnostics, never substituted for authenticated funded price evidence. Actual block mutation, conservation, rollback and replay remain with the existing sole-owner runtime.
