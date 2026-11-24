package tapvwapadapter_test

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"math/big"
	"os"
	"testing"
	"time"

	"github.com/btcsuite/btcd/btcec/v2"
	"github.com/btcsuite/btcd/btcec/v2/schnorr"
	"github.com/lightninglabs/lndclient"
	"github.com/lightninglabs/taproot-assets/asset"
	"github.com/lightninglabs/taproot-assets/fn"
	"github.com/lightninglabs/taproot-assets/rfq"
	"github.com/lightninglabs/taproot-assets/rfqmath"
	"github.com/lightninglabs/taproot-assets/rfqmsg"
	"github.com/lightninglabs/taproot-assets/tapvwapadapter"
	"github.com/lightningnetwork/lnd/graph/db/models"
	"github.com/lightningnetwork/lnd/lnwire"
	"github.com/lightningnetwork/lnd/routing/route"
	"github.com/lightningnetwork/lnd/tlv"
	"github.com/stretchr/testify/require"
)

type goldenBundle struct {
	WireVersion   int `json:"wire_version"`
	WireByteLen   int `json:"wire_byte_length"`
	Bip340Vectors []struct {
		Index     int    `json:"index"`
		Pubkey    string `json:"pubkey"`
		Message   string `json:"message"`
		Signature string `json:"signature"`
		Valid     bool   `json:"valid"`
		Comment   string `json:"comment"`
	} `json:"bip340_official_32b_vectors"`
	ReceiptFixtures []struct {
		Name          string            `json:"name"`
		Receipt       map[string]any    `json:"receipt"`
		EncodedHex    string            `json:"encoded_hex"`
		EncodedSha256 string            `json:"encoded_sha256"`
		DigestHex     string            `json:"digest_hex"`
		NullifierHex  string            `json:"nullifier_hex"`
		Signatures    map[string]string `json:"signatures"`
	} `json:"receipt_fixtures"`
}

func mustHex32(t *testing.T, s string) [32]byte {
	t.Helper()
	b, err := hex.DecodeString(s)
	require.NoError(t, err)
	require.Len(t, b, 32)
	var out [32]byte
	copy(out[:], b)
	return out
}

func mustHex36(t *testing.T, s string) tapvwapadapter.Outpoint36 {
	t.Helper()
	b, err := hex.DecodeString(s)
	require.NoError(t, err)
	require.Len(t, b, 36)
	var out tapvwapadapter.Outpoint36
	copy(out[:], b)
	return out
}

func mustU64Str(t *testing.T, s string) uint64 {
	t.Helper()
	n, ok := new(big.Int).SetString(s, 10)
	require.True(t, ok)
	return n.Uint64()
}

func privFromScalar(u uint64) (*btcec.PrivateKey, [32]byte, [33]byte) {
	var b [32]byte
	new(big.Int).SetUint64(u).FillBytes(b[:])
	priv, pub := btcec.PrivKeyFromBytes(b[:])
	var xonly [32]byte
	copy(xonly[:], schnorr.SerializePubKey(pub))
	var comp [33]byte
	copy(comp[:], pub.SerializeCompressed())
	return priv, xonly, comp
}

// TestOfficialBIP340AndGoldenFixturesCrossCheck verifies all 15 official 32-byte
// BIP-340 vectors and frozen Receipt/v1 fixtures against btcec/v2/schnorr.
func TestOfficialBIP340AndGoldenFixturesCrossCheck(t *testing.T) {
	fixturePath := os.Getenv("TAP_VWAP_GOLDEN_FIXTURES")
	require.NotEmpty(t, fixturePath, "TAP_VWAP_GOLDEN_FIXTURES must point to golden-fixtures.json")

	rawJSON, err := os.ReadFile(fixturePath)
	require.NoError(t, err)

	var bundle goldenBundle
	require.NoError(t, json.Unmarshal(rawJSON, &bundle))
	require.Equal(t, 1, bundle.WireVersion)
	require.Equal(t, 526, bundle.WireByteLen)
	require.Len(t, bundle.Bip340Vectors, 15)

	for _, v := range bundle.Bip340Vectors {
		pubBytes, err := hex.DecodeString(v.Pubkey)
		require.NoError(t, err)
		msgBytes, err := hex.DecodeString(v.Message)
		require.NoError(t, err)
		sigBytes, err := hex.DecodeString(v.Signature)
		require.NoError(t, err)

		pk, pubErr := schnorr.ParsePubKey(pubBytes)
		sg, sigErr := schnorr.ParseSignature(sigBytes)
		got := pubErr == nil && sigErr == nil && sg.Verify(msgBytes, pk)
		require.Equalf(t, v.Valid, got, "BIP-340 vector %d (%s)", v.Index, v.Comment)
	}

	require.Len(t, bundle.ReceiptFixtures, 2)
	for _, fx := range bundle.ReceiptFixtures {
		m := fx.Receipt
		rec := &tapvwapadapter.ReceiptV1{
			Genesis:         mustHex32(t, m["genesis"].(string)),
			Policy:          mustHex32(t, m["policy"].(string)),
			Asset:           mustHex32(t, m["asset"].(string)),
			EdgeKey:         mustHex32(t, m["edgeKey"].(string)),
			BtcPeerKey:      mustHex32(t, m["btcPeerKey"].(string)),
			AssetPeerKey:    mustHex32(t, m["assetPeerKey"].(string)),
			RfqID:           mustHex32(t, m["rfqId"].(string)),
			QuoteTranscript: mustHex32(t, m["quoteTranscript"].(string)),
			PaymentHash:     mustHex32(t, m["paymentHash"].(string)),
			AssetPre:        mustHex32(t, m["assetPre"].(string)),
			AssetPost:       mustHex32(t, m["assetPost"].(string)),
			BtcPre:          mustHex32(t, m["btcPre"].(string)),
			BtcPost:         mustHex32(t, m["btcPost"].(string)),
			Direction:       uint8(m["direction"].(float64)),
			AssetEdgeSide:   uint8(m["assetEdgeSide"].(float64)),
			BtcEdgeSide:     uint8(m["btcEdgeSide"].(float64)),
			AssetOutpoint:   mustHex36(t, m["assetOutpoint"].(string)),
			AssetOfferer:    uint8(m["assetOfferer"].(float64)),
			AssetHtlc:       mustU64Str(t, m["assetHtlc"].(string)),
			BtcOutpoint:     mustHex36(t, m["btcOutpoint"].(string)),
			BtcOfferer:      uint8(m["btcOfferer"].(float64)),
			BtcHtlc:         mustU64Str(t, m["btcHtlc"].(string)),
			AssetAtoms:      mustU64Str(t, m["assetAtoms"].(string)),
			GrossBtcMsat:    mustU64Str(t, m["grossBtcMsat"].(string)),
		}
		enc, err := rec.Encode()
		require.NoError(t, err)
		require.Equal(t, fx.EncodedHex, hex.EncodeToString(enc))
		encSha := sha256.Sum256(enc)
		require.Equal(t, fx.EncodedSha256, hex.EncodeToString(encSha[:]))

		dig, err := rec.Digest()
		require.NoError(t, err)
		require.Equal(t, fx.DigestHex, hex.EncodeToString(dig[:]))

		nul, err := rec.Nullifier()
		require.NoError(t, err)
		require.Equal(t, fx.NullifierHex, hex.EncodeToString(nul[:]))

		for role, keyBytes := range map[string][32]byte{
			"edge":      rec.EdgeKey,
			"btcPeer":   rec.BtcPeerKey,
			"assetPeer": rec.AssetPeerKey,
		} {
			pk, err := schnorr.ParsePubKey(keyBytes[:])
			require.NoError(t, err)
			sb, err := hex.DecodeString(fx.Signatures[role])
			require.NoError(t, err)
			sg, err := schnorr.ParseSignature(sb)
			require.NoError(t, err)
			require.True(t, sg.Verify(dig[:], pk), "fixture %s role %s", fx.Name, role)
		}
	}
}

// TestUpstreamTapdRfqAcceptSignatureAuditAndMutations proves natively against
// pinned tapd v0.8.6 rfqmsg that V1 Accept messages leave TLV type 6 (Sig) all
// zeros and unverified on decode, and verifies that QuoteTranscriptV1 closes
// all 7 mutation vectors (ID, direction, rate, expiry, fill cap, asset ID, peer).
func TestUpstreamTapdRfqAcceptSignatureAuditAndMutations(t *testing.T) {
	reqID, err := rfqmsg.NewID()
	require.NoError(t, err)

	_, reqX, reqComp := privFromScalar(101)
	_, respX, respComp := privFromScalar(103)
	reqPriv, _, _ := privFromScalar(101)
	respPriv, _, _ := privFromScalar(103)

	var peerVertex, wrongPeerVertex route.Vertex
	copy(peerVertex[:], respComp[:])
	copy(wrongPeerVertex[:], reqComp[:])

	assetID := asset.ID{0xaa, 0xbb, 0xcc}
	buyReq, err := rfqmsg.NewBuyRequest(
		peerVertex, asset.NewSpecifierFromId(assetID),
		500_000,
		fn.None[uint64](),
		fn.None[rfqmath.BigIntFixedPoint](),
		fn.None[rfqmsg.AssetRate](),
		"",
		fn.None[rfqmsg.ExecutionPolicy](),
	)
	require.NoError(t, err)
	buyReq.ID = reqID

	expiry := time.Now().Add(30 * time.Minute).UTC().Truncate(time.Second)
	rateFP := rfqmath.NewBigIntFixedPoint(65_000_000_000, 0)
	assetRate := rfqmsg.NewAssetRate(rateFP, expiry)
	buyAccept := rfqmsg.NewBuyAcceptFromRequest(*buyReq, assetRate, fn.Some(uint64(400_000)))

	wireMsg, err := buyAccept.ToWire()
	require.NoError(t, err)

	// Extract TLV type 6 (Sig) directly from wireMsg.Data and prove it is 64 zero bytes.
	var extractedSig [64]byte
	sigRec := tlv.NewPrimitiveRecord[tlv.TlvType6](extractedSig)
	s, err := tlv.NewStream(sigRec.Record())
	require.NoError(t, err)
	parsedTypes, err := s.DecodeWithParsedTypesP2P(bytes.NewReader(wireMsg.Data))
	require.NoError(t, err)
	_, hasSigRecord := parsedTypes[sigRec.TlvType()]
	require.True(t, hasSigRecord, "V1 accept wire message carries TLV type 6 slot")
	require.Equal(t, [64]byte{}, sigRec.Val, "pinned tapd v0.8.6 leaves Accept TLV type 6 signature all zeros")

	lookup := func(id rfqmsg.ID) (rfqmsg.OutgoingMsg, bool) {
		if id == reqID {
			return buyReq, true
		}
		return nil, false
	}

	// 1. Mutating non-zero forged signature bytes on wire STILL PASSES upstream NewIncomingAcceptFromWire!
	mutatedSigWire := wireMsg
	mutatedSigWire.Data = slices.CloneBytes(wireMsg.Data)
	// Flip rate/expiry/max_in_asset via fresh forged accept wire with garbage sig:
	forgedRate := rfqmsg.NewAssetRate(rfqmath.NewBigIntFixedPoint(1, 0), expiry.Add(2*time.Hour))
	forgedAccept := rfqmsg.NewBuyAcceptFromRequest(*buyReq, forgedRate, fn.Some(uint64(999_999)))
	forgedWire, err := forgedAccept.ToWire()
	require.NoError(t, err)
	decodedForged, err := rfqmsg.NewIncomingAcceptFromWire(forgedWire, lookup)
	require.NoError(t, err, "upstream tapd accepts mutated rate, expiry, and fill cap without signature verification")
	parsedBuyAccept, ok := decodedForged.(*rfqmsg.BuyAccept)
	require.True(t, ok)
	require.Equal(t, uint64(999_999), parsedBuyAccept.AcceptedFillAmount().UnwrapOr(0))

	// 2. Session lookup catches unknown RFQ ID or wrong Peer in memory, NOT cryptographically.
	var wrongID rfqmsg.ID
	wrongID[0] = 0xff
	wrongIDAccept := *forgedAccept
	wrongIDAccept.ID = wrongID
	wrongIDWire, err := wrongIDAccept.ToWire()
	require.NoError(t, err)
	_, err = rfqmsg.NewIncomingAcceptFromWire(wrongIDWire, lookup)
	require.ErrorContains(t, err, "no outgoing request found")

	wrongPeerWire := wireMsg
	wrongPeerWire.Peer = wrongPeerVertex
	_, err = rfqmsg.NewIncomingAcceptFromWire(wrongPeerWire, lookup)
	require.ErrorContains(t, err, "does not match original request peer")

	// 3. Now verify our SignedQuoteTranscript extension closes all 7 mutation vectors cryptographically!
	coeffBig, ok := new(big.Int).SetString(rateFP.Coefficient.String(), 10)
	require.True(t, ok)
	coeffBytes, err := tapvwapadapter.BigIntToBytes32(coeffBig)
	require.NoError(t, err)
	reqWire, err := buyReq.ToWire()
	require.NoError(t, err)

	qt := tapvwapadapter.QuoteTranscriptV1{
		Genesis:          [32]byte{0x01},
		RfqID:            reqID,
		RequesterXOnly:   reqX,
		ResponderXOnly:   respX,
		AssetID:          assetID,
		Direction:        tapvwapadapter.DirectionEdgeSale,
		RequestMaxMsat:   500_000_000,
		AcceptedFillCap:  400_000,
		RateCoefficient:  coeffBytes,
		RateScale:        rateFP.Scale,
		ExpiryUnix:       uint64(expiry.Unix()),
		RawRequestSha256: sha256.Sum256(reqWire.Data),
		RawAcceptSha256:  sha256.Sum256(wireMsg.Data),
	}
	sqt, err := tapvwapadapter.SignQuoteTranscript(qt, reqPriv, respPriv)
	require.NoError(t, err)
	nowUnix := uint64(time.Now().Unix())
	_, err = sqt.Verify(nowUnix)
	require.NoError(t, err)

	mutations := []struct {
		name string
		fn   func(c *tapvwapadapter.SignedQuoteTranscript)
	}{
		{"rfq_id", func(c *tapvwapadapter.SignedQuoteTranscript) { c.Transcript.RfqID[0] ^= 1 }},
		{"direction", func(c *tapvwapadapter.SignedQuoteTranscript) { c.Transcript.Direction = 0 }},
		{"rate", func(c *tapvwapadapter.SignedQuoteTranscript) { c.Transcript.RateCoefficient[31] ^= 1 }},
		{"expiry", func(c *tapvwapadapter.SignedQuoteTranscript) { c.Transcript.ExpiryUnix += 60 }},
		{"fill_cap", func(c *tapvwapadapter.SignedQuoteTranscript) { c.Transcript.AcceptedFillCap = 400_001 }},
		{"asset_id", func(c *tapvwapadapter.SignedQuoteTranscript) { c.Transcript.AssetID[0] ^= 1 }},
		{"counterparty", func(c *tapvwapadapter.SignedQuoteTranscript) {
			_, altX, _ := privFromScalar(107)
			c.Transcript.ResponderXOnly = altX
		}},
		{"forged_sig", func(c *tapvwapadapter.SignedQuoteTranscript) { c.ResponderSig[0] ^= 1 }},
	}
	for _, tc := range mutations {
		cp := *sqt
		tc.fn(&cp)
		_, mErr := cp.Verify(nowUnix)
		require.Errorf(t, mErr, "mutation %s must fail closed", tc.name)
	}
	// Expired quote check
	_, expErr := sqt.Verify(qt.ExpiryUnix + 1)
	require.ErrorContains(t, expErr, "expired")
}

// slicesCloneBytes avoids extra imports.
type byteClone struct{}

var slices byteClone

func (byteClone) CloneBytes(b []byte) []byte {
	out := make([]byte, len(b))
	copy(out, b)
	return out
}

// TestThreePartyAdapterPeerScopeKeyRotationAndQuoteCap exercises independent
// peer leg verification, key-rotation nullifier invariance, on-chain exclusion,
// state rollback, inconsistent leg amounts, and pinned tapd quote-cap parity.
func TestThreePartyAdapterPeerScopeKeyRotationAndQuoteCap(t *testing.T) {
	genesis := [32]byte{0x11}
	policy := [32]byte{0x22}
	assetID := [32]byte{0x33}

	edgePriv, edgeX, edgeNode := privFromScalar(11)
	btcPriv, btcX, btcNode := privFromScalar(13)
	assetPriv, assetX, assetNode := privFromScalar(17)

	assetOp, err := tapvwapadapter.MakeOutpoint36(
		"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", 1,
	)
	require.NoError(t, err)
	btcOp, err := tapvwapadapter.MakeOutpoint36(
		"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", 0,
	)
	require.NoError(t, err)

	reg := tapvwapadapter.NewChannelRegistry(genesis, policy)
	assetEdgeSide, _, err := reg.RegisterChannel(
		assetOp, edgeNode, edgeX, assetNode, assetX, true, assetID, true, false,
	)
	require.NoError(t, err)
	btcEdgeSide, _, err := reg.RegisterChannel(
		btcOp, edgeNode, edgeX, btcNode, btcX, false, [32]byte{}, true, false,
	)
	require.NoError(t, err)

	preimage := [32]byte{0x99, 0x88, 0x77}
	payHash := sha256.Sum256(preimage[:])
	rfqID := [32]byte{0x44}
	coeff, err := tapvwapadapter.BigIntToBytes32(big.NewInt(500_000_000))
	require.NoError(t, err)
	nowUnix := uint64(1_790_000_000)

	qt := tapvwapadapter.QuoteTranscriptV1{
		Genesis:          genesis,
		RfqID:            rfqID,
		RequesterXOnly:   assetX,
		ResponderXOnly:   edgeX,
		AssetID:          assetID,
		Direction:        tapvwapadapter.DirectionEdgePurchase,
		RequestMaxMsat:   180_000_000,
		AcceptedFillCap:  0,
		RateCoefficient:  coeff,
		RateScale:        0,
		ExpiryUnix:       nowUnix + 3600,
		RawRequestSha256: [32]byte{0x51},
		RawAcceptSha256:  [32]byte{0x52},
	}
	sqt, err := tapvwapadapter.SignQuoteTranscript(qt, assetPriv, edgePriv)
	require.NoError(t, err)
	qtDig, err := sqt.Verify(nowUnix)
	require.NoError(t, err)

	assetOfferer := uint8(1 - assetEdgeSide)
	btcOfferer := btcEdgeSide

	assetObs := &tapvwapadapter.ObservedLegSettlement{
		Outpoint:             assetOp,
		IsAssetLeg:           true,
		AssetID:              assetID,
		OffererSide:          assetOfferer,
		HtlcID:               7,
		PaymentHash:          payHash,
		FulfillmentPreimage:  preimage,
		PreStateCommitment:   [32]byte{0xa1},
		PostStateCommitment:  [32]byte{0xa2},
		AssetDeltaAtoms:      50_000,
		BtcDeltaMsat:         354_000, // Anchor dust, NOT spot BTC volume!
		HtlcInterceptOutMsat: 100_000_000,
		Settled:              true,
		RevokedPriorState:    true,
	}
	btcObs := &tapvwapadapter.ObservedLegSettlement{
		Outpoint:             btcOp,
		IsAssetLeg:           false,
		OffererSide:          btcOfferer,
		HtlcID:               19,
		PaymentHash:          payHash,
		FulfillmentPreimage:  preimage,
		PreStateCommitment:   [32]byte{0xb1},
		PostStateCommitment:  [32]byte{0xb2},
		BtcDeltaMsat:         100_000_000,
		HtlcInterceptOutMsat: 100_000_000,
		Settled:              true,
		RevokedPriorState:    true,
	}

	rec := &tapvwapadapter.ReceiptV1{
		Genesis:         genesis,
		Policy:          policy,
		Asset:           assetID,
		EdgeKey:         edgeX,
		BtcPeerKey:      btcX,
		AssetPeerKey:    assetX,
		RfqID:           rfqID,
		QuoteTranscript: qtDig,
		PaymentHash:     payHash,
		AssetPre:        assetObs.PreStateCommitment,
		AssetPost:       assetObs.PostStateCommitment,
		BtcPre:          btcObs.PreStateCommitment,
		BtcPost:         btcObs.PostStateCommitment,
		Direction:       tapvwapadapter.DirectionEdgePurchase,
		AssetEdgeSide:   assetEdgeSide,
		BtcEdgeSide:     btcEdgeSide,
		AssetOutpoint:   assetOp,
		AssetOfferer:    assetOfferer,
		AssetHtlc:       7,
		BtcOutpoint:     btcOp,
		BtcOfferer:      btcOfferer,
		BtcHtlc:         19,
		AssetAtoms:      50_000,
		GrossBtcMsat:    100_000_000,
	}

	assetSig, assetScope, err := tapvwapadapter.SignAsAssetPeer(assetPriv, reg, assetObs, rec)
	require.NoError(t, err)
	require.Contains(t, assetScope.AcknowledgedUnverifiedLegs, "grossBtcMsat")

	btcSig, btcScope, err := tapvwapadapter.SignAsBtcPeer(btcPriv, reg, btcObs, rec)
	require.NoError(t, err)
	require.Contains(t, btcScope.AcknowledgedUnverifiedLegs, "assetAtoms")

	edgeSig, _, err := tapvwapadapter.SignAsEdge(edgePriv, reg, sqt, nowUnix, assetObs, btcObs, rec)
	require.NoError(t, err)

	verified, err := tapvwapadapter.VerifyThreePartyReceipt(reg, rec, tapvwapadapter.ThreePartySignatures{
		Edge:      edgeSig,
		BtcPeer:   btcSig,
		AssetPeer: assetSig,
	})
	require.NoError(t, err)
	require.False(t, verified.EligibleForIndex)
	require.Equal(t, uint8(1), verified.EvidenceTier)

	// Negative 1: Inconsistent leg amounts refused by respective peer signer.
	badBtcRec := *rec
	badBtcRec.GrossBtcMsat = 354_000 // Trying to substitute asset anchor dust!
	_, _, err = tapvwapadapter.SignAsBtcPeer(btcPriv, reg, btcObs, &badBtcRec)
	require.ErrorContains(t, err, "btc peer delta")

	badAssetRec := *rec
	badAssetRec.AssetAtoms = 50_001
	_, _, err = tapvwapadapter.SignAsAssetPeer(assetPriv, reg, assetObs, &badAssetRec)
	require.ErrorContains(t, err, "asset peer delta")

	// Negative 2: On-chain resolved HTLC excluded from spot receipt signing.
	onChainObs := *assetObs
	onChainObs.OnChainResolved = true
	_, _, err = tapvwapadapter.SignAsAssetPeer(assetPriv, reg, &onChainObs, rec)
	require.ErrorContains(t, err, "on-chain HTLC resolution")

	// Negative 3: Unrevoked prior commitment state / rollback rejected.
	unrevokedObs := *btcObs
	unrevokedObs.RevokedPriorState = false
	_, _, err = tapvwapadapter.SignAsBtcPeer(btcPriv, reg, &unrevokedObs, rec)
	require.ErrorContains(t, err, "not durably revoked")

	// Negative 4: Receipt-key rotation keeps immutable funding-peer order and nullifier identical!
	nulBefore, err := rec.Nullifier()
	require.NoError(t, err)
	_, newEdgeX, _ := privFromScalar(211)
	_, newAssetX, _ := privFromScalar(223)
	require.NoError(t, reg.RotateReceiptKey(assetOp, edgeNode, newEdgeX))
	require.NoError(t, reg.RotateReceiptKey(assetOp, assetNode, newAssetX))
	require.NoError(t, reg.RotateReceiptKey(btcOp, edgeNode, newEdgeX))

	rotatedSide, resolvedEdgeX, resolvedPeerX, _, err := reg.ResolvePartySideAndPeerKey(assetOp, edgeNode)
	require.NoError(t, err)
	require.Equal(t, assetEdgeSide, rotatedSide, "funding-peer order must not change on key rotation")
	require.Equal(t, newEdgeX, resolvedEdgeX)
	require.Equal(t, newAssetX, resolvedPeerX)

	rotatedRec := *rec
	rotatedRec.EdgeKey = newEdgeX
	rotatedRec.AssetPeerKey = newAssetX
	nulAfter, err := rotatedRec.Nullifier()
	require.NoError(t, err)
	require.Equal(t, nulBefore, nulAfter, "nullifier must stay identical across receipt-key rotation")

	// Negative 5: QuoteCapacityTracker exact parity with tapd AssetPurchasePolicy!
	var tapRfqID rfqmsg.ID
	copy(tapRfqID[:], rfqID[:])
	var tapAssetID asset.ID
	copy(tapAssetID[:], assetID[:])
	var tapPeer route.Vertex
	copy(tapPeer[:], assetNode[:])
	sellReq := rfqmsg.SellRequest{
		Peer:           tapPeer,
		AssetSpecifier: asset.NewSpecifierFromId(tapAssetID),
		PaymentMaxAmt:  180_000_000,
	}
	sellAccept := rfqmsg.SellAccept{
		Peer:      tapPeer,
		Request:   sellReq,
		ID:        tapRfqID,
		AssetRate: rfqmsg.NewAssetRate(rfqmath.NewBigIntFixedPoint(1_000_000, 0), time.Now().Add(time.Hour)),
	}
	tapPolicy := rfq.NewAssetPurchasePolicy(sellAccept)
	scid := lnwire.NewShortChanIDFromInt(uint64(tapRfqID.Scid()))

	makeIntercept := func(htlcID uint64, outMsat uint64, atoms uint64) lndclient.InterceptedHtlc {
		rec := rfqmsg.NewHtlc(
			[]*rfqmsg.AssetBalance{rfqmsg.NewAssetBalance(tapAssetID, atoms)},
			fn.Some(tapRfqID),
			fn.None[[]rfqmsg.ID](),
		)
		cr, pErr := lnwire.ParseCustomRecords(rec.Bytes())
		require.NoError(t, pErr)
		return lndclient.InterceptedHtlc{
			IncomingCircuitKey:  models.CircuitKey{ChanID: scid, HtlcID: htlcID},
			AmountInMsat:        354_000,
			AmountOutMsat:       lnwire.MilliSatoshi(outMsat),
			InWireCustomRecords: cr,
		}
	}

	tracker := tapvwapadapter.NewQuoteCapacityTracker()
	require.NoError(t, tracker.RegisterQuote(rfqID, 180_000_000))

	checker := func(_ context.Context, _ asset.Specifier, id asset.ID) (bool, error) {
		return id == tapAssetID, nil
	}

	// Admit 100M msat partial fill -> both accept.
	require.NoError(t, tapPolicy.CheckHtlcCompliance(t.Context(), makeIntercept(1, 100_000_000, 200_000), checker))
	require.NoError(t, tracker.ReserveInFlight(rfqID, 1, 100_000_000))

	// Concurrent 100M msat overfill -> both reject!
	require.Error(t, tapPolicy.CheckHtlcCompliance(t.Context(), makeIntercept(2, 100_000_000, 200_000), checker))
	require.Error(t, tracker.ReserveInFlight(rfqID, 2, 100_000_000))

	// Settle HTLC 1 -> capacity permanently consumed; 100M still rejects, 80M succeeds!
	tapPolicy.SettleHtlc(models.CircuitKey{ChanID: scid, HtlcID: 1})
	require.NoError(t, tracker.SettleInFlight(rfqID, 1))
	require.Error(t, tapPolicy.CheckHtlcCompliance(t.Context(), makeIntercept(3, 100_000_000, 200_000), checker))
	require.Error(t, tracker.ReserveInFlight(rfqID, 3, 100_000_000))

	require.NoError(t, tapPolicy.CheckHtlcCompliance(t.Context(), makeIntercept(4, 80_000_000, 160_000), checker))
	require.NoError(t, tracker.ReserveInFlight(rfqID, 4, 80_000_000))

	// Fail HTLC 4 -> releases 80M capacity so HTLC 5 (80M) succeeds!
	tapPolicy.UntrackHtlc(models.CircuitKey{ChanID: scid, HtlcID: 4})
	tracker.FailInFlight(rfqID, 4)
	require.NoError(t, tapPolicy.CheckHtlcCompliance(t.Context(), makeIntercept(5, 80_000_000, 160_000), checker))
	require.NoError(t, tracker.ReserveInFlight(rfqID, 5, 80_000_000))
}
