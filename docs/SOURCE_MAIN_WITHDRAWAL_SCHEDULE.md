# Main raw21 withdrawal save

The strict upper corpus advances through funded type 19 and type 20, then exposes an eager queue write in type 21. Original `validateWithdrawal` awaits both channel lookups. The later `Logic.withdrawal` is synchronous: it starts `Channels.getChannel` and `Channels.addToWithdrawalQueue` without awaiting either. The latter appends the real pending row and starts the withdrawal DB factory. Main resumes and starts its processed-journal consensus factory before the withdrawal update runs.

Exact V9 Main now retains that row and the actual factory outcomes in a bounded save continuation. It reuses the existing SourceChannels owner, `begin_withdrawal`, Core reads/update, error handling and un-awaited rejection diagnostic. The actual consensus factory resumes the save; Main leave drains any remainder. A queue factory or update rejection retains the original pending row and does not become a synchronous Logic exception. No new balance or channel owner is introduced.

The pending row, loading and saving factory outcomes are explicit in V9's `pendingWithdrawalSave` diagnostic, null at accepted Main endpoints. V1–V8 and standalone paths retain their prior behavior and bytes. The original upper source observations, including complete ordered effects, are unchanged.

Work-stage Rust 1.90 formatting passes. Four isolated tests compile the exact continuation with the actual SourceChannels owner and pass for row-before-write ordering, deferred factory rejection, deferred update rejection and final drain. Their small DB adapter uses only the declared fixture keys; they are not a fresh full runtime build. The immutable linked public dependency library hash is `61bcc348b7cab95b2772d4d585b3bafe684326b0e3c2fea646db5565959a9841`. Root owns Cargo source parity and version gates.

Mount `raw21/src/source_withdrawal_schedule_impl.rs` as `src/source_withdrawal_schedule_impl.rs`, this document under `docs/`, and apply `raw21_prepare.py` transforms to fresh runtime/channel-logic/spot-schedule files. Its upper-test transform additionally asserts null at every observed Main endpoint. Do not overwrite Root's evolving transfer helpers or previews.
