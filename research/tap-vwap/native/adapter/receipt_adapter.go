package tapvwapadapter

import (
	"bytes"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"errors"
	"fmt"
	"math/big"

	"github.com/btcsuite/btcd/btcec/v2"
	"github.com/btcsuite/btcd/btcec/v2/schnorr"
)

const (
	ReceiptWireVersion      uint8 = 1
	ReceiptWireBytes              = 526
	NullifierPayloadBytes         = 77
	QuoteTranscriptBytes          = 283
	DirectionEdgePurchase   uint8 = 0 // Edge receives asset, sends BTC.
	DirectionEdgeSale       uint8 = 1 // Edge receives BTC, sends asset.
	TagReceiptV1                  = "TradeLayer/TAP-VWAP/Receipt/v1"
	TagAssetHTLCNullifierV1       = "TradeLayer/TAP-VWAP/AssetHTLC/v1"
	TagQuoteTranscriptV1          = "TradeLayer/TAP-VWAP/QuoteTranscript/v1"
)

// TaggedHash computes SHA256(SHA256(tag) || SHA256(tag) || msg).
func TaggedHash(tag string, msg []byte) [32]byte {
	th := sha256.Sum256([]byte(tag))
	h := sha256.New()
	h.Write(th[:])
	h.Write(th[:])
	h.Write(msg)
	var out [32]byte
	copy(out[:], h.Sum(nil))
	return out
}

// Outpoint36 encodes display-order 32-byte txid followed by big-endian u32 vout.
type Outpoint36 [36]byte

func MakeOutpoint36(displayTxidHex string, vout uint32) (Outpoint36, error) {
	var op Outpoint36
	if len(displayTxidHex) != 64 {
		return op, errors.New("invalid display txid hex length")
	}
	txidBytes, err := hex.DecodeString(displayTxidHex)
	if err != nil {
		return op, fmt.Errorf("invalid display txid hex: %w", err)
	}
	copy(op[:32], txidBytes)
	binary.BigEndian.PutUint32(op[32:36], vout)
	if err := ValidateOutpoint36(op); err != nil {
		return op, err
	}
	return op, nil
}

func ValidateOutpoint36(op Outpoint36) error {
	var zero32 [32]byte
	if bytes.Equal(op[:32], zero32[:]) {
		return errors.New("unresolved null outpoint txid")
	}
	if binary.BigEndian.Uint32(op[32:36]) == 0xffffffff {
		return errors.New("unresolved sentinel outpoint vout")
	}
	return nil
}

func (op Outpoint36) Hex() string {
	return hex.EncodeToString(op[:])
}

// ReceiptV1 matches the normative 526-byte research v1 wire contract in receipt.cjs.
type ReceiptV1 struct {
	Genesis         [32]byte
	Policy          [32]byte
	Asset           [32]byte
	EdgeKey         [32]byte
	BtcPeerKey      [32]byte
	AssetPeerKey    [32]byte
	RfqID           [32]byte
	QuoteTranscript [32]byte
	PaymentHash     [32]byte
	AssetPre        [32]byte
	AssetPost       [32]byte
	BtcPre          [32]byte
	BtcPost         [32]byte
	Direction       uint8
	AssetEdgeSide   uint8
	BtcEdgeSide     uint8
	AssetOutpoint   Outpoint36
	AssetOfferer    uint8
	AssetHtlc       uint64
	BtcOutpoint     Outpoint36
	BtcOfferer      uint8
	BtcHtlc         uint64
	AssetAtoms      uint64
	GrossBtcMsat    uint64

	// Out-of-band lifecycle flags checked fail-closed before encoding/signing.
	SplicePending     bool
	FundingUnresolved bool
}

func validateBit(v uint8, name string) error {
	if v != 0 && v != 1 {
		return fmt.Errorf("invalid %s: %d", name, v)
	}
	return nil
}

func validateXOnlyKey(k [32]byte, name string) error {
	if _, err := schnorr.ParsePubKey(k[:]); err != nil {
		return fmt.Errorf("invalid curve key %s: %w", name, err)
	}
	return nil
}

// Encode serializes ReceiptV1 to its canonical 526-byte representation.
func (r *ReceiptV1) Encode() ([]byte, error) {
	if r == nil {
		return nil, errors.New("nil receipt")
	}
	if r.SplicePending || r.FundingUnresolved {
		return nil, errors.New("unresolved funding/splice identity")
	}
	if r.EdgeKey == r.BtcPeerKey || r.EdgeKey == r.AssetPeerKey || r.BtcPeerKey == r.AssetPeerKey {
		return nil, errors.New("distinct roles required")
	}
	if err := validateXOnlyKey(r.EdgeKey, "edgeKey"); err != nil {
		return nil, err
	}
	if err := validateXOnlyKey(r.BtcPeerKey, "btcPeerKey"); err != nil {
		return nil, err
	}
	if err := validateXOnlyKey(r.AssetPeerKey, "assetPeerKey"); err != nil {
		return nil, err
	}
	if r.AssetOutpoint == r.BtcOutpoint {
		return nil, errors.New("distinct channels required")
	}
	if err := ValidateOutpoint36(r.AssetOutpoint); err != nil {
		return nil, fmt.Errorf("asset outpoint: %w", err)
	}
	if err := ValidateOutpoint36(r.BtcOutpoint); err != nil {
		return nil, fmt.Errorf("btc outpoint: %w", err)
	}
	if r.AssetPre == r.AssetPost || r.BtcPre == r.BtcPost {
		return nil, errors.New("unchanged state commitment")
	}
	if err := validateBit(r.Direction, "direction"); err != nil {
		return nil, err
	}
	if err := validateBit(r.AssetEdgeSide, "asset edge side"); err != nil {
		return nil, err
	}
	if err := validateBit(r.BtcEdgeSide, "btc edge side"); err != nil {
		return nil, err
	}
	if err := validateBit(r.AssetOfferer, "asset offerer"); err != nil {
		return nil, err
	}
	if err := validateBit(r.BtcOfferer, "btc offerer"); err != nil {
		return nil, err
	}
	var wantAsset, wantBtc uint8
	if r.Direction == DirectionEdgePurchase {
		wantAsset = 1 - r.AssetEdgeSide
		wantBtc = r.BtcEdgeSide
	} else {
		wantAsset = r.AssetEdgeSide
		wantBtc = 1 - r.BtcEdgeSide
	}
	if r.AssetOfferer != wantAsset || r.BtcOfferer != wantBtc {
		return nil, errors.New("offerer/direction mismatch")
	}
	if r.AssetAtoms == 0 {
		return nil, errors.New("invalid asset atoms: zero")
	}
	if r.GrossBtcMsat == 0 {
		return nil, errors.New("invalid gross btc msat: zero")
	}

	buf := make([]byte, 0, ReceiptWireBytes)
	buf = append(buf, ReceiptWireVersion)
	for _, f := range [13][32]byte{
		r.Genesis, r.Policy, r.Asset,
		r.EdgeKey, r.BtcPeerKey, r.AssetPeerKey,
		r.RfqID, r.QuoteTranscript, r.PaymentHash,
		r.AssetPre, r.AssetPost, r.BtcPre, r.BtcPost,
	} {
		buf = append(buf, f[:]...)
	}
	buf = append(buf, r.Direction, r.AssetEdgeSide, r.BtcEdgeSide)
	buf = append(buf, r.AssetOutpoint[:]...)
	buf = append(buf, r.AssetOfferer)
	buf = binary.BigEndian.AppendUint64(buf, r.AssetHtlc)
	buf = append(buf, r.BtcOutpoint[:]...)
	buf = append(buf, r.BtcOfferer)
	buf = binary.BigEndian.AppendUint64(buf, r.BtcHtlc)
	buf = binary.BigEndian.AppendUint64(buf, r.AssetAtoms)
	buf = binary.BigEndian.AppendUint64(buf, r.GrossBtcMsat)
	if len(buf) != ReceiptWireBytes {
		return nil, fmt.Errorf("internal receipt length %d != %d", len(buf), ReceiptWireBytes)
	}
	return buf, nil
}

// Digest computes TaggedHash("TradeLayer/TAP-VWAP/Receipt/v1", Encode(r)).
func (r *ReceiptV1) Digest() ([32]byte, error) {
	raw, err := r.Encode()
	if err != nil {
		return [32]byte{}, err
	}
	return TaggedHash(TagReceiptV1, raw), nil
}

// Nullifier computes TaggedHash("TradeLayer/TAP-VWAP/AssetHTLC/v1", genesis || assetOutpoint || assetOfferer || assetHtlc).
func (r *ReceiptV1) Nullifier() ([32]byte, error) {
	if _, err := r.Encode(); err != nil {
		return [32]byte{}, err
	}
	return ComputeCanonicalNullifier(r.Genesis, r.AssetOutpoint, r.AssetOfferer, r.AssetHtlc)
}

// ComputeCanonicalNullifier derives the global cross-epoch HTLC nullifier directly
// from immutable asset-channel funding identity and local HTLC index.
func ComputeCanonicalNullifier(genesis [32]byte, assetOutpoint Outpoint36, assetOfferer uint8, assetHtlc uint64) ([32]byte, error) {
	if err := ValidateOutpoint36(assetOutpoint); err != nil {
		return [32]byte{}, err
	}
	if err := validateBit(assetOfferer, "asset offerer"); err != nil {
		return [32]byte{}, err
	}
	payload := make([]byte, 0, NullifierPayloadBytes)
	payload = append(payload, genesis[:]...)
	payload = append(payload, assetOutpoint[:]...)
	payload = append(payload, assetOfferer)
	payload = binary.BigEndian.AppendUint64(payload, assetHtlc)
	return TaggedHash(TagAssetHTLCNullifierV1, payload), nil
}

// QuoteTranscriptV1 remedies upstream tapd v0.8.6's unsigned Accept wire message
// (rfqmsg/buy_accept.go:114, sell_accept.go:119 TODO(ffranr)) by binding the full
// negotiated quote parameters and both peers' BIP-340 signatures.
type QuoteTranscriptV1 struct {
	Genesis          [32]byte
	RfqID            [32]byte
	RequesterXOnly   [32]byte
	ResponderXOnly   [32]byte
	AssetID          [32]byte
	Direction        uint8  // 0 = edge purchase (SellRequest/SellAccept), 1 = edge sale (BuyRequest/BuyAccept).
	RequestMaxMsat   uint64 // Agreed max outbound msat for purchase, or UnitsToMsat(MaxAsset, AskRate) for sale.
	AcceptedFillCap  uint64 // Optional negotiated asset/msat cap (0 if unset).
	RateCoefficient  [32]byte
	RateScale        uint8
	ExpiryUnix       uint64
	RawRequestSha256 [32]byte
	RawAcceptSha256  [32]byte
}

func BigIntToBytes32(x *big.Int) ([32]byte, error) {
	var out [32]byte
	if x == nil || x.Sign() <= 0 || x.BitLen() > 256 {
		return out, errors.New("rate coefficient out of 256-bit positive range")
	}
	x.FillBytes(out[:])
	return out, nil
}

func (qt *QuoteTranscriptV1) Encode() ([]byte, error) {
	if qt == nil {
		return nil, errors.New("nil quote transcript")
	}
	if err := validateXOnlyKey(qt.RequesterXOnly, "requester"); err != nil {
		return nil, err
	}
	if err := validateXOnlyKey(qt.ResponderXOnly, "responder"); err != nil {
		return nil, err
	}
	if qt.RequesterXOnly == qt.ResponderXOnly {
		return nil, errors.New("quote requester and responder must be distinct")
	}
	if err := validateBit(qt.Direction, "quote direction"); err != nil {
		return nil, err
	}
	if qt.RequestMaxMsat == 0 {
		return nil, errors.New("quote max msat must be positive")
	}
	var zero32 [32]byte
	if qt.RateCoefficient == zero32 {
		return nil, errors.New("quote rate coefficient must be non-zero")
	}
	buf := make([]byte, 0, QuoteTranscriptBytes)
	buf = append(buf, 1) // transcript version 1
	buf = append(buf, qt.Genesis[:]...)
	buf = append(buf, qt.RfqID[:]...)
	buf = append(buf, qt.RequesterXOnly[:]...)
	buf = append(buf, qt.ResponderXOnly[:]...)
	buf = append(buf, qt.AssetID[:]...)
	buf = append(buf, qt.Direction)
	buf = binary.BigEndian.AppendUint64(buf, qt.RequestMaxMsat)
	buf = binary.BigEndian.AppendUint64(buf, qt.AcceptedFillCap)
	buf = append(buf, qt.RateCoefficient[:]...)
	buf = append(buf, qt.RateScale)
	buf = binary.BigEndian.AppendUint64(buf, qt.ExpiryUnix)
	buf = append(buf, qt.RawRequestSha256[:]...)
	buf = append(buf, qt.RawAcceptSha256[:]...)
	if len(buf) != QuoteTranscriptBytes {
		return nil, fmt.Errorf("internal quote transcript length %d != %d", len(buf), QuoteTranscriptBytes)
	}
	return buf, nil
}

func (qt *QuoteTranscriptV1) Digest() ([32]byte, error) {
	raw, err := qt.Encode()
	if err != nil {
		return [32]byte{}, err
	}
	return TaggedHash(TagQuoteTranscriptV1, raw), nil
}

type SignedQuoteTranscript struct {
	Transcript   QuoteTranscriptV1
	RequesterSig [64]byte
	ResponderSig [64]byte
}

func SignQuoteTranscript(qt QuoteTranscriptV1, requesterPriv, responderPriv *btcec.PrivateKey) (*SignedQuoteTranscript, error) {
	if requesterPriv == nil || responderPriv == nil {
		return nil, errors.New("missing quote signer key")
	}
	var reqX, respX [32]byte
	copy(reqX[:], schnorr.SerializePubKey(requesterPriv.PubKey()))
	copy(respX[:], schnorr.SerializePubKey(responderPriv.PubKey()))
	if reqX != qt.RequesterXOnly || respX != qt.ResponderXOnly {
		return nil, errors.New("quote signer key mismatch")
	}
	dig, err := qt.Digest()
	if err != nil {
		return nil, err
	}
	s1, err := schnorr.Sign(requesterPriv, dig[:])
	if err != nil {
		return nil, err
	}
	s2, err := schnorr.Sign(responderPriv, dig[:])
	if err != nil {
		return nil, err
	}
	var out SignedQuoteTranscript
	out.Transcript = qt
	copy(out.RequesterSig[:], s1.Serialize())
	copy(out.ResponderSig[:], s2.Serialize())
	return &out, nil
}

func (sqt *SignedQuoteTranscript) Verify(nowUnix uint64) ([32]byte, error) {
	if sqt == nil {
		return [32]byte{}, errors.New("nil signed quote transcript")
	}
	dig, err := sqt.Transcript.Digest()
	if err != nil {
		return [32]byte{}, err
	}
	if nowUnix > sqt.Transcript.ExpiryUnix {
		return [32]byte{}, errors.New("signed quote transcript expired")
	}
	reqPub, err := schnorr.ParsePubKey(sqt.Transcript.RequesterXOnly[:])
	if err != nil {
		return [32]byte{}, err
	}
	respPub, err := schnorr.ParsePubKey(sqt.Transcript.ResponderXOnly[:])
	if err != nil {
		return [32]byte{}, err
	}
	sig1, err := schnorr.ParseSignature(sqt.RequesterSig[:])
	if err != nil || !sig1.Verify(dig[:], reqPub) {
		return [32]byte{}, errors.New("invalid quote requester signature")
	}
	sig2, err := schnorr.ParseSignature(sqt.ResponderSig[:])
	if err != nil || !sig2.Verify(dig[:], respPub) {
		return [32]byte{}, errors.New("invalid quote responder signature")
	}
	return dig, nil
}

// RegisteredChannel binds an on-chain funding outpoint to immutable funding-peer
// ordering (Party0NodePub < Party1NodePub in compressed 33-byte lexicographic
// BOLT-2 funding order) separately from rotatable receipt signing keys.
type RegisteredChannel struct {
	Outpoint         Outpoint36
	Party0NodePub    [33]byte
	Party1NodePub    [33]byte
	Party0ReceiptKey [32]byte
	Party1ReceiptKey [32]byte
	IsAssetChannel   bool
	AssetID          [32]byte
	FundingConfirmed bool
	SplicePending    bool
}

type ChannelRegistry struct {
	Genesis  [32]byte
	Policy   [32]byte
	Channels map[Outpoint36]*RegisteredChannel
}

func NewChannelRegistry(genesis, policy [32]byte) *ChannelRegistry {
	return &ChannelRegistry{
		Genesis:  genesis,
		Policy:   policy,
		Channels: make(map[Outpoint36]*RegisteredChannel),
	}
}

// RegisterChannel orders the two channel peers by their immutable 33-byte node
// identity pubkeys (matching BOLT-2 multisig funding ordering), never by their
// rotatable receipt keys.
func (cr *ChannelRegistry) RegisterChannel(
	op Outpoint36,
	nodeA [33]byte, receiptKeyA [32]byte,
	nodeB [33]byte, receiptKeyB [32]byte,
	isAsset bool, assetID [32]byte,
	confirmed bool, splicePending bool,
) (sideA uint8, sideB uint8, err error) {
	if err := ValidateOutpoint36(op); err != nil {
		return 0, 0, err
	}
	if nodeA == nodeB {
		return 0, 0, errors.New("identical channel peer node pubkeys")
	}
	if err := validateXOnlyKey(receiptKeyA, "receiptKeyA"); err != nil {
		return 0, 0, err
	}
	if err := validateXOnlyKey(receiptKeyB, "receiptKeyB"); err != nil {
		return 0, 0, err
	}
	rc := &RegisteredChannel{
		Outpoint:         op,
		IsAssetChannel:   isAsset,
		AssetID:          assetID,
		FundingConfirmed: confirmed,
		SplicePending:    splicePending,
	}
	if bytes.Compare(nodeA[:], nodeB[:]) < 0 {
		rc.Party0NodePub = nodeA
		rc.Party1NodePub = nodeB
		rc.Party0ReceiptKey = receiptKeyA
		rc.Party1ReceiptKey = receiptKeyB
		sideA, sideB = 0, 1
	} else {
		rc.Party0NodePub = nodeB
		rc.Party1NodePub = nodeA
		rc.Party0ReceiptKey = receiptKeyB
		rc.Party1ReceiptKey = receiptKeyA
		sideA, sideB = 1, 0
	}
	cr.Channels[op] = rc
	return sideA, sideB, nil
}

// RotateReceiptKey rotates a party's dedicated receipt signing key while keeping
// Party0/Party1 funding-side ordering strictly invariant.
func (cr *ChannelRegistry) RotateReceiptKey(op Outpoint36, nodePub [33]byte, newReceiptKey [32]byte) error {
	rc, ok := cr.Channels[op]
	if !ok {
		return errors.New("unknown channel outpoint")
	}
	if err := validateXOnlyKey(newReceiptKey, "newReceiptKey"); err != nil {
		return err
	}
	switch nodePub {
	case rc.Party0NodePub:
		rc.Party0ReceiptKey = newReceiptKey
		return nil
	case rc.Party1NodePub:
		rc.Party1ReceiptKey = newReceiptKey
		return nil
	default:
		return errors.New("node pubkey not bound to channel")
	}
}

func (cr *ChannelRegistry) ResolvePartySideAndPeerKey(op Outpoint36, edgeNodePub [33]byte) (edgeSide uint8, edgeKey [32]byte, peerKey [32]byte, rc *RegisteredChannel, err error) {
	rc, ok := cr.Channels[op]
	if !ok {
		return 0, [32]byte{}, [32]byte{}, nil, errors.New("unregistered channel outpoint")
	}
	if !rc.FundingConfirmed || rc.SplicePending {
		return 0, [32]byte{}, [32]byte{}, nil, errors.New("channel funding unconfirmed or splice pending")
	}
	switch edgeNodePub {
	case rc.Party0NodePub:
		return 0, rc.Party0ReceiptKey, rc.Party1ReceiptKey, rc, nil
	case rc.Party1NodePub:
		return 1, rc.Party1ReceiptKey, rc.Party0ReceiptKey, rc, nil
	default:
		return 0, [32]byte{}, [32]byte{}, nil, errors.New("edge node not a participant in channel")
	}
}

func (cr *ChannelRegistry) VerifyExpectedBindings(r *ReceiptV1) error {
	if r.Genesis != cr.Genesis || r.Policy != cr.Policy {
		return errors.New("trusted context mismatch: genesis/policy")
	}
	assetCh, ok := cr.Channels[r.AssetOutpoint]
	if !ok || !assetCh.IsAssetChannel || !assetCh.FundingConfirmed || assetCh.SplicePending {
		return errors.New("trusted context mismatch: asset channel invalid")
	}
	if assetCh.AssetID != r.Asset {
		return errors.New("trusted context mismatch: asset ID")
	}
	btcCh, ok := cr.Channels[r.BtcOutpoint]
	if !ok || btcCh.IsAssetChannel || !btcCh.FundingConfirmed || btcCh.SplicePending {
		return errors.New("trusted context mismatch: btc channel invalid")
	}
	wantAssetEdge, wantAssetPeer := assetCh.Party0ReceiptKey, assetCh.Party1ReceiptKey
	if r.AssetEdgeSide == 1 {
		wantAssetEdge, wantAssetPeer = assetCh.Party1ReceiptKey, assetCh.Party0ReceiptKey
	}
	if r.EdgeKey != wantAssetEdge || r.AssetPeerKey != wantAssetPeer {
		return errors.New("trusted context mismatch: asset channel keys/side")
	}
	wantBtcEdge, wantBtcPeer := btcCh.Party0ReceiptKey, btcCh.Party1ReceiptKey
	if r.BtcEdgeSide == 1 {
		wantBtcEdge, wantBtcPeer = btcCh.Party1ReceiptKey, btcCh.Party0ReceiptKey
	}
	if r.EdgeKey != wantBtcEdge || r.BtcPeerKey != wantBtcPeer {
		return errors.New("trusted context mismatch: btc channel keys/side")
	}
	return nil
}

// QuoteCapacityTracker reproduces tapd rfq/order.go's exact quote fill accounting:
// in-flight reservations + lifetime settled outgoing msat <= policyMaxOutMsat.
type QuoteCapacityTracker struct {
	PolicyMaxOutMsat map[[32]byte]uint64
	InFlightMsat     map[[32]byte]map[uint64]uint64
	SettledMsat      map[[32]byte]uint64
}

func NewQuoteCapacityTracker() *QuoteCapacityTracker {
	return &QuoteCapacityTracker{
		PolicyMaxOutMsat: make(map[[32]byte]uint64),
		InFlightMsat:     make(map[[32]byte]map[uint64]uint64),
		SettledMsat:      make(map[[32]byte]uint64),
	}
}

func (qt *QuoteCapacityTracker) RegisterQuote(rfqID [32]byte, maxOutMsat uint64) error {
	if maxOutMsat == 0 {
		return errors.New("quote maxOutMsat must be positive")
	}
	if prev, exists := qt.PolicyMaxOutMsat[rfqID]; exists && prev != maxOutMsat {
		return errors.New("quote cap altered across calls")
	}
	qt.PolicyMaxOutMsat[rfqID] = maxOutMsat
	if qt.InFlightMsat[rfqID] == nil {
		qt.InFlightMsat[rfqID] = make(map[uint64]uint64)
	}
	return nil
}

func (qt *QuoteCapacityTracker) ReserveInFlight(rfqID [32]byte, htlcKey uint64, htlcOutMsat uint64) error {
	maxOut, ok := qt.PolicyMaxOutMsat[rfqID]
	if !ok {
		return errors.New("unregistered quote ID")
	}
	if htlcOutMsat == 0 {
		return errors.New("zero htlcOutMsat")
	}
	var inFlight uint64
	for k, v := range qt.InFlightMsat[rfqID] {
		if k != htlcKey {
			inFlight += v
		}
	}
	total := inFlight + qt.SettledMsat[rfqID]
	if total+htlcOutMsat > maxOut {
		return fmt.Errorf("quote capacity exceeded: in_flight=%d settled=%d add=%d max=%d",
			inFlight, qt.SettledMsat[rfqID], htlcOutMsat, maxOut)
	}
	qt.InFlightMsat[rfqID][htlcKey] = htlcOutMsat
	return nil
}

func (qt *QuoteCapacityTracker) FailInFlight(rfqID [32]byte, htlcKey uint64) {
	if m, ok := qt.InFlightMsat[rfqID]; ok {
		delete(m, htlcKey)
	}
}

func (qt *QuoteCapacityTracker) SettleInFlight(rfqID [32]byte, htlcKey uint64) error {
	m, ok := qt.InFlightMsat[rfqID]
	if !ok {
		return errors.New("unregistered quote ID")
	}
	amt, ok := m[htlcKey]
	if !ok {
		return errors.New("htlc not reserved in flight")
	}
	delete(m, htlcKey)
	qt.SettledMsat[rfqID] += amt
	return nil
}

// ObservedLegSettlement captures what ONE channel participant directly observes
// and verifies on its own channel leg before co-signing ReceiptV1.
type ObservedLegSettlement struct {
	Outpoint             Outpoint36
	IsAssetLeg           bool
	AssetID              [32]byte
	OffererSide          uint8
	HtlcID               uint64
	PaymentHash          [32]byte
	FulfillmentPreimage  [32]byte
	PreStateCommitment   [32]byte
	PostStateCommitment  [32]byte
	AssetDeltaAtoms      uint64
	BtcDeltaMsat         uint64
	HtlcInterceptOutMsat uint64 // Native tapd policy-consumption unit (htlc.AmountOutMsat at interception).
	Settled              bool
	Failed               bool
	OnChainResolved      bool
	RevokedPriorState    bool
	SplicePending        bool
}

func (obs *ObservedLegSettlement) checkCommonFinality(wantHash [32]byte) error {
	if obs == nil {
		return errors.New("nil observed leg")
	}
	if obs.SplicePending {
		return errors.New("splice pending on observed leg")
	}
	if obs.OnChainResolved {
		return errors.New("on-chain HTLC resolution requires separate disposition; excluded from spot receipt")
	}
	if obs.Failed || !obs.Settled {
		return errors.New("HTLC not off-chain settled")
	}
	if !obs.RevokedPriorState {
		return errors.New("prior commitment state not durably revoked")
	}
	if obs.PreStateCommitment == obs.PostStateCommitment {
		return errors.New("unchanged commitment state on leg")
	}
	preHash := sha256.Sum256(obs.FulfillmentPreimage[:])
	if preHash != obs.PaymentHash || obs.PaymentHash != wantHash {
		return errors.New("payment hash / fulfillment preimage mismatch")
	}
	return nil
}

// PeerScopeAttestation explicitly documents which ReceiptV1 fields a signer
// independently verified from its own channel state vs merely acknowledged.
type PeerScopeAttestation struct {
	Role                       string   `json:"role"`
	SignatureSha256Hex         string   `json:"signature_sha256"`
	VerifiedFields             []string `json:"verified_fields"`
	AcknowledgedUnverifiedLegs []string `json:"acknowledged_unverified_fields"`
}

// SignAsAssetPeer signs ReceiptV1 with btcec/v2/schnorr ONLY after verifying the
// asset channel leg against the asset peer's local state and registry binding.
func SignAsAssetPeer(
	priv *btcec.PrivateKey,
	reg *ChannelRegistry,
	obs *ObservedLegSettlement,
	r *ReceiptV1,
) ([64]byte, *PeerScopeAttestation, error) {
	var sigOut [64]byte
	if priv == nil {
		return sigOut, nil, errors.New("nil asset peer private key")
	}
	var pubX [32]byte
	copy(pubX[:], schnorr.SerializePubKey(priv.PubKey()))
	if pubX != r.AssetPeerKey {
		return sigOut, nil, errors.New("asset peer key mismatch")
	}
	if err := reg.VerifyExpectedBindings(r); err != nil {
		return sigOut, nil, err
	}
	if err := obs.checkCommonFinality(r.PaymentHash); err != nil {
		return sigOut, nil, fmt.Errorf("asset peer finality check: %w", err)
	}
	if !obs.IsAssetLeg || obs.Outpoint != r.AssetOutpoint || obs.AssetID != r.Asset {
		return sigOut, nil, errors.New("asset peer channel/asset mismatch")
	}
	if obs.OffererSide != r.AssetOfferer || obs.HtlcID != r.AssetHtlc {
		return sigOut, nil, errors.New("asset peer HTLC identity mismatch")
	}
	if obs.PreStateCommitment != r.AssetPre || obs.PostStateCommitment != r.AssetPost {
		return sigOut, nil, errors.New("asset peer pre/post state commitment mismatch")
	}
	if obs.AssetDeltaAtoms != r.AssetAtoms {
		return sigOut, nil, fmt.Errorf("asset peer delta %d != receipt assetAtoms %d", obs.AssetDeltaAtoms, r.AssetAtoms)
	}
	dig, err := r.Digest()
	if err != nil {
		return sigOut, nil, err
	}
	sig, err := schnorr.Sign(priv, dig[:])
	if err != nil {
		return sigOut, nil, err
	}
	copy(sigOut[:], sig.Serialize())
	sigSum := sha256.Sum256(sigOut[:])
	att := &PeerScopeAttestation{
		Role:               "assetPeer",
		SignatureSha256Hex: hex.EncodeToString(sigSum[:]),
		VerifiedFields: []string{
			"genesis", "policy", "asset", "edgeKey", "assetPeerKey",
			"direction", "assetEdgeSide", "assetOutpoint", "assetOfferer",
			"assetHtlc", "assetPre", "assetPost", "assetAtoms", "paymentHash",
		},
		AcknowledgedUnverifiedLegs: []string{
			"btcPeerKey", "btcEdgeSide", "btcOutpoint", "btcOfferer",
			"btcHtlc", "btcPre", "btcPost", "grossBtcMsat",
			"rfqId", "quoteTranscript",
		},
	}
	return sigOut, att, nil
}

// SignAsBtcPeer signs ReceiptV1 with btcec/v2/schnorr ONLY after verifying the
// BTC channel leg against the BTC peer's local state and registry binding.
func SignAsBtcPeer(
	priv *btcec.PrivateKey,
	reg *ChannelRegistry,
	obs *ObservedLegSettlement,
	r *ReceiptV1,
) ([64]byte, *PeerScopeAttestation, error) {
	var sigOut [64]byte
	if priv == nil {
		return sigOut, nil, errors.New("nil btc peer private key")
	}
	var pubX [32]byte
	copy(pubX[:], schnorr.SerializePubKey(priv.PubKey()))
	if pubX != r.BtcPeerKey {
		return sigOut, nil, errors.New("btc peer key mismatch")
	}
	if err := reg.VerifyExpectedBindings(r); err != nil {
		return sigOut, nil, err
	}
	if err := obs.checkCommonFinality(r.PaymentHash); err != nil {
		return sigOut, nil, fmt.Errorf("btc peer finality check: %w", err)
	}
	if obs.IsAssetLeg || obs.Outpoint != r.BtcOutpoint {
		return sigOut, nil, errors.New("btc peer channel mismatch")
	}
	if obs.OffererSide != r.BtcOfferer || obs.HtlcID != r.BtcHtlc {
		return sigOut, nil, errors.New("btc peer HTLC identity mismatch")
	}
	if obs.PreStateCommitment != r.BtcPre || obs.PostStateCommitment != r.BtcPost {
		return sigOut, nil, errors.New("btc peer pre/post state commitment mismatch")
	}
	if obs.BtcDeltaMsat != r.GrossBtcMsat {
		return sigOut, nil, fmt.Errorf("btc peer delta %d != receipt grossBtcMsat %d", obs.BtcDeltaMsat, r.GrossBtcMsat)
	}
	dig, err := r.Digest()
	if err != nil {
		return sigOut, nil, err
	}
	sig, err := schnorr.Sign(priv, dig[:])
	if err != nil {
		return sigOut, nil, err
	}
	copy(sigOut[:], sig.Serialize())
	sigSum := sha256.Sum256(sigOut[:])
	att := &PeerScopeAttestation{
		Role:               "btcPeer",
		SignatureSha256Hex: hex.EncodeToString(sigSum[:]),
		VerifiedFields: []string{
			"genesis", "policy", "edgeKey", "btcPeerKey",
			"direction", "btcEdgeSide", "btcOutpoint", "btcOfferer",
			"btcHtlc", "btcPre", "btcPost", "grossBtcMsat", "paymentHash",
		},
		AcknowledgedUnverifiedLegs: []string{
			"asset", "assetPeerKey", "assetEdgeSide", "assetOutpoint",
			"assetOfferer", "assetHtlc", "assetPre", "assetPost",
			"assetAtoms", "rfqId", "quoteTranscript",
		},
	}
	return sigOut, att, nil
}

// SignAsEdge signs ReceiptV1 ONLY after verifying both settled legs, the signed
// request/accept quote transcript, and native quote-capacity compliance.
func SignAsEdge(
	priv *btcec.PrivateKey,
	reg *ChannelRegistry,
	sqt *SignedQuoteTranscript,
	nowUnix uint64,
	assetObs *ObservedLegSettlement,
	btcObs *ObservedLegSettlement,
	r *ReceiptV1,
) ([64]byte, *PeerScopeAttestation, error) {
	var sigOut [64]byte
	if priv == nil {
		return sigOut, nil, errors.New("nil edge private key")
	}
	var pubX [32]byte
	copy(pubX[:], schnorr.SerializePubKey(priv.PubKey()))
	if pubX != r.EdgeKey {
		return sigOut, nil, errors.New("edge key mismatch")
	}
	if err := reg.VerifyExpectedBindings(r); err != nil {
		return sigOut, nil, err
	}
	qtDig, err := sqt.Verify(nowUnix)
	if err != nil {
		return sigOut, nil, fmt.Errorf("edge quote transcript verification failed: %w", err)
	}
	if qtDig != r.QuoteTranscript || sqt.Transcript.RfqID != r.RfqID ||
		sqt.Transcript.AssetID != r.Asset || sqt.Transcript.Genesis != r.Genesis ||
		sqt.Transcript.Direction != r.Direction {
		return sigOut, nil, errors.New("edge quote transcript does not match receipt fields")
	}
	if err := assetObs.checkCommonFinality(r.PaymentHash); err != nil {
		return sigOut, nil, fmt.Errorf("edge asset leg finality: %w", err)
	}
	if err := btcObs.checkCommonFinality(r.PaymentHash); err != nil {
		return sigOut, nil, fmt.Errorf("edge btc leg finality: %w", err)
	}
	if assetObs.Outpoint != r.AssetOutpoint || assetObs.HtlcID != r.AssetHtlc ||
		assetObs.OffererSide != r.AssetOfferer || assetObs.AssetDeltaAtoms != r.AssetAtoms ||
		assetObs.PreStateCommitment != r.AssetPre || assetObs.PostStateCommitment != r.AssetPost {
		return sigOut, nil, errors.New("edge asset leg mismatch")
	}
	if btcObs.Outpoint != r.BtcOutpoint || btcObs.HtlcID != r.BtcHtlc ||
		btcObs.OffererSide != r.BtcOfferer || btcObs.BtcDeltaMsat != r.GrossBtcMsat ||
		btcObs.PreStateCommitment != r.BtcPre || btcObs.PostStateCommitment != r.BtcPost {
		return sigOut, nil, errors.New("edge btc leg mismatch")
	}
	dig, err := r.Digest()
	if err != nil {
		return sigOut, nil, err
	}
	sig, err := schnorr.Sign(priv, dig[:])
	if err != nil {
		return sigOut, nil, err
	}
	copy(sigOut[:], sig.Serialize())
	sigSum := sha256.Sum256(sigOut[:])
	att := &PeerScopeAttestation{
		Role:               "edge",
		SignatureSha256Hex: hex.EncodeToString(sigSum[:]),
		VerifiedFields: []string{
			"genesis", "policy", "asset", "edgeKey", "btcPeerKey", "assetPeerKey",
			"rfqId", "quoteTranscript", "paymentHash",
			"assetPre", "assetPost", "btcPre", "btcPost",
			"direction", "assetEdgeSide", "btcEdgeSide",
			"assetOutpoint", "assetOfferer", "assetHtlc",
			"btcOutpoint", "btcOfferer", "btcHtlc",
			"assetAtoms", "grossBtcMsat",
		},
		AcknowledgedUnverifiedLegs: []string{},
	}
	return sigOut, att, nil
}

type ThreePartySignatures struct {
	Edge      [64]byte
	BtcPeer   [64]byte
	AssetPeer [64]byte
}

type Tier1VerifiedReceipt struct {
	Tier             string `json:"tier"`
	EvidenceTier     uint8  `json:"evidence_tier"`
	DigestHex        string `json:"digest"`
	NullifierHex     string `json:"nullifier"`
	AssetAtoms       uint64 `json:"asset_atoms"`
	GrossBtcMsat     uint64 `json:"gross_btc_msat"`
	EligibleForIndex bool   `json:"eligible_for_index"`
}

// VerifyThreePartyReceipt validates expected registry bindings and all three
// BIP-340 Schnorr signatures using btcec/v2/schnorr.
func VerifyThreePartyReceipt(reg *ChannelRegistry, r *ReceiptV1, sigs ThreePartySignatures) (*Tier1VerifiedReceipt, error) {
	if reg == nil {
		return nil, errors.New("nil channel registry")
	}
	if err := reg.VerifyExpectedBindings(r); err != nil {
		return nil, err
	}
	dig, err := r.Digest()
	if err != nil {
		return nil, err
	}
	nul, err := r.Nullifier()
	if err != nil {
		return nil, err
	}
	pairs := []struct {
		role string
		pub  [32]byte
		sig  [64]byte
	}{
		{"edge", r.EdgeKey, sigs.Edge},
		{"btcPeer", r.BtcPeerKey, sigs.BtcPeer},
		{"assetPeer", r.AssetPeerKey, sigs.AssetPeer},
	}
	for _, p := range pairs {
		pk, err := schnorr.ParsePubKey(p.pub[:])
		if err != nil {
			return nil, fmt.Errorf("invalid %s pubkey: %w", p.role, err)
		}
		sg, err := schnorr.ParseSignature(p.sig[:])
		if err != nil || !sg.Verify(dig[:], pk) {
			return nil, fmt.Errorf("invalid %s signature", p.role)
		}
	}
	return &Tier1VerifiedReceipt{
		Tier:             "tier1_attested_only",
		EvidenceTier:     1,
		DigestHex:        hex.EncodeToString(dig[:]),
		NullifierHex:     hex.EncodeToString(nul[:]),
		AssetAtoms:       r.AssetAtoms,
		GrossBtcMsat:     r.GrossBtcMsat,
		EligibleForIndex: false,
	}, nil
}
