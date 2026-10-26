# Exhausted joint-chain rollback history

`JointChainState` retains a bounded deque of complete pre-block registry/ledger/spot states. After older witnesses are evicted, rolling back every remaining witness can leave a valid current state **above the original anchor with no prior history**. Previously the writer serialized that state as TLJC v1, while the v1 reader correctly rejected `tip > anchor && history_count == 0`. Consequently the valid oldest rollback point could not be reopened or committed through the durable joint-chain owner's round-trip validation.

TLJC v2 represents precisely that otherwise unrepresentable state. It does not remove the v1 rejection, reset the original deployment anchor, manufacture a parent header, or synthesize a previous economic state.

## Canonical version selection

| Current state | Output |
| --- | --- |
| Current tip equals original anchor, with no retained history | Original TLJC v1 |
| At least one complete retained pre-block witness | Original TLJC v1 |
| No retained witnesses and current tip height strictly above original anchor | TLJC v2 with explicit rollback floor equal to current tip |

Every previously representable v1 state keeps its original byte layout and encoding. The v1 decoder keeps its original validation, including rejection of an above-anchor tip without a prior witness. Existing v1 decoders reject the new version instead of silently adopting a different rollback policy.

## V2 framing and invariants

The common prefix retains network tag, genesis, genesis-derived chain ID, independent original anchor, current tip, configured history limit and history count. Immediately after the count, v2 adds one 40-byte `Tip`: eight big-endian height bytes plus the 32-byte block identifier. The complete current TLJP state and outer CRC follow.

V2 is accepted only when all of these conditions hold:

1. The deployment-supplied network, original anchor, administrator, permitted spot properties and history limit match, using the existing nested state/profile checks.
2. History count is exactly zero, current tip height is strictly greater than original anchor height, and encoded floor equals current tip in **both height and identifier**.
3. The complete nested joint state is canonical and compatible with the current finalized height. All registry, sole-ledger, spot, replay-marker, reserve and supply invariants still apply.
4. There are no extra bytes, the checksum/size checks pass, and re-encoding selects the identical version and bytes.

V2 cannot represent an anchor-only state, a state with retained witnesses, a different floor, or a looser network/height policy. Re-labeling a v1 state as v2 rejects; removing the floor and re-labeling a floor state as v1 still rejects. CRC repair does not bypass those checks. The overall checkpoint size limit is unchanged.

`rollback_floor()` exposes the oldest complete retained witness when one exists; otherwise it returns the current tip. It is an inspection accessor, not a new trust anchor. At a restored v2 floor, another rollback returns `EmptyRollbackHistory` without mutation. Applying a new valid local block creates a real pre-block witness and returns to canonical v1 output. Rolling that extension back returns to the identical canonical v2 floor.

## Persistence and authority boundary

The durable joint-chain owner continues to restore through `JointChainState::from_checkpoint_bytes`, so it can now persist this valid endpoint without bypassing validation. Its header index must not invent a missing parent header: a fully exhausted floor may have no retained header/index entry. The original independently supplied deployment anchor stays unchanged through write, reopen, extension and rollback.

All these chain inputs and checkpoints remain **unverified caller assertions**. The explicit floor proves neither ancestry to the anchor nor which branch is active. A CRC detects accidental corruption; it does not authenticate a header, protect against malicious snapshot substitution, authorize a sender or permit an economic payout. The external txindex/UTXO/script-validation lane is unaffected.

## Regression evidence

`tests/joint_history_floor.rs` exercises the actual complete joint state and codec:

- Evict history, roll back to its oldest retained state, encode/reopen v2 and retain exact registry activation, token balances, original anchor and replay markers independently on Bitcoin, Litecoin and Dogecoin profiles.
- Confirm the original v1 encoding against a frozen reference layout, including its unchanged above-anchor/empty-history rejection.
- Extend a restored floor, round-trip ordinary v1, then roll back to byte-identical v2 without resurrecting orphaned replay IDs.
- Reject altered floor height/hash, nonzero count, wrong version/network/genesis/chain/anchor/admin/permitted-policy/history limit, state-height mismatch, excessive nested length, missing floor, trailing data, every truncated prefix and version-malleated representations, including recomputed outer CRC cases.
- Write and load an immutable floor checkpoint through the actual filesystem API, preserving the independently supplied original anchor.

The tests establish local round-trip/rollback behavior, not authenticated historical reorganization acceptance. No v1 state-validation guard is weakened.
