# Complete supported Bitcoin transaction relation

`bitcoin_supported_transaction` closes the gap between checking one selected input and checking **every input of one exact block-selected transaction**, within the existing six-template Script profile. It adds no chain/UTXO/TradeLayer admission capability, economic policy, state owner, default behavior or checkpoint format.

The explicit profile is `TL-BITCOIN-SUPPORTED-TRANSACTION-RELATION-13-1`. Only the declared `TradeLayerMainnet::Bitcoin` namespace and the existing exact `BITCOIN_STANDARD_INPUT_FLAGS` mask (`0x0e15`) are accepted. A matching namespace/header/context is still a supplied expectation; it does not prove actual network membership or deployment activation. Litecoin/Dogecoin and every other flag mask refuse without reinterpretation.

`check_bitcoin_supported_transaction` first reuses the complete BIP141/TXID relation over the actual block bytes, selects its spending transaction by actual position, and runs the bounded literal context-free Bitcoin transaction rules. It derives every input's funding TXID/vout from those spending bytes. External funding is an exact collection of complete raw transactions: missing/extra/duplicate TXIDs or malformed evidence fail. One external funding transaction may supply several distinct vouts. A funding transaction present in this same block is selected from the actual block bytes, must precede the spending position, cannot be the coinbase, and must not also be supplied as external evidence.

All selected funding values and aggregate inputs/outputs must be in Bitcoin MoneyRange, and inputs must cover outputs. This complete evidence/value preflight occurs before input signature checks. It then invokes the existing native checker for every input in vin order. A bad or unsupported later input fails the entire construction; no complete receipt can be constructed from a successful prefix. The six supported templates and all six defined ECDSA hash modes are inherited unchanged, including weak modes and the real legacy SINGLE bug. A genuine Core-valid arbitrary OP_TRUE input remains explicitly outside this profile.

The private-construction receipt borrows the same block/spending/funding bytes, retains complete ordered input satisfactions and actual funding origins, and exposes checked raw input/output totals and a **conditional** fee. No caller-supplied amount, signer list, signature verdict, parsed operation, financial state or UTXO root is accepted. Raw funding transactions can still be fabricated or already spent; the conditional fee is not a proven active-chain miner fee. An earlier same-block producer's own inputs/validity are not validated by this selected-transaction relation.

The receipt permanently refuses:

- authenticated funding inclusion/current parent unspentness;
- full transaction and chain consensus;
- TradeLayer actor/economic admission.

It is not a `TrustedChainAdapter` or `TrustedProtocolAdapter`, is not connected to the follower or economic execution, and does not authenticate a caller-selected checkpoint/root. It does not establish external coinbase maturity, sequence locks/finality, complete block subsidy/fees/sigops/version/deployment rules, producer validity, greatest-work active chain or TL operation/actor authorization. Existing block dependency checks remain separately useful for cross-transaction conflicts. Unknown/unsupported scripts are refusals, not negative verdicts on every Bitcoin-valid program.

## Literal Core rules and parser subset

`check_bitcoin_context_free_transaction` follows the rule order from unchanged Bitcoin Core `44d8b13c81e5276eb610c99f227a4d090cc532f6` [`src/consensus/tx_check.cpp`](https://github.com/bitcoin/bitcoin/blob/44d8b13c81e5276eb610c99f227a4d090cc532f6/src/consensus/tx_check.cpp), Git blob `f9496559093da1d757d5f3f531e3ffdb69364d76`: vin/vout nonempty, stripped size, negative/individual/aggregate output values, duplicate prevouts, coinbase script length and ordinary null prevouts. The existing canonical parser is intentionally stricter than all Core deserialization: nonempty/count/size/marker/CompactSize bounds can reject first. This API claims the admitted parser subset, not arbitrary Core serialized transaction parity. A successful return is the checked output sum, not a state capability.

The raw funding MoneyRange/aggregate/conservation portion follows the arithmetic in the same revision's [`src/consensus/tx_verify.cpp`](https://github.com/bitcoin/bitcoin/blob/44d8b13c81e5276eb610c99f227a4d090cc532f6/src/consensus/tx_verify.cpp), blob `154146f08d30833d1708ed32006ba1d7b788e7d2`. That does not implement its authenticated `CCoinsViewCache` availability or height rules. They stay unproven. No UTXO hash equality is promoted to that missing authority.

## Actual evidence

The deterministic corpus contains 67 whole-transaction scenes: 48 construct the relation and 19 refuse. It covers all 36 template/hash-mode combinations, mixed three-input shapes, nonzero funding vouts, weak modes/SINGLE missing output, zero/MAX_MONEY, duplicate/forward/same-block coinbase references, two vouts from one funding transaction, missing/extra/duplicate funding, wrong TXID/vout, invalid later signature and genuine Core-valid unsupported later Script, negative/overlarge/aggregate outputs, invalid/aggregate funding amounts and overspend. An invalid first signature plus a later invalid funding amount confirms complete value preflight precedes Script. API checks cover wrong network/flags/header/parent/position, malformed evidence, >100,000 funding references and >16 MiB valid raw funding bytes.

Independent reference execution:

- **133 selected Script calls** use the previously frozen genuine libbitcoinconsensus executable SHA256 `6e1b7c303c8986dabcda4968456fedce388be288a82873ba0231e10d8ea9efa4`. Every native complete success has all its actual inputs Core-valid.
- **72 context-free calls** execute the actual unchanged pinned `CheckTransaction` C++ body, compiled with the already verified Core/secp libraries. The harness only supplies exact byte-stream input and reports the actual boolean/reason; it does not substitute transaction checks. All native admitted cases match its literal outcomes/reasons, including coinbase script lengths 1/2/100/101 and ordinary null prevouts. The work-only executable SHA256 is `c38edd026b190b526eb440458b17eb9e5e47a3af4a4b9dd1eabd9db2f2b76828`; the source, compiler invocation, library hashes and MIT notice are retained.
- Four new standalone Rust1.90 tests plus two unchanged SHA support tests pass in debug/release; production/test Clippy denies all warnings and Clippy lints, and formatting passes. The actual immutable 8bcc provider supplies existing native APIs; those source files byte-match current Root. No whole-Core Cargo or Root edit was performed.
- Removing last-input coverage, conservation or the forward-producer check makes the full corpus test fail. The original first aggregate-counter typo (47 observed versus 48 expected across the earlier 63 scenes) is retained in its failure log; every per-case result had matched. Four meaningful scenes, including zero-value acceptance, expanded the final corpus to 67/48. No runtime behavior was changed to fit that counter.

No real block, node, wallet, private key, mempool, chainstate or funds were used. Signatures use public throwaway scalars; synthetic blocks have consistent commitments but no PoW/active-chain claim. A genuine Script/CheckTransaction outcome is not a full-Core block/UTXO acceptance result.

## Reproduction / integration

The portable Python producer takes explicit paths to both pinned offline reference executables and regenerates all five fixture files before byte comparison:

```text
python scripts/generate_bitcoin_supported_transaction.py --core-script-reference PATH_TO_PINNED_SCRIPT_EXE --core-basic-reference PATH_TO_PINNED_BASIC_EXE --check
```

The producer depends only on Python's standard library and the checked-in deterministic throwaway signing helper. It has no sibling work-directory or repository-output dependency. Reference executables/libraries are test-only, not production dependencies. A separately rebuilt oracle needs separately recorded source/build/executable provenance before changing its pinned expected executable hash.

Root should apply only `prepare_lib`, copy the new module, four-test integration file, fixtures, two Python producer files and this document, then run actual current-crate tests/Clippy/format and the existing selected-input/block-binding gates. The candidate's standalone result is not that pending actual-Core gate. Existing selected-input code, flags, default V2, old policies/checkpoints and PR137/138 files remain unchanged.

## Actual Root integration

All four new public tests pass in the actual current crate, with14 existing input/witness controls. Root independently regenerated all five fixtures using the pinned genuine reference executables:67 scenes,133 Script calls and72 unchanged Core CheckTransaction calls. External funding remains claimed raw bytes; all parent-UTXO/chain/TL authority methods still refuse. No follower, financial execution or external parent-snapshot files changed. The combined library passes511 tests (four child entries ignored); all-target Clippy denies warnings; Cargo fmt,285 included-file formatting, inventory and supply-chain checks pass. Counts overlap. Full all-target baseline remains2,142Rust/239numeric; this later batch is scoped. Verification: `fixtures/reference_fidelity/bitcoin-supported-transaction-core-integration.json`.
