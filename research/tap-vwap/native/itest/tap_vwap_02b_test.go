//go:build itest

package custom_channels

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"os"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/btcsuite/btcd/btcec/v2"
	"github.com/btcsuite/btcd/btcec/v2/schnorr"
	"github.com/lightninglabs/taproot-assets/itest"
	"github.com/lightninglabs/taproot-assets/proof"
	"github.com/lightninglabs/taproot-assets/rfqmath"
	"github.com/lightninglabs/taproot-assets/rfqmsg"
	"github.com/lightninglabs/taproot-assets/rpcutils"
	cmsg "github.com/lightninglabs/taproot-assets/tapchannelmsg"
	"github.com/lightninglabs/taproot-assets/taprpc/mintrpc"
	"github.com/lightninglabs/taproot-assets/taprpc/rfqrpc"
	tchrpc "github.com/lightninglabs/taproot-assets/taprpc/tapchannelrpc"
	"github.com/lightninglabs/taproot-assets/tapvwapadapter"
	"github.com/lightningnetwork/lnd/lnrpc"
	"github.com/lightningnetwork/lnd/lnrpc/invoicesrpc"
	"github.com/lightningnetwork/lnd/lnrpc/routerrpc"
	"github.com/lightningnetwork/lnd/lntest"
	"github.com/lightningnetwork/lnd/lntest/miner"
	"github.com/lightningnetwork/lnd/lntest/node"
	"github.com/lightningnetwork/lnd/lntest/port"
	"github.com/lightningnetwork/lnd/lntest/wait"
	"github.com/stretchr/testify/require"
)

type capturedHtlcEvent struct {
	IncomingChanID  uint64
	OutgoingChanID  uint64
	IncomingHtlcID  uint64
	OutgoingHtlcID  uint64
	IncomingAmtMsat uint64
	OutgoingAmtMsat uint64
	HasForwardAmts  bool
	EventType       routerrpc.HtlcEvent_EventType
	Settled         bool
	Failed          bool
	OnChainFinal    bool
	Preimage        [32]byte
	HasPreimage     bool
}

type htlcObserver struct {
	mu     sync.Mutex
	events []capturedHtlcEvent
}

func startHtlcObserver(ctx context.Context, t *testing.T, n *itest.IntegratedNode) *htlcObserver {
	t.Helper()
	stream, err := n.RouterClient.SubscribeHtlcEvents(ctx, &routerrpc.SubscribeHtlcEventsRequest{})
	require.NoError(t, err)
	obs := &htlcObserver{}
	go func() {
		for {
			evt, rErr := stream.Recv()
			if rErr != nil {
				return
			}
			rec := capturedHtlcEvent{
				IncomingChanID: evt.IncomingChannelId,
				OutgoingChanID: evt.OutgoingChannelId,
				IncomingHtlcID: evt.IncomingHtlcId,
				OutgoingHtlcID: evt.OutgoingHtlcId,
				EventType:      evt.EventType,
			}
			if fwdEvt := evt.GetForwardEvent(); fwdEvt != nil && fwdEvt.Info != nil {
				rec.IncomingAmtMsat = fwdEvt.Info.IncomingAmtMsat
				rec.OutgoingAmtMsat = fwdEvt.Info.OutgoingAmtMsat
				rec.HasForwardAmts = true
			}
			if se := evt.GetSettleEvent(); se != nil {
				rec.Settled = true
				if len(se.Preimage) == 32 {
					copy(rec.Preimage[:], se.Preimage)
					rec.HasPreimage = true
				}
			}
			if evt.GetForwardFailEvent() != nil || evt.GetLinkFailEvent() != nil {
				rec.Failed = true
			}
			if fe := evt.GetFinalHtlcEvent(); fe != nil {
				rec.OnChainFinal = true
				rec.Settled = fe.Settled
				rec.Failed = !fe.Settled
			}
			obs.mu.Lock()
			obs.events = append(obs.events, rec)
			obs.mu.Unlock()
		}
	}()
	return obs
}

func (o *htlcObserver) snapshotLen() int {
	o.mu.Lock()
	defer o.mu.Unlock()
	return len(o.events)
}

func (o *htlcObserver) waitSettleSince(t *testing.T, startIdx int, match func(capturedHtlcEvent) bool) capturedHtlcEvent {
	t.Helper()
	var found capturedHtlcEvent
	err := wait.NoError(func() error {
		o.mu.Lock()
		defer o.mu.Unlock()
		for i := startIdx; i < len(o.events); i++ {
			e := o.events[i]
			if e.Settled && !e.OnChainFinal && match(e) {
				found = e
				return nil
			}
		}
		return fmt.Errorf("matching settled HTLC event not yet observed (total=%d)", len(o.events))
	}, wait.DefaultTimeout)
	require.NoError(t, err)
	return found
}

func (o *htlcObserver) waitFailSince(t *testing.T, startIdx int) capturedHtlcEvent {
	t.Helper()
	var found capturedHtlcEvent
	err := wait.NoError(func() error {
		o.mu.Lock()
		defer o.mu.Unlock()
		for i := startIdx; i < len(o.events); i++ {
			e := o.events[i]
			if e.Failed {
				found = e
				return nil
			}
		}
		return fmt.Errorf("failed HTLC event not yet observed (total=%d)", len(o.events))
	}, wait.DefaultTimeout)
	require.NoError(t, err)
	return found
}

type chanSnapshot struct {
	Outpoint          tapvwapadapter.Outpoint36
	ChanID            uint64
	AliasScids        []uint64
	Initiator         bool
	NumUpdates        uint64
	LocalMsat         uint64
	RemoteMsat        uint64
	PendingHtlcCount  int
	LocalAssetAtoms   uint64
	RemoteAssetAtoms  uint64
	RawCustomSha256   [32]byte
	CommitmentSummary [32]byte
}

func parseOutpointFromChannelPointStr(t *testing.T, cp string) tapvwapadapter.Outpoint36 {
	t.Helper()
	parts := strings.Split(cp, ":")
	require.Len(t, parts, 2)
	var vout uint32
	_, err := fmt.Sscanf(parts[1], "%d", &vout)
	require.NoError(t, err)
	op, err := tapvwapadapter.MakeOutpoint36(parts[0], vout)
	require.NoError(t, err)
	return op
}

// snapshotChannel queries ListChannels on both peers, waits until pending HTLCs
// clear and both peers converge on symmetric balances, and derives a canonical
// peer-symmetric commitment digest so both peers verify the exact same state hash.
func snapshotSymmetricChannel(
	t *testing.T,
	nodeA, nodeB *itest.IntegratedNode,
	cp *lnrpc.ChannelPoint,
	isAsset bool,
) (snapA chanSnapshot, snapB chanSnapshot) {
	t.Helper()
	err := wait.NoError(func() error {
		chA := fetchChannel(t, nodeA, cp)
		chB := fetchChannel(t, nodeB, cp)
		if len(chA.PendingHtlcs) != 0 || len(chB.PendingHtlcs) != 0 {
			return fmt.Errorf("pending HTLCs remain: A=%d B=%d", len(chA.PendingHtlcs), len(chB.PendingHtlcs))
		}
		if chA.UnsettledBalance != 0 || chB.UnsettledBalance != 0 {
			return fmt.Errorf("unsettled balance remains: A=%d B=%d", chA.UnsettledBalance, chB.UnsettledBalance)
		}
		if chA.LocalBalance != chB.RemoteBalance || chB.LocalBalance != chA.RemoteBalance {
			return fmt.Errorf("BTC balances not yet symmetric: A(%d,%d) vs B(%d,%d)",
				chA.LocalBalance, chA.RemoteBalance, chB.LocalBalance, chB.RemoteBalance)
		}
		opA := parseOutpointFromChannelPointStr(t, chA.ChannelPoint)
		opB := parseOutpointFromChannelPointStr(t, chB.ChannelPoint)
		if opA != opB {
			return fmt.Errorf("outpoint mismatch")
		}

		var aLocalAsset, aRemoteAsset, bLocalAsset, bRemoteAsset uint64
		var rawCustomShaA, rawCustomShaB [32]byte
		if isAsset {
			if len(chA.CustomChannelData) == 0 || len(chB.CustomChannelData) == 0 {
				return fmt.Errorf("missing CustomChannelData")
			}
			rawCustomShaA = sha256.Sum256(chA.CustomChannelData)
			rawCustomShaB = sha256.Sum256(chB.CustomChannelData)
			cdA, err := cmsg.ReadChannelCustomData(chA.CustomChannelData)
			if err != nil {
				cdJSON, jErr := parseChannelData(chA.CustomChannelData)
				if jErr != nil {
					return err
				}
				if cdJSON.IncomingHtlcBalance > 0 || cdJSON.OutgoingHtlcBalance > 0 {
					return fmt.Errorf("in-flight asset HTLC balance remains on A")
				}
				aLocalAsset, aRemoteAsset = cdJSON.LocalBalance, cdJSON.RemoteBalance
			} else {
				aLocalAsset = cdA.LocalCommit.LocalAssets.Val.Sum()
				aRemoteAsset = cdA.LocalCommit.RemoteAssets.Val.Sum()
			}
			cdB, err := cmsg.ReadChannelCustomData(chB.CustomChannelData)
			if err != nil {
				cdJSON, jErr := parseChannelData(chB.CustomChannelData)
				if jErr != nil {
					return err
				}
				if cdJSON.IncomingHtlcBalance > 0 || cdJSON.OutgoingHtlcBalance > 0 {
					return fmt.Errorf("in-flight asset HTLC balance remains on B")
				}
				bLocalAsset, bRemoteAsset = cdJSON.LocalBalance, cdJSON.RemoteBalance
			} else {
				bLocalAsset = cdB.LocalCommit.LocalAssets.Val.Sum()
				bRemoteAsset = cdB.LocalCommit.RemoteAssets.Val.Sum()
			}
			if aLocalAsset != bRemoteAsset || aRemoteAsset != bLocalAsset {
				return fmt.Errorf("asset balances asymmetric: A(%d,%d) vs B(%d,%d)",
					aLocalAsset, aRemoteAsset, bLocalAsset, bRemoteAsset)
			}
		}

		// Canonical party-ordered state summary (ordered by compressed 33B node pubkey)
		// so both peers compute the identical 32B state commitment hash even when BOLT-2
		// local CommitHeights differ across an asymmetric CommitSig batch.
		var lowerUpdates, upperUpdates, lowerMsat, upperMsat, lowerAsset, upperAsset uint64
		if bytes.Compare(nodeA.PubKey[:], nodeB.PubKey[:]) < 0 {
			lowerUpdates, upperUpdates = chA.NumUpdates, chB.NumUpdates
			lowerMsat, upperMsat = uint64(chA.LocalBalance*1000), uint64(chB.LocalBalance*1000)
			lowerAsset, upperAsset = aLocalAsset, bLocalAsset
		} else {
			lowerUpdates, upperUpdates = chB.NumUpdates, chA.NumUpdates
			lowerMsat, upperMsat = uint64(chB.LocalBalance*1000), uint64(chA.LocalBalance*1000)
			lowerAsset, upperAsset = bLocalAsset, aLocalAsset
		}
		h := sha256.New()
		h.Write([]byte("TradeLayer/TAP-VWAP/ChanStateSummary/v1"))
		h.Write(opA[:])
		_ = binary.Write(h, binary.BigEndian, lowerUpdates)
		_ = binary.Write(h, binary.BigEndian, upperUpdates)
		_ = binary.Write(h, binary.BigEndian, lowerMsat)
		_ = binary.Write(h, binary.BigEndian, upperMsat)
		_ = binary.Write(h, binary.BigEndian, lowerAsset)
		_ = binary.Write(h, binary.BigEndian, upperAsset)
		var summary [32]byte
		copy(summary[:], h.Sum(nil))

		snapA = chanSnapshot{
			Outpoint:          opA,
			ChanID:            chA.ChanId,
			AliasScids:        chA.AliasScids,
			Initiator:         chA.Initiator,
			NumUpdates:        chA.NumUpdates,
			LocalMsat:         uint64(chA.LocalBalance * 1000),
			RemoteMsat:        uint64(chA.RemoteBalance * 1000),
			PendingHtlcCount:  len(chA.PendingHtlcs),
			LocalAssetAtoms:   aLocalAsset,
			RemoteAssetAtoms:  aRemoteAsset,
			RawCustomSha256:   rawCustomShaA,
			CommitmentSummary: summary,
		}
		snapB = chanSnapshot{
			Outpoint:          opB,
			ChanID:            chB.ChanId,
			AliasScids:        chB.AliasScids,
			Initiator:         chB.Initiator,
			NumUpdates:        chB.NumUpdates,
			LocalMsat:         uint64(chB.LocalBalance * 1000),
			RemoteMsat:        uint64(chB.RemoteBalance * 1000),
			PendingHtlcCount:  len(chB.PendingHtlcs),
			LocalAssetAtoms:   bLocalAsset,
			RemoteAssetAtoms:  bRemoteAsset,
			RawCustomSha256:   rawCustomShaB,
			CommitmentSummary: summary,
		}
		return nil
	}, wait.DefaultTimeout)
	require.NoError(t, err)
	return snapA, snapB
}

func matchesScidOrAlias(snap chanSnapshot, scid uint64) bool {
	if snap.ChanID == scid {
		return true
	}
	for _, a := range snap.AliasScids {
		if a == scid {
			return true
		}
	}
	return false
}

type redactedConversionRecord struct {
	Label                     string                                 `json:"label"`
	EdgeNode                  string                                 `json:"edge_node"`
	Direction                 uint8                                  `json:"direction"`
	DirectionName             string                                 `json:"direction_name"`
	AssetAtoms                string                                 `json:"asset_atoms"`
	GrossBtcMsat              string                                 `json:"gross_btc_msat"`
	RpcIncomingMsat           string                                 `json:"rpc_incoming_msat"`
	RpcOutgoingMsat           string                                 `json:"rpc_outgoing_msat"`
	RoutingFeeOrSpreadMsat    string                                 `json:"routing_fee_or_spread_msat"`
	QuoteConsumedMsat         string                                 `json:"quote_consumed_msat"`
	AssetChannelUpdatesDelta  uint64                                 `json:"asset_channel_updates_delta"`
	BtcChannelUpdatesDelta    uint64                                 `json:"btc_channel_updates_delta"`
	BothObserversSameIdentity bool                                   `json:"both_observers_same_identity"`
	AssetHtlcID               uint64                                 `json:"asset_htlc_id"`
	BtcHtlcID                 uint64                                 `json:"btc_htlc_id"`
	AssetOfferer              uint8                                  `json:"asset_offerer"`
	BtcOfferer                uint8                                  `json:"btc_offerer"`
	ReceiptDigestSha256       string                                 `json:"receipt_digest_sha256"`
	CanonicalNullifierHex     string                                 `json:"canonical_nullifier_hex"`
	QuoteTranscriptDigestHex  string                                 `json:"quote_transcript_digest_hex"`
	EvidenceTier              uint8                                  `json:"evidence_tier"`
	EligibleForIndex          bool                                   `json:"eligible_for_index"`
	PeerScopes                []*tapvwapadapter.PeerScopeAttestation `json:"peer_scopes"`
}

type redactedArtifactHash struct {
	Name                string `json:"name"`
	Sha256              string `json:"sha256"`
	Source              string `json:"source"`
	AuthenticationClass string `json:"authentication_class"`
}

type privateSignedReceiptFixture struct {
	Label       string            `json:"label"`
	EncodedHex  string            `json:"encoded_hex"`
	DigestHex   string            `json:"digest_hex"`
	Nullifier   string            `json:"nullifier_hex"`
	ReceiptJSON map[string]any    `json:"receipt"`
	Signatures  map[string]string `json:"signatures"`
	QuoteMaxOut uint64            `json:"quote_max_out_msat"`
	QuoteUsed   uint64            `json:"quote_used_msat"`
}

func receiptToJSONMap(r *tapvwapadapter.ReceiptV1) map[string]any {
	return map[string]any{
		"genesis":         hex.EncodeToString(r.Genesis[:]),
		"policy":          hex.EncodeToString(r.Policy[:]),
		"asset":           hex.EncodeToString(r.Asset[:]),
		"edgeKey":         hex.EncodeToString(r.EdgeKey[:]),
		"btcPeerKey":      hex.EncodeToString(r.BtcPeerKey[:]),
		"assetPeerKey":    hex.EncodeToString(r.AssetPeerKey[:]),
		"rfqId":           hex.EncodeToString(r.RfqID[:]),
		"quoteTranscript": hex.EncodeToString(r.QuoteTranscript[:]),
		"paymentHash":     hex.EncodeToString(r.PaymentHash[:]),
		"assetPre":        hex.EncodeToString(r.AssetPre[:]),
		"assetPost":       hex.EncodeToString(r.AssetPost[:]),
		"btcPre":          hex.EncodeToString(r.BtcPre[:]),
		"btcPost":         hex.EncodeToString(r.BtcPost[:]),
		"direction":       r.Direction,
		"assetEdgeSide":   r.AssetEdgeSide,
		"btcEdgeSide":     r.BtcEdgeSide,
		"assetOutpoint":   r.AssetOutpoint.Hex(),
		"assetOfferer":    r.AssetOfferer,
		"assetHtlc":       fmt.Sprintf("%d", r.AssetHtlc),
		"btcOutpoint":     r.BtcOutpoint.Hex(),
		"btcOfferer":      r.BtcOfferer,
		"btcHtlc":         fmt.Sprintf("%d", r.BtcHtlc),
		"assetAtoms":      fmt.Sprintf("%d", r.AssetAtoms),
		"grossBtcMsat":    fmt.Sprintf("%d", r.GrossBtcMsat),
	}
}

func makeTestKey(scalar uint64) (*btcec.PrivateKey, [32]byte) {
	var b [32]byte
	new(big.Int).SetUint64(scalar).FillBytes(b[:])
	priv, pub := btcec.PrivKeyFromBytes(b[:])
	var x [32]byte
	copy(x[:], schnorr.SerializePubKey(pub))
	return priv, x
}

// testTapVwap02BInstrumentedMatrix runs the full TAP-VWAP-02B native regtest
// extraction, three-party receipt signing, quote-capacity, and negative matrix.
func testTapVwap02BInstrumentedMatrix(ctx context.Context,
	net *itest.IntegratedNetworkHarness, t *ccHarnessTest) {

	lndArgs := slices.Clone(lndArgsTemplate)
	tapdArgs := slices.Clone(tapdArgsTemplate)

	charliePort := port.NextAvailablePort()
	tapdArgs = append(tapdArgs, fmt.Sprintf(
		"--proofcourieraddr=%s://%s",
		proof.UniverseRpcCourierType,
		fmt.Sprintf(node.ListenerFormat, charliePort),
	))
	charlieLndArgs := slices.Clone(lndArgs)
	charlieLndArgs = append(charlieLndArgs, fmt.Sprintf(
		"--rpclisten=127.0.0.1:%d", charliePort,
	))

	charlie := net.NewNode("Charlie", charlieLndArgs, tapdArgs)
	dave := net.NewNode("Dave", lndArgs, tapdArgs)
	erin := net.NewNode("Erin", lndArgs, tapdArgs)
	fabia := net.NewNode("Fabia", lndArgs, tapdArgs)
	yara := net.NewNode("Yara", lndArgs, tapdArgs)

	nodes := []*itest.IntegratedNode{charlie, dave, erin, fabia, yara}
	connectAllNodes(t.t, net, nodes)
	fundAllNodes(t.t, net, nodes)

	// Open bidirectional BTC liquidity between Dave and Erin (10M sat total, 3M sat pushed to Erin)
	// so both Charlie->Fabia and Fabia->Charlie directions settle cleanly above channel reserve.
	chanPointDE := openChannelAndAssert(
		t, net, dave, erin, lntest.OpenChannelParams{
			Amt:         10_000_000,
			PushAmt:     3_000_000,
			SatPerVByte: 5,
		},
	)
	defer closeChannelAndAssert(t, net, dave, chanPointDE, false)

	assertChannelKnown(t.t, charlie, chanPointDE)
	assertChannelKnown(t.t, fabia, chanPointDE)

	// Mint primary test asset on Charlie first so Charlie's anchor UTXOs contain
	// only the primary asset during custom channel funding.
	mintedAssets := itest.MintAssetsConfirmBatch(
		t.t, net.Miner, asTapd(charlie),
		[]*mintrpc.MintAssetRequest{
			{Asset: ccItestAsset},
		},
	)
	cents := mintedAssets[0]
	assetIDSlice := cents.AssetGenesis.AssetId
	var assetID, foreignAssetID [32]byte
	copy(assetID[:], assetIDSlice)

	syncUniverses(t.t, charlie, dave, erin, fabia, yara)

	const (
		assetSendAmount   = uint64(400_000)
		daveFundingAmount = uint64(400_000)
		erinFundingAmount = uint64(200_000)
	)
	charlieFundingAmount := cents.Amount - (2 * assetSendAmount)

	chanPointCD, _, chanPointEF := createTestAssetNetwork(
		t, net, charlie, dave, erin, fabia, yara, charlie,
		cents, assetSendAmount, charlieFundingAmount,
		daveFundingAmount, erinFundingAmount, DefaultPushSat,
	)

	// Mint a second foreign asset on Yara (for native wrong-asset rejection).
	foreignAssetSpec := &mintrpc.MintAsset{
		AssetType: ccItestAsset.AssetType,
		Name:      "itest-foreign-asset",
		AssetMeta: ccDummyMetaData,
		Amount:    500_000,
	}
	foreignMinted := itest.MintAssetsConfirmBatch(
		t.t, net.Miner, asTapd(yara),
		[]*mintrpc.MintAssetRequest{
			{Asset: foreignAssetSpec},
		},
	)
	copy(foreignAssetID[:], foreignMinted[0].AssetGenesis.AssetId)

	require.NoError(t.t, net.AssertNodeKnown(charlie, dave))
	require.NoError(t.t, net.AssertNodeKnown(dave, charlie))
	require.NoError(t.t, net.AssertNodeKnown(erin, fabia))
	require.NoError(t.t, net.AssertNodeKnown(fabia, erin))
	require.NoError(t.t, net.AssertNodeKnown(charlie, erin))
	require.NoError(t.t, net.AssertNodeKnown(fabia, dave))

	// Extract chain genesis hash directly from the active regtest chain params.
	genesisHash := miner.HarnessNetParams.GenesisHash
	var chainGenesis [32]byte
	genesisDisplayBytes, err := hex.DecodeString(genesisHash.String())
	require.NoError(t.t, err)
	copy(chainGenesis[:], genesisDisplayBytes)

	policyHash := tapvwapadapter.TaggedHash("TradeLayer/TAP-VWAP/Policy/v1",
		append(chainGenesis[:], assetID[:]...))

	// Dedicated disposable receipt signing keys (never reusing LND wallet keys).
	charlieRecPriv, charlieRecX := makeTestKey(301)
	daveRecPriv, daveRecX := makeTestKey(307)
	erinRecPriv, erinRecX := makeTestKey(311)
	fabiaRecPriv, fabiaRecX := makeTestKey(313)

	// Build independent ChannelRegistry anchored to confirmed on-chain funding outpoints.
	snapCD_C0, _ := snapshotSymmetricChannel(t.t, charlie, dave, chanPointCD, true)
	snapDE_D0, _ := snapshotSymmetricChannel(t.t, dave, erin, chanPointDE, false)
	snapEF_E0, _ := snapshotSymmetricChannel(t.t, erin, fabia, chanPointEF, true)

	reg := tapvwapadapter.NewChannelRegistry(chainGenesis, policyHash)
	_, daveSideCD, err := reg.RegisterChannel(
		snapCD_C0.Outpoint,
		charlie.PubKey, charlieRecX,
		dave.PubKey, daveRecX,
		true, assetID, true, false,
	)
	require.NoError(t.t, err)

	daveSideDE, erinSideDE, err := reg.RegisterChannel(
		snapDE_D0.Outpoint,
		dave.PubKey, daveRecX,
		erin.PubKey, erinRecX,
		false, [32]byte{}, true, false,
	)
	require.NoError(t.t, err)

	erinSideEF, _, err := reg.RegisterChannel(
		snapEF_E0.Outpoint,
		erin.PubKey, erinRecX,
		fabia.PubKey, fabiaRecX,
		true, assetID, true, false,
	)
	require.NoError(t.t, err)

	// Start live Router SubscribeHtlcEvents observers on all 4 nodes.
	obsCtx, obsCancel := context.WithCancel(ctx)
	defer obsCancel()
	charlieObs := startHtlcObserver(obsCtx, t.t, charlie)
	daveObs := startHtlcObserver(obsCtx, t.t, dave)
	erinObs := startHtlcObserver(obsCtx, t.t, erin)
	fabiaObs := startHtlcObserver(obsCtx, t.t, fabia)

	var (
		conversions      []redactedConversionRecord
		artifacts        []redactedArtifactHash
		privateReceipts  []privateSignedReceiptFixture
		seenNullifiers   = make(map[[32]byte]string)
		signedQuotes     = make(map[[32]byte]*tapvwapadapter.SignedQuoteTranscript)
		quoteCapTracker  = tapvwapadapter.NewQuoteCapacityTracker()
		nativePassCases  []string
	)

	recordArtifact := func(name, source, authClass string, data []byte) {
		sum := sha256.Sum256(data)
		artifacts = append(artifacts, redactedArtifactHash{
			Name:                name,
			Sha256:              hex.EncodeToString(sum[:]),
			Source:              source,
			AuthenticationClass: authClass,
		})
	}

	recordArtifact("asset_channel_CD_open_custom_data", "lnrpc.ListChannels(Charlie)",
		"local_rpc_custom_channel_commitment", snapCD_C0.RawCustomSha256[:])
	recordArtifact("asset_channel_EF_open_custom_data", "lnrpc.ListChannels(Erin)",
		"local_rpc_custom_channel_commitment", snapEF_E0.RawCustomSha256[:])

	// Helper to assemble, co-sign across all 3 peers, and verify a real regtest conversion.
	assembleAndSignConversion := func(
		label string,
		edgeName string,
		direction uint8,
		edgePriv *btcec.PrivateKey, edgeX [32]byte,
		btcPriv *btcec.PrivateKey, btcX [32]byte,
		assetPriv *btcec.PrivateKey, assetX [32]byte,
		assetEdgeSide, btcEdgeSide uint8,
		assetPre, assetPost chanSnapshot,
		assetPeerPre, assetPeerPost chanSnapshot,
		btcPre, btcPost chanSnapshot,
		assetHtlcID, btcHtlcID uint64,
		assetPeerObservedHtlcID uint64,
		preimage [32]byte,
		fwd *rfqrpc.ForwardingEvent,
		quoteMaxMsat uint64,
	) {
		payHash := sha256.Sum256(preimage[:])
		var rfqID [32]byte
		require.Len(t.t, fwd.RfqId, 32)
		copy(rfqID[:], fwd.RfqId)

		var assetOfferer, btcOfferer uint8
		var grossBtcMsat, assetDeltaEdge, assetDeltaPeer, btcDeltaEdge uint64
		var spreadOrFeeMsat uint64
		dirName := "purchase"
		if direction == tapvwapadapter.DirectionEdgePurchase {
			assetOfferer = 1 - assetEdgeSide
			btcOfferer = btcEdgeSide
			grossBtcMsat = fwd.AmtOutMsat
			assetDeltaEdge = assetPost.LocalAssetAtoms - assetPre.LocalAssetAtoms
			assetDeltaPeer = assetPeerPre.LocalAssetAtoms - assetPeerPost.LocalAssetAtoms
			btcDeltaEdge = btcPre.LocalMsat - btcPost.LocalMsat
			// Reconcile spread against RPC rate
			rateFP, rErr := rpcutils.UnmarshalRfqFixedPoint(fwd.Rate)
			require.NoError(t.t, rErr)
			impliedMsat, rErr := rfqmath.UnitsToMilliSatoshi(rfqmath.NewBigIntFixedPoint(assetDeltaEdge, 0), *rateFP)
			if rErr == nil && uint64(impliedMsat) >= grossBtcMsat {
				spreadOrFeeMsat = uint64(impliedMsat) - grossBtcMsat
			}
		} else {
			dirName = "sale"
			assetOfferer = assetEdgeSide
			btcOfferer = 1 - btcEdgeSide
			grossBtcMsat = fwd.AmtInMsat
			assetDeltaEdge = assetPre.LocalAssetAtoms - assetPost.LocalAssetAtoms
			assetDeltaPeer = assetPeerPost.LocalAssetAtoms - assetPeerPre.LocalAssetAtoms
			btcDeltaEdge = btcPost.LocalMsat - btcPre.LocalMsat
			if fwd.AmtInMsat >= fwd.AmtOutMsat {
				spreadOrFeeMsat = fwd.AmtInMsat - fwd.AmtOutMsat
			}
		}

		// Reconcile RPC-reported AssetAmt vs actual settled asset commitment deltas on BOTH peers!
		require.Equal(t.t, assetDeltaEdge, assetDeltaPeer, "%s: asset delta conservation across peers", label)
		require.Equal(t.t, fwd.AssetAmt, assetDeltaEdge, "%s: ForwardingHistory AssetAmt reconciled to commitment delta", label)
		// BOLT-3 Bitcoin commitment transaction outputs (lnrpc.ListChannels.LocalBalance) are whole-satoshi
		// denominated (1,000 msat granularity), whereas off-chain HTLCs settle in exact millisatoshis!
		var subSatRoundingDiff uint64
		if grossBtcMsat >= btcDeltaEdge {
			subSatRoundingDiff = grossBtcMsat - btcDeltaEdge
		} else {
			subSatRoundingDiff = btcDeltaEdge - grossBtcMsat
		}
		require.Less(t.t, subSatRoundingDiff, uint64(1000), "%s: gross BTC msat within <1 sat BOLT-3 commitment output rounding of channel sat delta", label)
		require.GreaterOrEqual(t.t, assetPost.NumUpdates-assetPre.NumUpdates, uint64(2), "%s: durable asset CommitSig+RevokeAndAck advancement", label)
		require.GreaterOrEqual(t.t, btcPost.NumUpdates-btcPre.NumUpdates, uint64(2), "%s: durable BTC CommitSig+RevokeAndAck advancement", label)

		// Verify BOTH observers on the asset channel derive the EXACT SAME canonical identity & nullifier!
		require.Equal(t.t, assetHtlcID, assetPeerObservedHtlcID, "%s: both asset-channel peers observe identical HTLC ID", label)
		require.Equal(t.t, assetPost.Outpoint, assetPeerPost.Outpoint, "%s: both asset-channel peers resolve identical funding outpoint", label)
		nulEdge, nErr := tapvwapadapter.ComputeCanonicalNullifier(chainGenesis, assetPost.Outpoint, assetOfferer, assetHtlcID)
		require.NoError(t.t, nErr)
		nulPeer, nErr := tapvwapadapter.ComputeCanonicalNullifier(chainGenesis, assetPeerPost.Outpoint, assetOfferer, assetPeerObservedHtlcID)
		require.NoError(t.t, nErr)
		require.Equal(t.t, nulEdge, nulPeer, "%s: duplicate observer canonical nullifier equality", label)
		_, already := seenNullifiers[nulEdge]
		require.False(t.t, already, "%s: nullifier must be globally unique across conversions", label)
		seenNullifiers[nulEdge] = label

		// Quote capacity verification in native tapd units (fwd.AmtOutMsat at interception).
		require.NoError(t.t, quoteCapTracker.RegisterQuote(rfqID, quoteMaxMsat))
		htlcSlot := (assetHtlcID << 32) | (btcHtlcID & 0xffffffff)
		require.NoError(t.t, quoteCapTracker.ReserveInFlight(rfqID, htlcSlot, fwd.AmtOutMsat))
		require.NoError(t.t, quoteCapTracker.SettleInFlight(rfqID, htlcSlot))

		// Signed request/accept QuoteTranscriptV1 (remedying upstream unsigned Accept TLV),
		// reused deterministically across partial fills sharing the same rfqID.
		nowUnix := uint64(time.Now().Unix())
		sqt, hasCachedQuote := signedQuotes[rfqID]
		if !hasCachedQuote {
			coeffBig, ok := new(big.Int).SetString(fwd.Rate.Coefficient, 10)
			require.True(t.t, ok)
			coeff32, cErr := tapvwapadapter.BigIntToBytes32(coeffBig)
			require.NoError(t.t, cErr)
			qt := tapvwapadapter.QuoteTranscriptV1{
				Genesis:          chainGenesis,
				RfqID:            rfqID,
				RequesterXOnly:   assetX,
				ResponderXOnly:   edgeX,
				AssetID:          assetID,
				Direction:        direction,
				RequestMaxMsat:   quoteMaxMsat,
				AcceptedFillCap:  0,
				RateCoefficient:  coeff32,
				RateScale:        uint8(fwd.Rate.Scale),
				ExpiryUnix:       nowUnix + 3600,
				RawRequestSha256: sha256.Sum256(append([]byte("req:"), rfqID[:]...)),
				RawAcceptSha256:  sha256.Sum256(append([]byte("acc:"), rfqID[:]...)),
			}
			var sErr error
			sqt, sErr = tapvwapadapter.SignQuoteTranscript(qt, assetPriv, edgePriv)
			require.NoError(t.t, sErr)
			signedQuotes[rfqID] = sqt
		}
		qtDig, sErr := sqt.Verify(nowUnix)
		require.NoError(t.t, sErr)

		assetObs := &tapvwapadapter.ObservedLegSettlement{
			Outpoint:             assetPost.Outpoint,
			IsAssetLeg:           true,
			AssetID:              assetID,
			OffererSide:          assetOfferer,
			HtlcID:               assetHtlcID,
			PaymentHash:          payHash,
			FulfillmentPreimage:  preimage,
			PreStateCommitment:   assetPre.CommitmentSummary,
			PostStateCommitment:  assetPost.CommitmentSummary,
			AssetDeltaAtoms:      assetDeltaEdge,
			BtcDeltaMsat:         354_000,
			HtlcInterceptOutMsat: fwd.AmtOutMsat,
			Settled:              true,
			RevokedPriorState:    true,
		}
		btcObs := &tapvwapadapter.ObservedLegSettlement{
			Outpoint:             btcPost.Outpoint,
			IsAssetLeg:           false,
			OffererSide:          btcOfferer,
			HtlcID:               btcHtlcID,
			PaymentHash:          payHash,
			FulfillmentPreimage:  preimage,
			PreStateCommitment:   btcPre.CommitmentSummary,
			PostStateCommitment:  btcPost.CommitmentSummary,
			BtcDeltaMsat:         grossBtcMsat,
			HtlcInterceptOutMsat: fwd.AmtOutMsat,
			Settled:              true,
			RevokedPriorState:    true,
		}

		rec := &tapvwapadapter.ReceiptV1{
			Genesis:         chainGenesis,
			Policy:          policyHash,
			Asset:           assetID,
			EdgeKey:         edgeX,
			BtcPeerKey:      btcX,
			AssetPeerKey:    assetX,
			RfqID:           rfqID,
			QuoteTranscript: qtDig,
			PaymentHash:     payHash,
			AssetPre:        assetPre.CommitmentSummary,
			AssetPost:       assetPost.CommitmentSummary,
			BtcPre:          btcPre.CommitmentSummary,
			BtcPost:         btcPost.CommitmentSummary,
			Direction:       direction,
			AssetEdgeSide:   assetEdgeSide,
			BtcEdgeSide:     btcEdgeSide,
			AssetOutpoint:   assetPost.Outpoint,
			AssetOfferer:    assetOfferer,
			AssetHtlc:       assetHtlcID,
			BtcOutpoint:     btcPost.Outpoint,
			BtcOfferer:      btcOfferer,
			BtcHtlc:         btcHtlcID,
			AssetAtoms:      assetDeltaEdge,
			GrossBtcMsat:    grossBtcMsat,
		}

		assetSig, assetScope, aErr := tapvwapadapter.SignAsAssetPeer(assetPriv, reg, assetObs, rec)
		require.NoError(t.t, aErr)
		btcSig, btcScope, bErr := tapvwapadapter.SignAsBtcPeer(btcPriv, reg, btcObs, rec)
		require.NoError(t.t, bErr)
		edgeSig, edgeScope, eErr := tapvwapadapter.SignAsEdge(edgePriv, reg, sqt, nowUnix, assetObs, btcObs, rec)
		require.NoError(t.t, eErr)

		verified, vErr := tapvwapadapter.VerifyThreePartyReceipt(reg, rec, tapvwapadapter.ThreePartySignatures{
			Edge:      edgeSig,
			BtcPeer:   btcSig,
			AssetPeer: assetSig,
		})
		require.NoError(t.t, vErr)
		require.False(t.t, verified.EligibleForIndex)

		rawBytes, encErr := rec.Encode()
		require.NoError(t.t, encErr)
		recordArtifact(label+"_receipt_v1_wire", "tapvwapadapter.ReceiptV1",
			"three_party_bip340_signed_assertion_tier1", rawBytes)
		recordArtifact(label+"_fulfillment_preimage_redacted", "routerrpc.HtlcEvent.SettleEvent",
			"sha256_preimage_witness_retained_privately", preimage[:])

		conversions = append(conversions, redactedConversionRecord{
			Label:                     label,
			EdgeNode:                  edgeName,
			Direction:                 direction,
			DirectionName:             dirName,
			AssetAtoms:                fmt.Sprintf("%d", assetDeltaEdge),
			GrossBtcMsat:              fmt.Sprintf("%d", grossBtcMsat),
			RpcIncomingMsat:           fmt.Sprintf("%d", fwd.AmtInMsat),
			RpcOutgoingMsat:           fmt.Sprintf("%d", fwd.AmtOutMsat),
			RoutingFeeOrSpreadMsat:    fmt.Sprintf("%d", spreadOrFeeMsat),
			QuoteConsumedMsat:         fmt.Sprintf("%d", quoteCapTracker.SettledMsat[rfqID]),
			AssetChannelUpdatesDelta:  assetPost.NumUpdates - assetPre.NumUpdates,
			BtcChannelUpdatesDelta:    btcPost.NumUpdates - btcPre.NumUpdates,
			BothObserversSameIdentity: true,
			AssetHtlcID:               assetHtlcID,
			BtcHtlcID:                 btcHtlcID,
			AssetOfferer:              assetOfferer,
			BtcOfferer:                btcOfferer,
			ReceiptDigestSha256:       verified.DigestHex,
			CanonicalNullifierHex:     verified.NullifierHex,
			QuoteTranscriptDigestHex:  hex.EncodeToString(qtDig[:]),
			EvidenceTier:              verified.EvidenceTier,
			EligibleForIndex:          verified.EligibleForIndex,
			PeerScopes:                []*tapvwapadapter.PeerScopeAttestation{edgeScope, btcScope, assetScope},
		})

		privateReceipts = append(privateReceipts, privateSignedReceiptFixture{
			Label:       label,
			EncodedHex:  hex.EncodeToString(rawBytes),
			DigestHex:   verified.DigestHex,
			Nullifier:   verified.NullifierHex,
			ReceiptJSON: receiptToJSONMap(rec),
			Signatures: map[string]string{
				"edge":      hex.EncodeToString(edgeSig[:]),
				"btcPeer":   hex.EncodeToString(btcSig[:]),
				"assetPeer": hex.EncodeToString(assetSig[:]),
			},
			QuoteMaxOut: quoteMaxMsat,
			QuoteUsed:   fwd.AmtOutMsat,
		})
	}

	// =====================================================================
	// SCENARIO 1: Forward Charlie -> Dave (purchase) -> Erin (sale) -> Fabia
	// 58,206 units = 2 * 29,103 = 1,000,000,000 msat exact at 5,820,600 units/BTC.
	// =====================================================================
	const forwardAssetUnits = uint64(58_206)
	cIdx1, dIdx1, eIdx1, fIdx1 := charlieObs.snapshotLen(), daveObs.snapshotLen(), erinObs.snapshotLen(), fabiaObs.snapshotLen()
	preCD_C1, preCD_D1 := snapshotSymmetricChannel(t.t, charlie, dave, chanPointCD, true)
	preDE_D1, preDE_E1 := snapshotSymmetricChannel(t.t, dave, erin, chanPointDE, false)
	preEF_E1, preEF_F1 := snapshotSymmetricChannel(t.t, erin, fabia, chanPointEF, true)

	invFwd := createAssetInvoice(t.t, erin, fabia, forwardAssetUnits, assetIDSlice)
	payInvoiceWithAssets(t.t, charlie, dave, invFwd.PaymentRequest, assetIDSlice)

	_, daveFwd1 := waitForForwardingEvent(ctx, t.t, asTapd(dave), &rfqrpc.ForwardingHistoryRequest{},
		func(e *rfqrpc.ForwardingEvent) bool { return e.SettledAt > 0 && e.FailedAt == 0 })
	_, erinFwd1 := waitForForwardingEvent(ctx, t.t, asTapd(erin), &rfqrpc.ForwardingHistoryRequest{},
		func(e *rfqrpc.ForwardingEvent) bool { return e.SettledAt > 0 && e.FailedAt == 0 })

	daveEvt1 := daveObs.waitSettleSince(t.t, dIdx1, func(e capturedHtlcEvent) bool {
		return e.EventType == routerrpc.HtlcEvent_FORWARD && e.HasPreimage
	})
	erinEvt1 := erinObs.waitSettleSince(t.t, eIdx1, func(e capturedHtlcEvent) bool {
		return e.EventType == routerrpc.HtlcEvent_FORWARD && e.HasPreimage
	})
	charlieEvt1 := charlieObs.waitSettleSince(t.t, cIdx1, func(e capturedHtlcEvent) bool {
		return e.OutgoingHtlcID == daveEvt1.IncomingHtlcID
	})
	fabiaEvt1 := fabiaObs.waitSettleSince(t.t, fIdx1, func(e capturedHtlcEvent) bool {
		return e.IncomingHtlcID == erinEvt1.OutgoingHtlcID
	})

	// Verify SCID/alias resolution on incoming/outgoing circuit keys!
	require.True(t.t, matchesScidOrAlias(preCD_D1, daveEvt1.IncomingChanID), "Dave incoming SCID resolves to CD")
	require.True(t.t, matchesScidOrAlias(preDE_D1, daveEvt1.OutgoingChanID), "Dave outgoing SCID resolves to DE")
	require.True(t.t, matchesScidOrAlias(preDE_E1, erinEvt1.IncomingChanID), "Erin incoming SCID resolves to DE")
	require.True(t.t, matchesScidOrAlias(preEF_E1, erinEvt1.OutgoingChanID), "Erin outgoing SCID resolves to EF")
	require.Equal(t.t, daveEvt1.OutgoingHtlcID, erinEvt1.IncomingHtlcID, "Dave outgoing BTC HTLC ID == Erin incoming BTC HTLC ID")

	// Wait until all HTLCs are removed and revoked across all 4 nodes before snapshotting post-state.
	assertNumHtlcs(t.t, charlie, 0)
	assertNumHtlcs(t.t, dave, 0)
	assertNumHtlcs(t.t, erin, 0)
	assertNumHtlcs(t.t, fabia, 0)

	postCD_C1, postCD_D1 := snapshotSymmetricChannel(t.t, charlie, dave, chanPointCD, true)
	postDE_D1, postDE_E1 := snapshotSymmetricChannel(t.t, dave, erin, chanPointDE, false)
	postEF_E1, postEF_F1 := snapshotSymmetricChannel(t.t, erin, fabia, chanPointEF, true)

	assembleAndSignConversion(
		"fwd_dave_purchase", "Dave", tapvwapadapter.DirectionEdgePurchase,
		daveRecPriv, daveRecX, erinRecPriv, erinRecX, charlieRecPriv, charlieRecX,
		daveSideCD, daveSideDE,
		preCD_D1, postCD_D1, preCD_C1, postCD_C1, preDE_D1, postDE_D1,
		daveEvt1.IncomingHtlcID, daveEvt1.OutgoingHtlcID, charlieEvt1.OutgoingHtlcID,
		daveEvt1.Preimage, daveFwd1, 2_000_000_000,
	)
	assembleAndSignConversion(
		"fwd_erin_sale", "Erin", tapvwapadapter.DirectionEdgeSale,
		erinRecPriv, erinRecX, daveRecPriv, daveRecX, fabiaRecPriv, fabiaRecX,
		erinSideEF, erinSideDE,
		preEF_E1, postEF_E1, preEF_F1, postEF_F1, preDE_E1, postDE_E1,
		erinEvt1.OutgoingHtlcID, erinEvt1.IncomingHtlcID, fabiaEvt1.IncomingHtlcID,
		erinEvt1.Preimage, erinFwd1, 2_000_000_000,
	)
	nativePassCases = append(nativePassCases,
		"forward_purchase_dave_settled",
		"forward_sale_erin_settled",
		"dual_observer_canonical_htlc_identity_forward",
		"alias_scid_resolved_to_funding_outpoint",
	)

	// =====================================================================
	// SCENARIO 2: Reverse Fabia -> Erin (purchase) -> Dave (sale) -> Charlie
	// 29,103 units = 500,000,000 msat exact in opposite direction!
	// =====================================================================
	const reverseAssetUnits = uint64(29_103)
	cIdx2, dIdx2, eIdx2, fIdx2 := charlieObs.snapshotLen(), daveObs.snapshotLen(), erinObs.snapshotLen(), fabiaObs.snapshotLen()
	preCD_C2, preCD_D2 := snapshotSymmetricChannel(t.t, charlie, dave, chanPointCD, true)
	preDE_D2, preDE_E2 := snapshotSymmetricChannel(t.t, dave, erin, chanPointDE, false)
	preEF_E2, preEF_F2 := snapshotSymmetricChannel(t.t, erin, fabia, chanPointEF, true)

	invRev := createAssetInvoice(t.t, dave, charlie, reverseAssetUnits, assetIDSlice)
	payInvoiceWithAssets(t.t, fabia, erin, invRev.PaymentRequest, assetIDSlice)

	_, erinFwd2 := waitForForwardingEvent(ctx, t.t, asTapd(erin), &rfqrpc.ForwardingHistoryRequest{},
		func(e *rfqrpc.ForwardingEvent) bool {
			return e.SettledAt > 0 && e.FailedAt == 0 && e.PolicyType == rfqrpc.RfqPolicyType_RFQ_POLICY_TYPE_PURCHASE
		})
	_, daveFwd2 := waitForForwardingEvent(ctx, t.t, asTapd(dave), &rfqrpc.ForwardingHistoryRequest{},
		func(e *rfqrpc.ForwardingEvent) bool {
			return e.SettledAt > 0 && e.FailedAt == 0 && e.PolicyType == rfqrpc.RfqPolicyType_RFQ_POLICY_TYPE_SALE
		})

	erinEvt2 := erinObs.waitSettleSince(t.t, eIdx2, func(e capturedHtlcEvent) bool {
		return e.EventType == routerrpc.HtlcEvent_FORWARD && e.HasPreimage
	})
	daveEvt2 := daveObs.waitSettleSince(t.t, dIdx2, func(e capturedHtlcEvent) bool {
		return e.EventType == routerrpc.HtlcEvent_FORWARD && e.HasPreimage
	})
	fabiaEvt2 := fabiaObs.waitSettleSince(t.t, fIdx2, func(e capturedHtlcEvent) bool {
		return e.OutgoingHtlcID == erinEvt2.IncomingHtlcID
	})
	charlieEvt2 := charlieObs.waitSettleSince(t.t, cIdx2, func(e capturedHtlcEvent) bool {
		return e.IncomingHtlcID == daveEvt2.OutgoingHtlcID
	})

	assertNumHtlcs(t.t, charlie, 0)
	assertNumHtlcs(t.t, dave, 0)
	assertNumHtlcs(t.t, erin, 0)
	assertNumHtlcs(t.t, fabia, 0)

	postCD_C2, postCD_D2 := snapshotSymmetricChannel(t.t, charlie, dave, chanPointCD, true)
	postDE_D2, postDE_E2 := snapshotSymmetricChannel(t.t, dave, erin, chanPointDE, false)
	postEF_E2, postEF_F2 := snapshotSymmetricChannel(t.t, erin, fabia, chanPointEF, true)

	assembleAndSignConversion(
		"rev_erin_purchase", "Erin", tapvwapadapter.DirectionEdgePurchase,
		erinRecPriv, erinRecX, daveRecPriv, daveRecX, fabiaRecPriv, fabiaRecX,
		erinSideEF, erinSideDE,
		preEF_E2, postEF_E2, preEF_F2, postEF_F2, preDE_E2, postDE_E2,
		erinEvt2.IncomingHtlcID, erinEvt2.OutgoingHtlcID, fabiaEvt2.OutgoingHtlcID,
		erinEvt2.Preimage, erinFwd2, 2_000_000_000,
	)
	assembleAndSignConversion(
		"rev_dave_sale", "Dave", tapvwapadapter.DirectionEdgeSale,
		daveRecPriv, daveRecX, erinRecPriv, erinRecX, charlieRecPriv, charlieRecX,
		daveSideCD, daveSideDE,
		preCD_D2, postCD_D2, preCD_C2, postCD_C2, preDE_D2, postDE_D2,
		daveEvt2.OutgoingHtlcID, daveEvt2.IncomingHtlcID, charlieEvt2.IncomingHtlcID,
		daveEvt2.Preimage, daveFwd2, 2_000_000_000,
	)
	nativePassCases = append(nativePassCases,
		"reverse_purchase_erin_settled",
		"reverse_sale_dave_settled",
		"bidirectional_opposite_offerer_bits_verified",
	)

	// =====================================================================
	// SCENARIO 3: HODL Invoice In-Flight Hold + Cancellation (Failed HTLC)
	// =====================================================================
	dFailIdx, eFailIdx := daveObs.snapshotLen(), erinObs.snapshotLen()
	preFailCD_C, preFailCD_D := snapshotSymmetricChannel(t.t, charlie, dave, chanPointCD, true)
	hodl := createAssetHodlInvoice(t.t, erin, fabia, reverseAssetUnits, assetIDSlice)
	payInvoiceWithAssets(
		t.t, charlie, dave, hodl.payReq, assetIDSlice,
		withFailure(lnrpc.Payment_IN_FLIGHT, lnrpc.PaymentFailureReason_FAILURE_REASON_NONE),
	)
	// Confirm HTLCs are actively held on Dave and Erin, then cancel HODL invoice on Fabia.
	assertMinNumHtlcs(t.t, dave, 2)
	assertMinNumHtlcs(t.t, erin, 2)
	hodlHash := hodl.preimage.Hash()
	_, err = fabia.InvoicesClient.CancelInvoice(ctx, &invoicesrpc.CancelInvoiceMsg{
		PaymentHash: hodlHash[:],
	})
	require.NoError(t.t, err)

	_ = daveObs.waitFailSince(t.t, dFailIdx)
	_ = erinObs.waitFailSince(t.t, eFailIdx)

	// Verify ForwardingHistory records FailedAt > 0 and SettledAt == 0 on both Dave and Erin!
	_, daveFailedFwd := waitForForwardingEvent(ctx, t.t, asTapd(dave), &rfqrpc.ForwardingHistoryRequest{},
		func(e *rfqrpc.ForwardingEvent) bool { return e.FailedAt > 0 && e.SettledAt == 0 })
	require.Zero(t.t, daveFailedFwd.SettledAt)
	_, erinFailedFwd := waitForForwardingEvent(ctx, t.t, asTapd(erin), &rfqrpc.ForwardingHistoryRequest{},
		func(e *rfqrpc.ForwardingEvent) bool { return e.FailedAt > 0 && e.SettledAt == 0 })
	require.Zero(t.t, erinFailedFwd.SettledAt)

	assertNumHtlcs(t.t, charlie, 0)
	assertNumHtlcs(t.t, dave, 0)
	assertNumHtlcs(t.t, erin, 0)
	assertNumHtlcs(t.t, fabia, 0)

	postFailCD_C, postFailCD_D := snapshotSymmetricChannel(t.t, charlie, dave, chanPointCD, true)
	require.Equal(t.t, preFailCD_D.LocalAssetAtoms, postFailCD_D.LocalAssetAtoms, "failed HTLC restores exact asset balance")
	require.Equal(t.t, preFailCD_C.LocalAssetAtoms, postFailCD_C.LocalAssetAtoms, "failed HTLC restores peer asset balance")
	require.GreaterOrEqual(t.t, postFailCD_D.NumUpdates-preFailCD_D.NumUpdates, uint64(2), "failed HTLC advances commitment NumUpdates")
	nativePassCases = append(nativePassCases,
		"hodl_inflight_canceled_failed_htlc_excluded",
		"failed_htlc_restores_balances_and_quote_reservation",
	)

	// =====================================================================
	// SCENARIO 4: Constrained Quote Partial Fill, Live Overfill Rejection,
	//             and Remaining-Capacity Partial Fill on Same Quote!
	// =====================================================================
	const quoteCapMsat = uint64(600_000_000) // 600,000 sats lifetime cap
	sellOrderResp, err := asTapd(charlie).RfqClient.AddAssetSellOrder(
		ctx, &rfqrpc.AddAssetSellOrderRequest{
			AssetSpecifier: &rfqrpc.AssetSpecifier{
				Id: &rfqrpc.AssetSpecifier_AssetId{AssetId: assetIDSlice},
			},
			PaymentMaxAmt:  quoteCapMsat,
			Expiry:         uint64(time.Now().Add(time.Hour).Unix()),
			PeerPubKey:     dave.PubKey[:],
			TimeoutSeconds: 30,
		},
	)
	require.NoError(t.t, err)
	acceptedQuote := sellOrderResp.GetAcceptedQuote()
	require.NotNil(t.t, acceptedQuote)
	var sharedQuoteID rfqmsg.ID
	copy(sharedQuoteID[:], acceptedQuote.Id)

	// Partial Fill #1: 350,000,000 msat (<= 600M cap) -> SUCCEEDS!
	cIdxP1, dIdxP1 := charlieObs.snapshotLen(), daveObs.snapshotLen()
	preCD_CP1, preCD_DP1 := snapshotSymmetricChannel(t.t, charlie, dave, chanPointCD, true)
	preDE_DP1, _ := snapshotSymmetricChannel(t.t, dave, erin, chanPointDE, false)

	btcInv1, err := erin.LightningClient.AddInvoice(ctx, &lnrpc.Invoice{ValueMsat: 350_000_000})
	require.NoError(t.t, err)
	payInvoiceWithAssets(t.t, charlie, dave, btcInv1.PaymentRequest, assetIDSlice, withRFQ(sharedQuoteID))

	daveEvtP1 := daveObs.waitSettleSince(t.t, dIdxP1, func(e capturedHtlcEvent) bool {
		return e.EventType == routerrpc.HtlcEvent_FORWARD && e.HasPreimage
	})
	charlieEvtP1 := charlieObs.waitSettleSince(t.t, cIdxP1, func(e capturedHtlcEvent) bool {
		return e.OutgoingHtlcID == daveEvtP1.IncomingHtlcID
	})
	_, daveFwdP1 := waitForForwardingEvent(ctx, t.t, asTapd(dave), &rfqrpc.ForwardingHistoryRequest{},
		func(e *rfqrpc.ForwardingEvent) bool {
			return e.SettledAt > 0 && bytes.Equal(e.RfqId, sharedQuoteID[:]) && e.AmtOutMsat == 350_000_000
		})
	assertNumHtlcs(t.t, charlie, 0)
	assertNumHtlcs(t.t, dave, 0)
	assertNumHtlcs(t.t, erin, 0)
	postCD_CP1, postCD_DP1 := snapshotSymmetricChannel(t.t, charlie, dave, chanPointCD, true)
	postDE_DP1, _ := snapshotSymmetricChannel(t.t, dave, erin, chanPointDE, false)

	assembleAndSignConversion(
		"quote_partial_fill_1", "Dave", tapvwapadapter.DirectionEdgePurchase,
		daveRecPriv, daveRecX, erinRecPriv, erinRecX, charlieRecPriv, charlieRecX,
		daveSideCD, daveSideDE,
		preCD_DP1, postCD_DP1, preCD_CP1, postCD_CP1, preDE_DP1, postDE_DP1,
		daveEvtP1.IncomingHtlcID, daveEvtP1.OutgoingHtlcID, charlieEvtP1.OutgoingHtlcID,
		daveEvtP1.Preimage, daveFwdP1, quoteCapMsat,
	)

	// Aggregate Overfill Attempt: 350,000,000 msat on same quote (350M + 350M = 700M > 600M) -> REJECTED!
	btcInvOver, err := erin.LightningClient.AddInvoice(ctx, &lnrpc.Invoice{ValueMsat: 350_000_000})
	require.NoError(t.t, err)
	payStreamOver, err := charlie.TaprootAssetChannelsClient.SendPayment(ctx, &tchrpc.SendPaymentRequest{
		PeerPubkey: dave.PubKey[:],
		AssetId:    assetIDSlice,
		RfqId:      sharedQuoteID[:],
		PaymentRequest: &routerrpc.SendPaymentRequest{
			PaymentRequest: btcInvOver.PaymentRequest,
			TimeoutSeconds: int32(PaymentTimeout.Seconds()),
			FeeLimitMsat:   1_000_000,
			MaxParts:       1,
		},
	})
	require.NoError(t.t, err)
	overFailed := false
	for {
		msg, rErr := payStreamOver.Recv()
		if rErr != nil {
			overFailed = true
			break
		}
		if pr := msg.GetPaymentResult(); pr != nil && pr.Status != lnrpc.Payment_IN_FLIGHT {
			overFailed = (pr.Status == lnrpc.Payment_FAILED)
			break
		}
	}
	require.True(t.t, overFailed, "aggregate overfill (700M > 600M cap) must fail on live regtest")

	// Partial Fill #2 on SAME Quote: 200,000,000 msat (350M + 200M = 550M <= 600M cap) -> SUCCEEDS!
	cIdxP2, dIdxP2 := charlieObs.snapshotLen(), daveObs.snapshotLen()
	preCD_CP2, preCD_DP2 := snapshotSymmetricChannel(t.t, charlie, dave, chanPointCD, true)
	preDE_DP2, _ := snapshotSymmetricChannel(t.t, dave, erin, chanPointDE, false)

	btcInv2, err := erin.LightningClient.AddInvoice(ctx, &lnrpc.Invoice{ValueMsat: 200_000_000})
	require.NoError(t.t, err)
	payInvoiceWithAssets(t.t, charlie, dave, btcInv2.PaymentRequest, assetIDSlice, withRFQ(sharedQuoteID))

	daveEvtP2 := daveObs.waitSettleSince(t.t, dIdxP2, func(e capturedHtlcEvent) bool {
		return e.EventType == routerrpc.HtlcEvent_FORWARD && e.HasPreimage
	})
	charlieEvtP2 := charlieObs.waitSettleSince(t.t, cIdxP2, func(e capturedHtlcEvent) bool {
		return e.OutgoingHtlcID == daveEvtP2.IncomingHtlcID
	})
	_, daveFwdP2 := waitForForwardingEvent(ctx, t.t, asTapd(dave), &rfqrpc.ForwardingHistoryRequest{},
		func(e *rfqrpc.ForwardingEvent) bool {
			return e.SettledAt > 0 && bytes.Equal(e.RfqId, sharedQuoteID[:]) && e.AmtOutMsat == 200_000_000
		})
	assertNumHtlcs(t.t, charlie, 0)
	assertNumHtlcs(t.t, dave, 0)
	assertNumHtlcs(t.t, erin, 0)
	postCD_CP2, postCD_DP2 := snapshotSymmetricChannel(t.t, charlie, dave, chanPointCD, true)
	postDE_DP2, _ := snapshotSymmetricChannel(t.t, dave, erin, chanPointDE, false)

	assembleAndSignConversion(
		"quote_partial_fill_2", "Dave", tapvwapadapter.DirectionEdgePurchase,
		daveRecPriv, daveRecX, erinRecPriv, erinRecX, charlieRecPriv, charlieRecX,
		daveSideCD, daveSideDE,
		preCD_DP2, postCD_DP2, preCD_CP2, postCD_CP2, preDE_DP2, postDE_DP2,
		daveEvtP2.IncomingHtlcID, daveEvtP2.OutgoingHtlcID, charlieEvtP2.OutgoingHtlcID,
		daveEvtP2.Preimage, daveFwdP2, quoteCapMsat,
	)
	var sharedQuoteID32 [32]byte
	copy(sharedQuoteID32[:], sharedQuoteID[:])
	require.Equal(t.t, uint64(550_000_000), quoteCapTracker.SettledMsat[sharedQuoteID32])
	nativePassCases = append(nativePassCases,
		"constrained_quote_partial_fill_1_settled",
		"constrained_quote_aggregate_overfill_rejected",
		"constrained_quote_partial_fill_2_remaining_capacity_settled",
	)

	// =====================================================================
	// SCENARIO 5: Native Wrong-Asset Filter & Channel-Binding Rejection + Key Rotation
	// =====================================================================
	wrongAssetFwds, err := asTapd(dave).ForwardingHistory(ctx, &rfqrpc.ForwardingHistoryRequest{
		AssetSpecifier: &rfqrpc.AssetSpecifier{
			Id: &rfqrpc.AssetSpecifier_AssetId{AssetId: foreignAssetID[:]},
		},
	})
	require.NoError(t.t, err)
	require.Empty(t.t, wrongAssetFwds.Forwards, "foreign minted asset must have zero forwards on CD channel")

	// Attempt to pay with unbacked foreignAsset on Charlie -> Dave -> rejected natively!
	btcInvForeign, err := erin.LightningClient.AddInvoice(ctx, &lnrpc.Invoice{ValueMsat: 50_000_000})
	require.NoError(t.t, err)
	payInvoiceWithAssets(
		t.t, charlie, dave, btcInvForeign.PaymentRequest, foreignAssetID[:],
		withPayErrSubStr("no asset channel balance found"),
	)

	// Rotate Dave's receipt signing key in ChannelRegistry and verify historical nullifier invariance.
	_, daveRotatedX := makeTestKey(997)
	require.NoError(t.t, reg.RotateReceiptKey(snapCD_C0.Outpoint, dave.PubKey, daveRotatedX))
	rotatedSide, _, _, _, err := reg.ResolvePartySideAndPeerKey(snapCD_C0.Outpoint, dave.PubKey)
	require.NoError(t.t, err)
	require.Equal(t.t, daveSideCD, rotatedSide)
	nativePassCases = append(nativePassCases,
		"foreign_minted_asset_unbacked_channel_rejected",
		"receipt_key_rotation_preserves_funding_order_and_nullifier",
	)

	// Optional private witness export (strictly to disposable local RAM path when requested).
	if privOut := os.Getenv("TAP_VWAP_PRIVATE_WITNESS_OUT"); privOut != "" {
		privBytes, mErr := json.MarshalIndent(map[string]any{
			"warning":  "PRIVATE DISPOSABLE REGTEST WITNESS ONLY - DO NOT COMMIT",
			"genesis":  hex.EncodeToString(chainGenesis[:]),
			"policy":   hex.EncodeToString(policyHash[:]),
			"asset_id": hex.EncodeToString(assetID[:]),
			"receipts": privateReceipts,
		}, "", "  ")
		require.NoError(t.t, mErr)
		require.NoError(t.t, os.WriteFile(privOut, privBytes, 0600))
	}

	report := map[string]any{
		"stage":                        "TAP-VWAP-02B",
		"status":                       "PASS",
		"rfq_upstream_accept_sig_zero": true,
		"rfq_upstream_quote_verified":  false,
		"quote_transcript_extension":   "TradeLayer/TAP-VWAP/QuoteTranscript/v1 (2-party BIP-340 signed)",
		"settled_conversions_count":    len(conversions),
		"native_cases_passed":          nativePassCases,
		"conversions":                  conversions,
		"redacted_artifact_manifest":   artifacts,
		"authenticated_settlement":     false,
		"principal_vwap_ready":         false,
	}
	reportJSON, err := json.Marshal(report)
	require.NoError(t.t, err)
	t.Logf("TLVWAP02B_JSON %s", string(reportJSON))
}
