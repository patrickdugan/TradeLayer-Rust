# Verified actual-file historical metadata views

The additive Journal reader derives a requested historical KV view from the complete actual CRC/batch log while the current owner retains its exclusive file lock. It checks the current disk suffix against the healthy owner, restores the original cursor, compares the independent historical complete-KV receipt, and never appends or repairs. A wrong expected pin refuses without poisoning a healthy owner; observed disk/current divergence poisons access while retaining the lock.

CompactingJournal supports historical physical/logical mapping only within its verified current publication. SourceCompactIndex captures complete metadata and row receipts, cold-opens an independently expected actual post owner, and audits the historical Directory and ordered row stream. Old compacted/pruned generations are explicitly unavailable. An in-memory lease is not durable generation escrow.

Eight actual-Core controls cover cold drop/reopen, empty sequence0, restored non-EOF cursor, wrong pins, valid-CRC rewritten history, malformed batch/CRC/footer/partial frame, held views across later writes/compaction, unavailable publication and retained-lock corruption refusal. Combined library354, all-target Clippy, Cargo fmt,191 included-format files, inventory and supply-chain checks pass. Evidence: fixtures/reference_fidelity/native-main-wal-history-core-integration.json.

This milestone is metadata-only. Cold financial/source replay, catchup/service scheduling, cross-publication durable retention, actual killed-process and host power-loss exercises remain separate gates. Integrity receipts do not authenticate chain, UTXO or actor inputs.
