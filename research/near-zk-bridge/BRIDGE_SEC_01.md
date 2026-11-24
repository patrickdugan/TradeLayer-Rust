# BRIDGE-SEC-01: CYBER lockbox drain, source-event authorization

Registered 2026-11-24. Isolated synthetic regression for the ZTB bridge, no real funds.

## Evidence and limits

- [The Block, 2026-11-21](https://www.theblock.co/news/defi/2026-11-21-cyber-investigating-suspected-1-million-bridge-exploit-as-upbit-bithumb-freeze-transfers-444810): unexpected Solana mint, 5,213,000 CYBER released on Ethereum, approximately 575 ETH in sale proceeds, Upbit/Bithumb transfer suspensions.
- [Bitquery investigation](https://www.bitquery.io/investigations/cyber-bridge-hack), checked 2026-11-22 10:00 UTC: 5,213,905 starting lockbox units; source had emitted 329 messages, destination accepted absent messages 330/331, and another absent message 332 was approved after a verifier key change. One signer sufficed on that route. Signatures support the key-control inference; the cause of compromise and operator identity remain unknown. Message 332 was not yet executed at the reported cutoff.
- These values calibrate a **fictional integer-unit accounting replay**; this does not replay CYBER transactions, reconstruct the real token supply, validate signatures, or estimate market impact.

## Failure and acceptance

Existing ZTB-01 model.mjs checks D >= M >= B >= R and D-R >= outstanding + pending but trusts the caller's deposit input. If the caller manufactures a deposit, D grows without an actual increase in reserves. An isolated relay-only negative control feeds a phantom deposit through the unchanged accounting model and models payout from independent real reserves. Internal arithmetic passes with 5,213,905 reported reserve units while external reserves have fallen to 905 and honest liabilities remain 5,213,905. These are model values, not claims about real CYBER liabilities.

A separate guard matches each proposed deposit against an independent immutable fixture of finalized source events, whose contents cannot be set by the relay. It rejects both phantom-event submissions even after simulated key rotation. It binds the owner, amount, asset, vault, chain, route and root; replay uses a stable canonical deposit ID independent of signer epoch. An independent pause rejects further admissions. A genuine fixture deposit remains usable.

Run:

    node research/near-zk-bridge/bridge-sec-01.mjs
    node research/near-zk-bridge/test.mjs

The regression is passing only when all checks complete; the negative-control depletion is expected. The relay identifiers are abstract labels, not cryptographic signatures; the source table is an oracle fixture, not a proof verifier or a live finality implementation. No claim about production authorization follows.

## Mandatory follow-through for ZTB-02/04/08/09/11

The real adapter must derive deposit identity from a chain-specific finalized event locator and bind source/destination genesis, route, vault/token identity, amount, recipient, event identity, finality anchor, program/VK and predecessor/state commitment into the statement. The finalized anchor must be independently authenticated under pinned consensus rules; a signer, prover, RPC response, or admin-supplied root is insufficient. Consume the event once even across relayer, verifier, config and key epochs. A verifier upgrade must not silently reset consumed IDs or allow an arbitrary new trust root. Prove canonical source burn plus authenticated payout, callback and payout uniqueness independently. Reorg, restart, pending payouts, wrong-route proofs, alternate encodings, stale checkpoints, compromised administrator and emergency recovery need dedicated adversarial vectors. Pause may stop movement, never bypass proof validation. Keep reserve/solvency accounting independent of any claimed source credit.

Proof statuses remain false until native NEAR and Bitcoin finality, real proof verification and authorization are independently established. This file does not modify the production or research custody API.

Companion agent diagnostic: SystemicRiskBench content case content_cyber_bridge_2026 (separate from frozen historical baseline).
