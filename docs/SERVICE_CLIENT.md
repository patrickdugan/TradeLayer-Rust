# Local service clients

`service_client::ServiceClient<T: ServiceTransport>` implements both complete pinned client objects. It sends actual requests through the supplied transport; `HttpServiceClient` supplies the bounded TCP/HTTP implementation. The client never fills an unavailable backend result with invented balances, history, wallet ownership, or proof verification.

The reference is Git commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. Full unchanged LF archives are `fixtures/legacy_interface_express_source.txt` (SHA256 `8cfa990d9aa460d29f5d309beb0987a040e06e2840632fac38e713d94836aab2`) and `fixtures/legacy_wallet_interface_source.txt` (`d22fbe4ee631ca15fbb4fe4fb2af65c7a424ef4f249eb8686f8aa7b5fd2d8735`).

## API and source behavior

Choose `SourceClientKind::{InterfaceExpress,WalletInterface}` and `ClientRouting::{PinnedSource,RepairedServiceV1}` explicitly. `ClientMethod::ALL` contains the 22 unique names across both objects; `supported_by` preserves each object's exports. `prepare_request` exposes the exact bounded request for review. `call`/`call_by_name` bind it to the transport, and `into_transport` returns ownership. Arguments are ordered `JsonValue`s; missing positions remain JavaScript `undefined`. Extra arguments are ignored after resource validation.

| Active method | interfaceExpress source | walletInterface source |
|---|---|---|
| initMain | POST `/initMain`, `{test}`; returns undefined; swallows errors | POST `/tl_initmain`, `{test:true}`; returns Axios response; throws |
| listProperties | POST `/listProperties`, absent body | POST `/tl_listproperties`, absent body |
| getAllBalancesForAddress | POST `/getAllBalancesForAddress`, `{address}` | POST `/tl_getAllBalancesForAddress`, `{params:address}` (matches wallet listener) |
| getActivations | POST `/getActivations`, absent body | POST `/tl_getactivations`, absent body |
| getOrderBook | POST `/getOrderBook`, `{propertyId1,propertyId2}` | **ReferenceError** `propertyId1 is not defined`, before transport |
| getContractOrderBook | POST `/getContractOrderBook`, `{contractId}` | **ReferenceError** `contractId is not defined`, before transport |
| listContractSeries | POST `/listContractSeries`, absent body | POST `/tl_listcontractseries`, absent body |
| listOracles | POST `/listOracles`, absent body | POST `/tl_listoracles`, absent body |
| getContractPositionForAddressAndContractId | GET `/contractPosition/:address/:contractId` | GET `/tl_contractposition`, **no query**, despite destructuring params |
| getTradeHistory | GET `/tradeHistory/:propertyId1/:propertyId2` | GET `/tl_tradehistory`, **no query** |
| getContractTradeHistory | GET `/contractTradeHistory/:contractId` | GET `/tl_contracttradehistory`, **no query** |
| getFundingHistory | GET `/fundingHistory/:contractId` | GET `/tl_fundinghistory`, **no query** |
| getOracleHistory | GET `/oracleHistory/:oracleId` | GET `/tl_oraclehistory`, **no query** |
| getClearingHistory | GET `/clearingHistory/:contractId` | absent export |
| getWalletPositions | GET `/walletPositions`, no address | absent export |
| getAddressesWithProperty | GET `/addressesWithProperty/:propertyId` | absent export |
| getContractInfo | absent export | GET `/tl_getContractInfo`, query `{contractId}` |
| getInitialMargin | absent export | GET `/tl_getInitMargin`, query `{contractId,price}`; **Number(data)** |
| getColumn | absent export | GET `/tl_getChannelColumn`, query `{channel,addressA,addressB}` |
| getMaxProcessedHeight | absent export | POST `/tl_getMaxProcessedHeight`, `{}` |
| getTrackHeight | absent export | POST `/tl_getTrackHeight`, `{}` |
| checkSync | absent export | POST `/tl_checkSync`, `{}` |

The Express object has 17 declarations and 16 active exports: its later duplicate `getContractPositionForAddressAndContractId` replaces the earlier identical method. Wallet has 19 active exports. Source GET configuration fields outside `params` are deliberately omitted from query serialization. Destructuring missing/null params raises the source TypeError; primitive params have no corresponding named fields. Wallet book methods retain their source ReferenceErrors, even with a well-formed params object.

`ClientOutcome::Undefined {swallowed}` distinguishes the source init return value from its underlying swallowed failure. All other source methods throw transport/HTTP failures. Statuses outside 200–299 become `ClientFailure::Http` and retain response data. `ClientData` distinguishes JSON from text; JSON data is a validated raw `RpcValue`, preserving its lexemes rather than asserting arbitrary JavaScript object identity. Duplicate fields within the finite 64 KiB ordered-JSON domain are parsed with last-write semantics. Malformed text stays text. Wallet init returns the transport's status/content-type/body projection of an Axios response; Axios headers, socket and config objects are explicitly outside that projection.

`SourceNumber` preserves binary64 bits, including negative zero and infinities; NaN is canonicalized. `getInitialMargin` executes source Number coercion for null, booleans, finite/overflowed numeric lexemes, strings (including radix forms and JavaScript whitespace), arrays and ordinary objects. Non-callable own `toString` produces TypeError where JavaScript cannot obtain a primitive. This unvalidated historical result is never credited to custody or treated as a trusted margin amount.

## Explicit routing repair

`RepairedServiceV1` uses the actual local router contracts and remains separate from source observations. It fixes wallet order-book free-variable failures, uses the explicit repaired balance alias under `address` (the original wallet listener correctly uses `params`), supplies missing GET query parameters and exact canonical `/tl_` route spelling, and sends channel-column requests as POST `{channelAddress,newCommitAddress,cpAddress}`. Oracle-history routing explicitly maps the chosen source `oracleId` argument to the listener/backend's `contractId`; it does not infer an oracle-to-contract relation.

Express repair uses the corresponding canonical listener routes. Its clearing and property-address history paths retain path parameters. Repaired `getWalletPositions` requires an explicit nonempty address argument and sends `/walletPositions?address=...`; source mode retains the original missing-address request. Repaired wallet `initMain` accepts an optional boolean `test` argument and defaults to `false` for this mainnet read service. Invalid typed arguments fail before transport; explicit `true` remains subject to the server's testnet-unavailable gate. Pinned source mode still ignores arguments and sends `{test:true}`. Initialization only acknowledges the already-restored read service and does not start a daemon, create state or authorize funds. Repairs retain the chosen source object's return/error policy. The repaired `/tl_listContractSeries` call has no contractId and relies on the router's explicitly repaired list operation; source listener lookup with an omitted contractId is separately characterized by the router.

## Resource and transport boundary

At most 32 arguments and 1 MiB aggregate encoded argument bytes are accepted; each argument obeys the shared finite JSON 64 KiB / 4,096-node / depth-16 bounds. Validation precedes field clones. Query arguments support scalar JSON values, with null/undefined omitted as Axios does. Nested query objects/arrays require Axios's richer repeated/bracketed query representation and return `UnsupportedSourceQuery` here. Raw source URL interpolation containing slash, backslash, query, fragment, percent, controls or dot segments returns `UnsupportedSourcePath`; the oracle retains its original raw-URL observation. Ordinary Unicode and spaces are passed as decoded path text and encoded once by HTTP transport. Source mode first removes only trailing ASCII spaces from the entire URL, matching Node URL parsing; repaired routing preserves the explicit segment text. Repaired query arguments can safely contain reserved URL characters.

Requests use shared 8 KiB path/query and 1 MiB body bounds; absent POST data is represented by `body_present=false` and produces zero body bytes, distinct from JSON null or `{}`. Responses are bounded to 16 MiB; Number coercion additionally caps its input at 64 KiB and nested arrays at the shared JSON depth limit. The generic transport is checked for invalid status and oversized responses. Cancellation/deadlines are checked before preparation, before I/O and after transport return, including error return; caller control takes precedence over Express init swallowing. HTTP transport checks within bounded socket operations. There is no retry or global localhost endpoint assumption in the Rust client; the caller supplies the configured transport.

## Evidence

`node scripts/service_client_vectors.cjs --check` executes both full unchanged archives with an explicit Axios request/response/error adapter. Its 213 observations cover every active export with normal/missing/network/HTTP cases, duplicate declarations, ignored arguments, destructuring and free-variable failures, body/query contracts, missing query parameters, coercions, malformed response text, undefined/Number return values, and raw-URL/nested-query observations outside the typed client domain. Ten rows explicitly characterize those unsupported transport shapes; they are not counted as Rust source successes.

`--source-root PATH` checks the pinned checkout instead. `--axios-root PATH` additionally loads genuine installed Axios **1.20.0**, verifies its request JSON transformation and URI/query serialization against the adapter projection, and verifies its response JSON/text transformation for all calls. These checks ran successfully against the pinned local checkout's installed package. The explicit adapter run requires no npm dependency and makes no claim about live server readiness. Rust tests compare all 213 observations, exercise routing repairs and resource/control failures, and send both clients over real loopback `HttpServiceClient` sockets to check absent body bytes, single URL encoding, Number signed zero and the repaired balance request. Runtime authentication and unavailable backend capabilities remain owned by their actual adapters.
