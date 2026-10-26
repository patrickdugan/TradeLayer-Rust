# Core RPC client

`client.rs` provides a concrete synchronous HTTP JSON-RPC transport for a
configured Bitcoin, Litecoin or Dogecoin Core node. It implements all 21 active
RPC wrapper routes from pinned `client.js`, including the distinct `listUnspent`
and `listunspent` methods. The archived source is unchanged at
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, SHA-256
`dc8cf76301825e865a92d7b0e6028ccbe06c600a79e1e5cdcf59ac366cc6212e`.

The application supplies a socket address, Basic credentials or a cookie file,
optional wallet name, a nonzero timeout no greater than five minutes, and the
independently expected TradeLayer mainnet. `RpcClient::from_env` handles literal
IPv4/IPv6 and localhost from an explicit environment snapshot. An application
with a bounded DNS resolver can pass its result to `new`; this module does not
perform an unbounded hostname lookup. It does not search for `.env` files.
No connection or wallet call occurs during construction.

Each call opens one connection and sends one JSON-RPC 1.0 request. Failed calls
are never retried, including ambiguous write failures. Callers must reconcile
the result of a potentially completed wallet mutation before retrying it. There
is no connection pool, hidden daemon, background discovery or automatic wallet
operation. This is HTTP for a local node or an independently secured transport;
TLS is not implemented in this dependency-free crate.

The request includes Basic authentication and an encoded wallet path only for
the source's seven wallet-scoped wrapper routes. Cookie bytes are reread per
call, so normal Core cookie rotation takes effect. Credential Debug output is
redacted, response Debug shows only its byte count, and error Display omits the
remote message. Explicit error fields remain available to the application.
Wallet names are percent-encoded as a single path segment; the source simply
interpolates them. `getRawTransaction` preserves the source's default verbose
true and its ignored third block-hash argument. Generic `call` lets an adapter
explicitly request other Core parameters without changing that compatibility
method.

Socket connect, write and read use timeouts, and reads share an absolute call
deadline. HTTP headers are capped at 16 KiB and 64 entries. Requests are capped
at 1 MiB, 4096 parameters and 96 method bytes; responses are capped at 16 MiB.
The parser accepts explicit Content-Length or chunked transfer framing, bounds
chunk overhead and trailers, and rejects duplicate/conflicting framing,
compressed bodies, truncated replies and extra buffered bytes. HTTP errors,
Core JSON-RPC errors, response-ID mismatches and transport/deadline failures
remain distinct. Results require a matching numeric request ID. Duplicate JSON
keys, malformed UTF-8/JSON and unpaired UTF-16 escapes reject.

`RpcValue` preserves validated raw JSON rather than converting all numbers to
binary64 or using the issuance store's smaller metadata budget. Thus a raw block
hex string larger than 64 KiB and `18446744073709551615` survive unchanged. JSON
has a 64-level depth limit and 524288-value budget. `field`, `as_array`,
`as_string`, `as_u64` and `as_json` support caller-specific response adapters.
These are resource limits of this implementation, not historical protocol
acceptance rules.

`verify_network` queries network information, mainnet identity and block zero.
All three must match the independently expected family/mainnet/genesis. A
mismatch returns an error without changing the configured family or endpoint.
Readiness accepts real Core's Satoshi and Shibetoshi user agents; the separately
exposed `determine_chain_from_subversion` preserves the source's literal
litecoin/bitcoin/dogecoin search order and failure on those defaults. See
[Bitcoin Core network information](https://bitcoincore.org/en/doc/25.0.0/rpc/network/getnetworkinfo/)
and [Dogecoin Core's release discussion](https://github.com/dogecoin/dogecoin/discussions/3054).
These responses remain claims of the configured node. Readiness is not PoW,
script, signature, transaction inclusion or TradeLayer authorization verification.

## Source method map and repairs

| Source member | Rust implementation / disposition |
| --- | --- |
| `loadDotenvFromKnownLocations` | Explicit environment snapshot; implicit filesystem search is not performed |
| `getRpcBootstrapFromEnv`, `getEnvBootstrap` | `RpcBootstrap::from_env`, `RpcAuth::from_env`; source valid defaults/precedence, invalid socket numbers rejected |
| constructor, `_createClientByChain`, `getInstance` | Owned `RpcClient::new` / `from_env`; a single explicitly configured node, without a process-global singleton |
| `init`, `waitForInitialization` | Explicit synchronous `verify_network`; source never clears `isInitializing`, including successful locked initialization |
| `determineChainFromSubversion` | `determine_chain_from_subversion`, exact literal family priority |
| `getWalletPath` | Source wallet-env precedence; wallet-scoped HTTP uses an encoded path segment |
| `rpcCall` | `call(method, params, wallet_scoped)` |
| `getBlockchainInfo`, `getRawTransaction`, `getnewaddress`, `sendtoaddress`, `getNetworkInfo`, `getTransaction`, `getBlock`, `getBlockHash`, `createRawTransaction`, `listUnspent`, `decoderawtransaction`, `signrawtransactionwithwallet`, `dumpprivkey`, `sendrawtransaction`, `validateAddress`, `getBlockCount`, `loadWallet`, `listlabels`, `getaddressesbylabel`, `getwalletinfo`, `listunspent` | Each has its own `SourceRpcMethod` variant with source argument and wallet routing |
| `verifyClientChain` | Runtime `verify_network` rejects a mismatch; it does not silently rewrite the expected chain |
| `getChain`, `getTest` | Runtime readiness checks family and requires mainnet; generic calls expose the underlying response for inspection |

The source initializer defaults to `LTCTEST`, creates the fallback Litecoin
client even for that literal, probes and may change ports, uses two different
Dogecoin test ports, never clears its initialization flag, and only recognizes
literal chain names in subversion. These facts are characterized rather than
copied into node readiness. Testnet deployment, automatic discovery/redirect,
production TLS, transport pooling and daemon/listener wiring remain open.

`scripts/client_vectors.cjs` executes the complete unchanged source with a
captured library RPC boundary. Its 68 observations cover bootstrap/defaults,
wallet precedence, every route, ignored/default arguments, chain matching and
the initialization flag defect. Rust consumes the same corpus. Socket tests use
temporary loopback mock servers for real HTTP requests, large block replies,
chunking, truncation, framing/JSON adversaries, cookie rotation, deadline expiry,
remote errors without retry and all three mainnet genesis expectations. They do
not call a real wallet or prove interoperability with an installed Core daemon.
