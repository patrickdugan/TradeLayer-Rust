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
  src/source_main_native_no_index_impl.rs src/source_main_native_no_index_runtime_tests_impl.rs \
  src/source_main_native_supersession_controller_impl.rs src/source_main_native_supersession_runtime_impl.rs src/source_main_native_no_index_index_tests_impl.rs \
  src/source_native_append_plan_impl.rs \
  src/source_index_prepared_put_impl.rs src/source_index_prepared_validity_impl.rs \
  src/source_index_verified_reopen_impl.rs src/source_index_supersession_binding_impl.rs \
  src/compact_native_supersession_binding_impl.rs src/source_main_native_superseding_intent_impl.rs \
  src/source_main_native_prepared_prefix_ack_impl.rs \
  src/source_main_native_multiheight_tests.rs \
  src/source_main_native_prefix_ack_controller_impl.rs src/source_main_native_no_control_store_impl.rs src/source_main_native_control_store_tests_impl.rs \
  src/source_main_native_service_block_prelude_impl.rs src/source_main_native_service_block_impl.rs src/source_main_native_service_fields_impl.rs src/source_main_native_service_io_impl.rs src/source_main_native_service_runtime_impl.rs src/source_main_native_service_session_impl.rs src/source_main_native_service_tests.rs src/source_main_service_kernel_impl.rs src/source_main_native_service_block_tests.rs src/source_main_native_service_test_transport.rs src/source_main_native_completed_ack_binding_impl.rs src/source_main_native_completed_bounded_codec_impl.rs src/source_main_native_completed_bytes_impl.rs src/source_main_native_completed_chain_impl.rs src/source_main_native_completed_fields_impl.rs src/source_main_native_completed_install_impl.rs src/source_main_native_completed_publication_impl.rs src/source_main_native_completed_publication_tests.rs src/source_main_native_completed_runtime_test_impl.rs \
  src/source_main_native_completed_accepted_impl.rs src/source_main_native_completed_barrier_impl.rs src/source_main_native_completed_barrier_tests.rs src/source_main_native_completed_command_ack_impl.rs \
  src/source_main_native_completed_guard_test_impl.rs \
  src/source_main_native_completed_empty_tests.rs src/source_main_native_completed_controller_impl.rs src/source_main_native_completed_recovery_tests.rs \
  src/source_main_native_service_client_scope_tests.rs \
  src/source_index_cold_execution_impl.rs src/source_index_cold_execution_tests.rs src/source_index_cold_discard_tests.rs src/compact_generation_escrow_impl.rs src/compact_generation_escrow_tests.rs src/source_index_generation_escrow_impl.rs src/source_index_generation_escrow_tests.rs src/storage_verified_copy_impl.rs src/source_index_historical_native_await_tests.rs src/source_main_native_historical_row_bridge_impl.rs src/source_main_native_historical_row_bridge_tests.rs src/source_index_cold_escrow_guard_tests.rs \
  src/compact_generation_independent_prefix_impl.rs src/compact_independent_escrow_tests.rs src/source_index_independent_escrow_impl.rs src/source_index_independent_escrow_tests.rs src/source_index_inline_escrow_guard_impl.rs src/source_index_inline_escrow_guard_tests.rs src/source_main_active_escrow_controller_impl.rs src/source_main_active_service_escrow_tests.rs src/source_main_service_escrow_append_impl.rs src/source_main_service_escrow_runtime_impl.rs src/source_main_active_service_cold_history_impl.rs src/source_main_active_service_cold_history_tests.rs src/source_main_active_service_cold_history_actual_tests.rs src/source_index_history_pin_codec_impl.rs src/source_index_history_pin_codec_tests.rs src/source_main_active_history_pin_codec_tests.rs \
  src/source_main_native_cold_prefix_impl.rs src/source_main_native_cold_prefix_tests.rs src/source_main_bootstrap_promises_state_impl.rs src/source_main_bootstrap_promises_core_impl.rs src/source_main_bootstrap_promises_runtime_impl.rs src/source_main_bootstrap_promises_tests.rs \
  src/source_index_inline_database_impl.rs src/source_index_inline_database_tests.rs src/source_main_inline_index_runtime_impl.rs src/source_main_inline_service_runtime_impl.rs src/source_main_inline_service_session_impl.rs src/source_main_inline_service_tests.rs src/txindex_inline_continuation_impl.rs src/txindex_inline_continuation_tests.rs src/txindex_inline_raw_inputs.rs src/source_main_native_cold_prefix_review_tests.rs \
  src/source_index_historical_source_read_impl.rs src/source_index_historical_source_read_tests.rs src/source_main_native_cold_active_observation_impl.rs src/source_main_native_cold_active_reexecution_impl.rs src/source_main_native_cold_active_reexecution_tests.rs src/source_main_native_cold_historical_index_impl.rs src/source_main_native_cold_service_access_impl.rs \
  src/source_index_inline_recovery_impl.rs src/source_index_inline_recovery_tests.rs src/source_main_inline_install_runtime_impl.rs src/source_main_inline_wal_impl.rs src/source_main_inline_wal_install_impl.rs src/source_main_inline_wal_primed_setup.rs src/source_main_inline_wal_recovery_impl.rs src/source_main_inline_wal_tests.rs \
  src/channel_queue_v8_sort_tests.rs \
  src/source_index_inline_review_tests.rs \
  src/compact_escrow_descriptor_codec_impl.rs src/compact_escrow_descriptor_codec_tests.rs src/source_index_escrow_descriptor_codec_impl.rs src/source_index_verified_old_view_impl.rs src/source_index_verified_old_view_tests.rs \
  src/source_main_retained_caller_runtime_impl.rs src/source_main_retained_caller_session_impl.rs src/source_main_retained_caller_tests.rs src/source_main_retained_caller_test_transport.rs src/source_main_retained_inline_io_impl.rs src/txindex_extract_continuation_impl.rs src/txindex_extract_continuation_tests.rs \
  src/source_index_historical_inline_database_impl.rs src/source_index_historical_inline_database_tests.rs src/source_index_inline_source_outcome_impl.rs src/source_index_metadata_state_entries_impl.rs src/source_index_historical_inline_review_tests.rs src/bitcoin_supported_transaction.rs \
  src/source_main_retained_consensus_factory_impl.rs src/source_main_retained_consensus_runtime_impl.rs src/source_main_retained_consensus_session_impl.rs src/source_main_retained_consensus_tests.rs src/txindex_retained_consensus_job_tests.rs src/source_main_retained_caller_review_tests.rs \
  src/source_main_bootstrap_core_impl.rs src/source_main_bootstrap_runtime_impl.rs src/source_main_bootstrap_state_impl.rs src/source_main_bootstrap_tests.rs src/txindex_startup_continuation_impl.rs src/source_main_bootstrap_old_dispatch_tests.rs \
  src/source_index_historical_validity_impl.rs src/source_index_historical_validity_tests.rs src/source_main_native_historical_replay_impl.rs src/source_main_native_historical_replay_tests.rs src/source_main_native_historical_rows_impl.rs \
  src/source_main_active_service_recovery_impl.rs src/source_main_active_service_recovery_runtime_impl.rs src/source_main_active_service_recovery_tests.rs src/source_main_active_wal_exit_tests.rs \
  src/compact_history_prefix_impl.rs src/source_index_history_read_impl.rs src/source_index_history_read_tests.rs src/storage_history_prefix_impl.rs src/storage_history_prefix_tests.rs \
  src/source_main_active_service_action_codec_impl.rs src/source_main_active_service_action_codec_tests.rs src/source_main_active_service_durable_impl.rs src/source_main_active_service_durable_tests.rs src/source_main_active_service_wal.rs src/source_main_active_service_wal_tests.rs \
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
