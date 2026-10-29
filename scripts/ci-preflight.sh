#!/usr/bin/env bash
# Match the first Rust CI gates before publishing an intermediate commit.
set -euo pipefail
cd "$(dirname "$0")/.."

case "${1:---full}" in
  --fast|--full) mode="${1:---full}" ;;
  *) echo "Usage: bash scripts/ci-preflight.sh [--fast|--full]" >&2; exit 2 ;;
esac
if (( $# > 1 )); then
  echo "Only one option is supported" >&2
  exit 2
fi

# Cargo fmt omits files brought in through include!; check them independently.
cargo fmt --all -- --check
rustfmt --check --edition 2021 \
  src/registry_snapshot_impl.rs src/execution_snapshot_impl.rs \
  src/ledger_channels_impl.rs src/ledger_settlement_impl.rs \
  src/ledger_channel_revisions_impl.rs src/execution_channel_revisions_impl.rs \
  src/ledger_native_impl.rs src/spot_native_impl.rs src/joint_native_impl.rs src/joint_chain_native_impl.rs \
  src/spot_snapshot_impl.rs src/joint_snapshot_impl.rs \
  src/trade_history_position_impl.rs src/volume_index_source_impl.rs \
  src/source_contract_accounting_impl.rs src/source_contract_aux_impl.rs src/source_contract_fee_impl.rs src/source_contract_oracle_impl.rs \
  src/economic_block_stage_impl.rs src/economic_consensus_impl.rs \
  src/economic_source_catalog_impl.rs src/economic_source_catalog_snapshot_impl.rs \
  src/economic_source_send_impl.rs src/economic_source_oracle_impl.rs \
  src/economic_native_impl.rs src/economic_insurance_impl.rs src/economic_insurance_snapshot_impl.rs \
  src/joint_economic_impl.rs src/economic_snapshot_impl.rs src/economic_funding_impl.rs src/economic_mark_impl.rs src/economic_contract_impl.rs \
  src/economic_liquidation_impl.rs src/economic_liquidation_replay_impl.rs src/contract_orderbook_liquidation_impl.rs src/economic_amm_impl.rs src/economic_amm_replay_impl.rs \
  src/iou_snapshot_impl.rs src/iou_ledger_snapshot_impl.rs \
  src/source_clearing_funding_impl.rs src/source_insurance_lifecycle_impl.rs src/source_insurance_legacy_v1_impl.rs src/source_iou_settlement_impl.rs src/source_clearing_liquidation_impl.rs src/source_clearing_expiry_impl.rs src/source_clearing_global_impl.rs src/source_clearing_mark_settlement_impl.rs src/source_clearing_supply_impl.rs src/source_contract_history_schedule_impl.rs src/source_contract_registry_lifecycle_impl.rs src/source_channel_financial_impl.rs src/contract_channel_queue_source_impl.rs src/source_channel_logic_impl.rs src/source_transition_witness_impl.rs src/source_contract_position_identity_impl.rs src/source_channel_volume_impl.rs src/source_option_trade_impl.rs src/source_synthetic_lifecycle_impl.rs src/source_derivative_exercise_impl.rs src/source_distribution_lifecycle_impl.rs src/margin_option_index_source_impl.rs \
  src/clearing_distress_impl.rs \
  src/clearing_partial_impl.rs \
  src/clearing_snapshot_impl.rs \
  src/rpc_http_impl.rs \
  src/rpc_json_impl.rs \
  src/source_lifecycle_transition_witness_impl.rs \
  src/source_liquidity_rewards_impl.rs \
  src/source_lower_clearlist_impl.rs \
  src/source_lower_oracle_lifecycle_impl.rs \
  src/source_lower_property_lifecycle_impl.rs \
  src/source_lower_send_commit_impl.rs \
  src/source_lower_utxo_trade_impl.rs \
  src/source_main_callable_impl.rs \
  src/source_main_production_impl.rs \
  src/source_main_service_impl.rs \
  src/source_main_snapshot_impl.rs \
  src/source_main_startup_impl.rs \
  src/source_main_file_io_impl.rs \
  src/source_rich_runtime_impl.rs src/source_rich_replay_impl.rs src/source_rich_owner_prefix_impl.rs \
  src/storage_read_snapshot_impl.rs \
  src/storage_verified_prefix_impl.rs src/compact_verified_prefix_impl.rs \
  src/source_compact_stream_impl.rs \
  src/source_main_index_stream_impl.rs \
  src/source_index_validity_stream_impl.rs \
  src/compact_native_main_owner_access_impl.rs \
  src/source_main_native_owner_access_impl.rs \
  src/source_main_native_validity_impl.rs \
  src/source_main_native_runtime_impl.rs \
  src/source_main_native_catchup_impl.rs \
  src/source_main_native_checkpoint_impl.rs \
  src/source_main_native_row_continuation_impl.rs src/source_main_native_row_session_impl.rs src/source_index_row_binding_impl.rs \
  src/source_main_native_catchup_prepare_impl.rs src/source_main_native_catchup_continuation_impl.rs \
  src/source_native_append_plan_impl.rs \
  src/source_index_prepared_put_impl.rs src/source_index_prepared_validity_impl.rs \
  src/source_index_verified_reopen_impl.rs src/source_index_supersession_binding_impl.rs \
  src/compact_native_supersession_binding_impl.rs src/source_main_native_superseding_intent_impl.rs \
  src/source_main_native_step_checkpoint_impl.rs src/source_main_native_step_decode_impl.rs \
  src/source_main_native_step_session_impl.rs src/source_main_native_step_segment_impl.rs \
  src/source_main_step_block_impl.rs src/source_main_step_runtime_impl.rs src/source_main_step_fields_impl.rs \
  src/source_main_step_file_core_impl.rs src/source_main_step_file_block_impl.rs \
  src/source_main_step_file_runtime_impl.rs src/source_main_step_file_kernel_impl.rs \
  src/source_v8_sort_impl.rs \
  src/source_procedural_bitvm_impl.rs \
  src/source_proof_lifecycle_impl.rs \
  src/source_proof_settlement_impl.rs \
  src/source_recent_lifecycle_transition_witness_impl.rs \
  src/source_spot_lifecycle_impl.rs \
  src/source_spot_matching_impl.rs \
  src/source_spot_queue_fragments_impl.rs \
  src/source_spot_schedule_impl.rs \
  src/source_tail_lifecycle_impl.rs \
  src/source_transfer_schedule_impl.rs \
  src/source_token_channel_schedule_impl.rs \
  src/source_withdrawal_schedule_impl.rs \
  src/source_activation_lifecycle_impl.rs \
  src/source_activation_hash_impl.rs \
  src/source_amm_lifecycle_impl.rs \
  src/source_amm_quote_impl.rs \
  src/source_amm_remaining_impl.rs \
  src/source_durable_replay_impl.rs \
  src/source_durable_replay_tests.rs \
  src/litecoin_transparent_impl.rs \
  src/litecoin_mweb_structure_impl.rs \
  src/source_full_dispatch_transition_witness_impl.rs src/source_durable_transition_witness_impl.rs src/source_durable_transition_witness_tests.rs

cargo clippy --all-targets --locked -- -D warnings
if [[ "$mode" == "--full" ]]; then
  node scripts/check_main_production_parse_int.cjs
  cargo test --locked
fi
echo "Rust CI preflight passed ($mode). GitHub still runs network, Lean and Kani gates."
