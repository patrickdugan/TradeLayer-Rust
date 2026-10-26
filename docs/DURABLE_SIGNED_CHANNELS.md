# Durable co-signed channel owner

`DurableSignedChannelChain` stores the complete opt-in `UnifiedChainState` as one
synced Journal value in the existing network-specific `channels` namespace. The
Ledger inside that owner remains the only spendable balance state. Expected
network, anchor, administrator, history limit and the full policy table are
provided independently when creating or restoring an owner.

Every mutation compares an exact `(sequence, tip, checkpoint commitment)` head
before calling an adapter or verifying candidate signatures. Rollback publishes a
new Journal sequence even if a subsequent replay restores the same tip and
checkpoint bytes. The wrapper publishes its cached owner only after sync; an
uncertain write poisons all cached state and head reads. Recovery requires closing
the poisoned owner, inspecting the actual durable journal, and independently
pinning the recovered head. Debug output also respects that health fence.

Candidate counts and signed-input aggregate bytes are bounded before cloning the
owner. Actual fixed-pool custody, policy pinning, both signatures, successor heads,
enrollment replay IDs, retained-history checks and full checkpoint validation are
enforced by the enclosed Ledger/Protocol/Unified owner implementation. No mutable
Ledger accessor or unvalidated callback is exposed by this wrapper.

The integration tests use genuine two-key signatures and real disk journals under
synthetic block headers on Bitcoin, Litecoin and Dogecoin profiles. Their adapter
does not authenticate L1 transactions, actors, scripts or funding outputs. Local
supersession makes an old revision non-current in this owner; its old signatures
remain mathematically valid. This is the explicitly repaired
`SupersedingCoSignedAllocationV1` policy, not a historical channels.js wire format
or proof of L1 commitment revocation. The separate parent-UTXO/script gate remains
closed.

## Integrated repository validation

The parent integrated this code and ran the actual pinned Rust1.90 repository:
1,199 top-level tests plus one subprocess check pass across148 executable
targets. Formatting (including included implementation files), all-target frozen
Clippy with warnings denied, all11 supply-chain mutation tests and the all-target
frozen release build pass. The final run held553 implementation/check inputs
unchanged. The source corpus has445 exact observations and46 explicit domain
cases; new-policy references cover81 transcript vectors and12 real-bootstrap
signed envelopes. See [ROADMAP_PROGRESS.md](ROADMAP_PROGRESS.md) for scope and
remaining gates. These checks grant no L1 admission or revocation authority.
