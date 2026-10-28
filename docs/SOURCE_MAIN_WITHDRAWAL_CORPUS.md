# Original Main withdrawal failure corpus

Eighteen frames execute twelve original Main calls and six independent resets, with no unavailable-call exclusions. Indexed and live routes each cover successful withdrawal queue persistence, queue factory failure and queue update failure, followed by the exact original processed retry.

The unchanged source keeps `params.valid=true` when the un-awaited queue save rejects. Its actual pending row remains in memory, and the rejection is observed after Main's awaited work. The fixture retains all ordered database effects, registry/cache/receiver state, data-only input records, actual postphases and retry journals. No validator or financial result is supplied. Factory faults use the existing explicit one-shot mock storage failure convention.

Both genuine BigNumber 9.3.1 and 9.1.2 reproduce fixture SHA-256 `e2d56d9850fb5b396e43016962a5fc2e6a20200b08213ce3c1d212306e7da43d`. Rust 1.90 formatting, test metadata and test Clippy pass. Fresh runtime execution is a separate Root gate and is not claimed here. The test requires both pending threshold reads and the pending withdrawal save to be drained at each Main boundary, then verifies full replay and canonical checkpoint re-encoding.
