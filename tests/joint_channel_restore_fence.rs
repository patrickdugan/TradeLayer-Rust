//! An ordinary Joint owner cannot advance channel liabilities. Inner channel
//! Ledger and standalone Spot snapshots remain valid in their own domains.
use std::collections::BTreeSet;
use tradelayer_core::joint::{JointError, JointProtocolState, JointSnapshotError};
use tradelayer_core::ledger::{
    AccountId, ChannelSide, Ledger, Operation, PropertyId, PropertyKind, TxId,
};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::spot::SpotExchange;

fn account(value: &str) -> AccountId {
    value.try_into().unwrap()
}
fn id(value: u8) -> TxId {
    TxId::from_bytes([value; 32])
}
fn property() -> PropertyId {
    7.try_into().unwrap()
}
fn funded() -> Ledger {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(1),
            Operation::CreateProperty {
                id: property(),
                issuer: account("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: 1_000,
            },
        )
        .unwrap();
    ledger
        .apply(
            id(2),
            Operation::Transfer {
                id: property(),
                from: account("alice"),
                to: account("bob"),
                amount_atoms: 200,
            },
        )
        .unwrap();
    ledger
}
fn bilateral() -> Ledger {
    let mut ledger = funded();
    for (txid, owner, side) in [(3, "alice", ChannelSide::A), (4, "bob", ChannelSide::B)] {
        ledger
            .apply(
                id(txid),
                Operation::ChannelCommit {
                    id: property(),
                    owner: account(owner),
                    channel: account("channel"),
                    side,
                    amount_atoms: 100,
                },
            )
            .unwrap();
    }
    ledger
}
fn net_claim(ledger: &mut Ledger) {
    ledger
        .apply(
            id(6),
            Operation::ChannelNetClaim {
                id: property(),
                channel: account("channel"),
                payer_side: ChannelSide::A,
                amount_atoms: 10,
                trade_reference: id(50),
                height: 2,
                expiry_height: 30,
            },
        )
        .unwrap();
}
fn checksum(mut bytes: Vec<u8>) -> Vec<u8> {
    let mut crc = !0u32;
    for &byte in &bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    bytes.extend_from_slice(&(!crc).to_be_bytes());
    bytes
}
fn size(bytes: &[u8], offset: usize) -> usize {
    u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize
}

/// Replace only the public TLSP frame of an otherwise-valid pristine TLJP.
/// The original Registry, global replay categories and boundary remain intact.
fn imported_joint(network: TradeLayerMainnet, ledger: Ledger) -> Vec<u8> {
    let permitted = BTreeSet::from([property()]);
    let ledger_bytes = ledger.snapshot_bytes().unwrap();
    assert_eq!(Ledger::from_snapshot_bytes(&ledger_bytes).unwrap(), ledger);
    let spot = SpotExchange::from_ledger_unverified(ledger, permitted.clone()).unwrap();
    let inner = spot.snapshot_bytes().unwrap();
    assert_eq!(
        SpotExchange::from_snapshot_bytes(&inner, &permitted)
            .unwrap()
            .snapshot_bytes()
            .unwrap(),
        inner
    );
    let base = JointProtocolState::from_bootstrap_unverified(
        network,
        account("admin"),
        funded(),
        permitted,
    )
    .unwrap()
    .snapshot_bytes()
    .unwrap();
    // TLJP magic[5], network[1], genesis[32], Registry length and payload.
    let spot_length_offset = 38 + 4 + size(&base, 38);
    let old_spot_length = size(&base, spot_length_offset);
    let mut replacement = base[..spot_length_offset].to_vec();
    replacement.extend_from_slice(&(inner.len() as u32).to_be_bytes());
    replacement.extend_from_slice(&inner);
    replacement.extend_from_slice(&base[spot_length_offset + 4 + old_spot_length..base.len() - 4]);
    checksum(replacement)
}
fn rejects_channel_import(ledger: Ledger) {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        assert!(matches!(
            JointProtocolState::from_bootstrap_unverified(
                network,
                account("admin"),
                ledger.clone(),
                BTreeSet::from([property()]),
            ),
            Err(JointError::UnsupportedLegacyLiabilities)
        ));
        let bytes = imported_joint(network, ledger.clone());
        let result = JointProtocolState::from_snapshot_bytes(
            &bytes,
            network,
            &account("admin"),
            &BTreeSet::from([property()]),
        );
        assert!(
            matches!(
                result,
                Err(JointSnapshotError::Joint(
                    JointError::UnsupportedLegacyLiabilities
                ))
            ),
            "ordinary {network:?} TLJP imported channel state despite its constructor policy: {result:?}"
        );
    }
}

#[test]
fn ordinary_joint_restore_rejects_valid_channel_escrow_with_repaired_crc() {
    rejects_channel_import(bilateral());
}

#[test]
fn ordinary_joint_restore_rejects_valid_pending_channel_withdrawal() {
    let mut ledger = bilateral();
    ledger
        .apply(
            id(5),
            Operation::ChannelWithdraw {
                id: property(),
                owner: account("alice"),
                channel: account("channel"),
                side: ChannelSide::A,
                amount_atoms: 25,
                unlock_height: 8,
            },
        )
        .unwrap();
    rejects_channel_import(ledger);
}

#[test]
fn ordinary_joint_restore_rejects_valid_pending_net_claim() {
    let mut ledger = bilateral();
    net_claim(&mut ledger);
    rejects_channel_import(ledger);
}

#[test]
fn ordinary_joint_restore_rejects_valid_settled_king_history() {
    let mut ledger = bilateral();
    net_claim(&mut ledger);
    let preview = ledger
        .preview_king_settlement_unverified(&account("channel"), property(), 2, 2, 3)
        .unwrap();
    ledger
        .apply(
            id(7),
            Operation::ChannelKingSettle {
                id: property(),
                channel: account("channel"),
                start_height: 2,
                end_height: 2,
                settled_at: 3,
                expected_root: preview.root,
                expected_count: preview.count,
                payer_side: preview.payer_side,
                net_atoms: preview.net_atoms,
            },
        )
        .unwrap();
    rejects_channel_import(ledger);
}

#[test]
fn ordinary_joint_without_channels_keeps_its_exact_bytes() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let original = JointProtocolState::from_bootstrap_unverified(
            network,
            account("admin"),
            funded(),
            BTreeSet::from([property()]),
        )
        .unwrap();
        let before = original.snapshot_bytes().unwrap();
        let after = JointProtocolState::from_snapshot_bytes(
            &before,
            network,
            &account("admin"),
            &BTreeSet::from([property()]),
        )
        .unwrap();
        assert_eq!(after, original);
        assert_eq!(after.snapshot_bytes().unwrap(), before);
    }
}
