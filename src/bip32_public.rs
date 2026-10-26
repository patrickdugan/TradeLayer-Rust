//! Dependency-free BIP32 public derivation and legacy P2PKH encoding.
//!
//! All elliptic-curve operands are public and the shared secp256k1 arithmetic
//! is variable-time. No seed, private key, signing, or hardened derivation API
//! is provided. See `docs/BIP32_PUBLIC.md` for source compatibility boundaries.

use crate::hash160::hash160;
use crate::hash256::double_sha256;
use crate::secp256k1::{canonical_compressed_public_key, public_key_tweak_add};

const HARDENED: u32 = 1 << 31;
const MAX_RETRIES: usize = 128;
const ORDER: [u8; 32] = [
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe,
    0xba, 0xae, 0xdc, 0xe6, 0xaf, 0x48, 0xa0, 0x3b, 0xbf, 0xd2, 0x5e, 0x8c, 0xd0, 0x36, 0x41, 0x41,
];
const FIELD_PRIME: [u8; 32] = [
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe, 0xff, 0xff, 0xfc, 0x2f,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bip32Error {
    InvalidLength,
    InvalidBase58,
    InvalidChecksum,
    UnsupportedVersion,
    UnsupportedPrivateKey,
    InvalidRootMetadata,
    InvalidPublicKey,
    HardenedChild,
    DepthExhausted,
    ChildIndexExhausted,
    RetryLimit,
    Arithmetic,
}
impl core::fmt::Display for Bip32Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Bip32Error {}

/// The two standard BIP32 public serialization versions. Litecoin's installed
/// source library uses these same versions; they do not identify a coin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicVersion {
    Mainnet,
    Testnet,
}
impl PublicVersion {
    fn bytes(self) -> [u8; 4] {
        match self {
            Self::Mainnet => [0x04, 0x88, 0xb2, 0x1e],
            Self::Testnet => [0x04, 0x35, 0x87, 0xcf],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressNetwork {
    BitcoinMainnet,
    LitecoinMainnet,
    Testnet,
}
impl AddressNetwork {
    fn prefix(self) -> u8 {
        match self {
            Self::BitcoinMainnet => 0,
            Self::LitecoinMainnet => 0x30,
            Self::Testnet => 0x6f,
        }
    }
}

/// Which dependency the JavaScript source successfully loaded. Its catch only
/// falls back when loading Litecoin fails, never when parsing an xpub fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLibrary {
    Litecoin,
    BitcoinFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtendedPublicKey {
    version: PublicVersion,
    depth: u8,
    parent_fingerprint: [u8; 4],
    child_number: u32,
    chain_code: [u8; 32],
    public_key: [u8; 33],
}
impl ExtendedPublicKey {
    /// Strict, bounded Base58Check parsing of a standard xpub or tpub. Rejects
    /// private versions, unsupported coin versions, invalid SEC1 points and
    /// nonzero root fingerprint/index. Non-root hardened *ancestry* is valid.
    pub fn parse(encoded: &str) -> Result<Self, Bip32Error> {
        Self::parse_inner(encoded, true)
    }

    fn parse_inner(encoded: &str, strict_root: bool) -> Result<Self, Bip32Error> {
        let data = decode_extended(encoded)?;
        let version = match data[..4] {
            [0x04, 0x88, 0xb2, 0x1e] => PublicVersion::Mainnet,
            [0x04, 0x35, 0x87, 0xcf] => PublicVersion::Testnet,
            [0x04, 0x88, 0xad, 0xe4] | [0x04, 0x35, 0x83, 0x94] => {
                return Err(Bip32Error::UnsupportedPrivateKey);
            }
            _ => return Err(Bip32Error::UnsupportedVersion),
        };
        let depth = data[4];
        let parent_fingerprint = data[5..9].try_into().unwrap();
        let child_number = u32::from_be_bytes(data[9..13].try_into().unwrap());
        if strict_root && depth == 0 && (parent_fingerprint != [0; 4] || child_number != 0) {
            return Err(Bip32Error::InvalidRootMetadata);
        }
        let mut public_key: [u8; 33] = data[45..78].try_into().unwrap();
        // elliptic's pointFromX reduces field coordinates in the source. Do
        // this only in the source wrapper, retaining strict SEC1 everywhere
        // else. Any 256-bit x needs at most one subtraction of p.
        if !strict_root && matches!(public_key[0], 2 | 3) {
            let mut x: [u8; 32] = public_key[1..].try_into().unwrap();
            if x >= FIELD_PRIME {
                subtract_modulus(&mut x, &FIELD_PRIME);
                public_key[1..].copy_from_slice(&x);
            }
        }
        let public_key = canonical_compressed_public_key(&public_key)
            .map_err(|_| Bip32Error::InvalidPublicKey)?;
        Ok(Self {
            version,
            depth,
            parent_fingerprint,
            child_number,
            chain_code: data[13..45].try_into().unwrap(),
            public_key,
        })
    }

    pub fn version(&self) -> PublicVersion {
        self.version
    }
    pub fn depth(&self) -> u8 {
        self.depth
    }
    pub fn parent_fingerprint(&self) -> [u8; 4] {
        self.parent_fingerprint
    }
    pub fn child_number(&self) -> u32 {
        self.child_number
    }
    pub fn chain_code(&self) -> [u8; 32] {
        self.chain_code
    }
    pub fn public_key(&self) -> [u8; 33] {
        self.public_key
    }
    pub fn fingerprint(&self) -> [u8; 4] {
        hash160(&self.public_key)[..4].try_into().unwrap()
    }

    pub fn to_base58(&self) -> String {
        let mut data = [0; 78];
        data[..4].copy_from_slice(&self.version.bytes());
        data[4] = self.depth;
        data[5..9].copy_from_slice(&self.parent_fingerprint);
        data[9..13].copy_from_slice(&self.child_number.to_be_bytes());
        data[13..45].copy_from_slice(&self.chain_code);
        data[45..].copy_from_slice(&self.public_key);
        encode_check(&data)
    }

    pub fn p2pkh_address(&self, network: AddressNetwork) -> String {
        let mut data = [0; 21];
        data[0] = network.prefix();
        data[1..].copy_from_slice(&hash160(&self.public_key));
        encode_check(&data)
    }

    /// CKDpub. Invalid IL >= n or infinity advances to the next non-hardened
    /// index, as BIP32 specifies. Zero IL is valid and leaves the point intact.
    /// The returned child_number records the index actually used. At most 128
    /// attempts are made; no retry crosses into hardened indices.
    pub fn derive_child(&self, index: u32) -> Result<Self, Bip32Error> {
        self.derive_inner(index, false, &mut hmac_sha512)
    }

    /// Installed bitcore CKDpub behavior: reduce IL modulo n, accept zero IL,
    /// retry infinity, and wrap the one-byte depth. This is deliberately
    /// separate from the strict BIP32 method. The same finite retry/index
    /// limits apply to both methods.
    pub fn derive_child_source_compatible(&self, index: u32) -> Result<Self, Bip32Error> {
        self.derive_inner(index, true, &mut hmac_sha512)
    }

    fn derive_inner(
        &self,
        mut index: u32,
        source_mode: bool,
        hmac: &mut impl FnMut(&[u8], &[u8]) -> [u8; 64],
    ) -> Result<Self, Bip32Error> {
        if index >= HARDENED {
            return Err(Bip32Error::HardenedChild);
        }
        let depth = if source_mode {
            self.depth.wrapping_add(1)
        } else {
            self.depth
                .checked_add(1)
                .ok_or(Bip32Error::DepthExhausted)?
        };
        let mut data = [0; 37];
        data[..33].copy_from_slice(&self.public_key);
        for attempt in 0..MAX_RETRIES {
            data[33..].copy_from_slice(&index.to_be_bytes());
            let digest = hmac(&self.chain_code, &data);
            let mut tweak: [u8; 32] = digest[..32].try_into().unwrap();
            // Installed bitcore multiplies without BIP32's IL >= n check;
            // its group multiplication therefore reduces modulo n. A single
            // subtraction suffices because IL < 2^256 < 2*n.
            if source_mode && tweak >= ORDER {
                subtract_modulus(&mut tweak, &ORDER);
            }
            if tweak < ORDER {
                let child = public_key_tweak_add(&self.public_key, &tweak)
                    .map_err(|_| Bip32Error::Arithmetic)?;
                if let Some(public_key) = child {
                    return Ok(Self {
                        version: self.version,
                        depth,
                        parent_fingerprint: self.fingerprint(),
                        child_number: index,
                        chain_code: digest[32..].try_into().unwrap(),
                        public_key,
                    });
                }
            }
            if index == HARDENED - 1 {
                return Err(Bip32Error::ChildIndexExhausted);
            }
            if attempt + 1 == MAX_RETRIES {
                return Err(Bip32Error::RetryLimit);
            }
            index += 1;
        }
        Err(Bip32Error::RetryLimit)
    }
}

fn subtract_modulus(value: &mut [u8; 32], modulus: &[u8; 32]) {
    let mut borrow = false;
    for i in (0..32).rev() {
        let (a, b1) = value[i].overflowing_sub(modulus[i]);
        let (b, b2) = a.overflowing_sub(u8::from(borrow));
        value[i] = b;
        borrow = b1 || b2;
    }
    debug_assert!(!borrow);
}

/// The source's installed-library behavior: Litecoin P2PKH addresses, receive
/// m/0/0..19 then change m/1/0..19. Its network argument is ignored. The caller
/// owns caching and can distinguish errors (which JavaScript does not cache).
pub fn derive_addresses_source_compatible(xpub: &str) -> Result<Vec<String>, Bip32Error> {
    derive_addresses_with_source_library(xpub, SourceLibrary::Litecoin)
}

/// Explicitly model the source's Bitcoin dependency fallback when Litecoin
/// cannot be loaded. Parsing failure never changes the selected library.
pub fn derive_addresses_with_source_library(
    xpub: &str,
    library: SourceLibrary,
) -> Result<Vec<String>, Bip32Error> {
    // bitcore accepts noncanonical root metadata and wraps one-byte depth.
    let root = ExtendedPublicKey::parse_inner(xpub, false)?;
    let network = match (root.version, library) {
        (PublicVersion::Testnet, _) => AddressNetwork::Testnet,
        (_, SourceLibrary::Litecoin) => AddressNetwork::LitecoinMainnet,
        (_, SourceLibrary::BitcoinFallback) => AddressNetwork::BitcoinMainnet,
    };
    let mut addresses = Vec::with_capacity(40);
    for chain in [0, 1] {
        let chain_key = root.derive_child_source_compatible(chain)?;
        for index in 0..20 {
            let child = chain_key.derive_child_source_compatible(index)?;
            addresses.push(child.p2pkh_address(network));
        }
    }
    Ok(addresses)
}

const BASE58: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// An xpub has exactly 82 decoded bytes and 111 encoded bytes with the only
/// two supported versions. Reject length before any input-dependent work or
/// allocation, and retain all decoded bytes in fixed storage.
fn decode_extended(encoded: &str) -> Result<[u8; 78], Bip32Error> {
    if encoded.len() != 111 {
        return Err(Bip32Error::InvalidLength);
    }
    let mut bytes = [0u8; 82];
    for digit in encoded.bytes() {
        let value = BASE58
            .iter()
            .position(|b| *b == digit)
            .ok_or(Bip32Error::InvalidBase58)?;
        let mut carry = value as u32;
        for byte in bytes.iter_mut().rev() {
            carry += u32::from(*byte) * 58;
            *byte = carry as u8;
            carry >>= 8;
        }
        if carry != 0 {
            return Err(Bip32Error::InvalidLength);
        }
    }
    let leading_digits = encoded.bytes().take_while(|b| *b == b'1').count();
    let leading_bytes = bytes.iter().take_while(|b| **b == 0).count();
    if leading_digits != leading_bytes {
        return Err(Bip32Error::InvalidLength);
    }
    let data: [u8; 78] = bytes[..78].try_into().unwrap();
    if double_sha256(&data)[..4] != bytes[78..] {
        return Err(Bip32Error::InvalidChecksum);
    }
    Ok(data)
}

fn encode_check(data: &[u8]) -> String {
    // Only fixed-size extended keys (78) and P2PKH payloads (21) call this.
    debug_assert!(data.len() <= 78);
    let mut bytes = [0; 82];
    let length = data.len() + 4;
    bytes[..data.len()].copy_from_slice(data);
    bytes[data.len()..length].copy_from_slice(&double_sha256(data)[..4]);
    let leading = bytes[..length].iter().take_while(|b| **b == 0).count();
    let mut digits = [0u8; 112];
    let mut used = 0;
    let mut start = leading;
    while start < length {
        let mut remainder = 0u32;
        for byte in &mut bytes[start..length] {
            let value = (remainder << 8) | u32::from(*byte);
            *byte = (value / 58) as u8;
            remainder = value % 58;
        }
        digits[used] = BASE58[remainder as usize];
        used += 1;
        while start < length && bytes[start] == 0 {
            start += 1;
        }
    }
    let mut output = String::with_capacity(leading + used);
    for _ in 0..leading {
        output.push('1');
    }
    for digit in digits[..used].iter().rev() {
        output.push(char::from(*digit));
    }
    output
}

const SHA512_K: [u64; 80] = [
    0x428a2f98d728ae22,
    0x7137449123ef65cd,
    0xb5c0fbcfec4d3b2f,
    0xe9b5dba58189dbbc,
    0x3956c25bf348b538,
    0x59f111f1b605d019,
    0x923f82a4af194f9b,
    0xab1c5ed5da6d8118,
    0xd807aa98a3030242,
    0x12835b0145706fbe,
    0x243185be4ee4b28c,
    0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f,
    0x80deb1fe3b1696b1,
    0x9bdc06a725c71235,
    0xc19bf174cf692694,
    0xe49b69c19ef14ad2,
    0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5,
    0x240ca1cc77ac9c65,
    0x2de92c6f592b0275,
    0x4a7484aa6ea6e483,
    0x5cb0a9dcbd41fbd4,
    0x76f988da831153b5,
    0x983e5152ee66dfab,
    0xa831c66d2db43210,
    0xb00327c898fb213f,
    0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2,
    0xd5a79147930aa725,
    0x06ca6351e003826f,
    0x142929670a0e6e70,
    0x27b70a8546d22ffc,
    0x2e1b21385c26c926,
    0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df,
    0x650a73548baf63de,
    0x766a0abb3c77b2a8,
    0x81c2c92e47edaee6,
    0x92722c851482353b,
    0xa2bfe8a14cf10364,
    0xa81a664bbc423001,
    0xc24b8b70d0f89791,
    0xc76c51a30654be30,
    0xd192e819d6ef5218,
    0xd69906245565a910,
    0xf40e35855771202a,
    0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8,
    0x1e376c085141ab53,
    0x2748774cdf8eeb99,
    0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63,
    0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc,
    0x78a5636f43172f60,
    0x84c87814a1f0ab72,
    0x8cc702081a6439ec,
    0x90befffa23631e28,
    0xa4506cebde82bde9,
    0xbef9a3f7b2c67915,
    0xc67178f2e372532b,
    0xca273eceea26619c,
    0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e,
    0xf57d4f7fee6ed178,
    0x06f067aa72176fba,
    0x0a637dc5a2c898a6,
    0x113f9804bef90dae,
    0x1b710b35131c471b,
    0x28db77f523047d84,
    0x32caab7b40c72493,
    0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6,
    0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec,
    0x6c44198c4a475817,
];

fn sha512_compress(state: &mut [u64; 8], block: &[u8]) {
    let mut words = [0u64; 80];
    for (word, bytes) in words[..16].iter_mut().zip(block.chunks_exact(8)) {
        *word = u64::from_be_bytes(bytes.try_into().unwrap());
    }
    for i in 16..80 {
        let x = words[i - 15];
        let y = words[i - 2];
        let a = x.rotate_right(1) ^ x.rotate_right(8) ^ (x >> 7);
        let b = y.rotate_right(19) ^ y.rotate_right(61) ^ (y >> 6);
        words[i] = words[i - 16]
            .wrapping_add(a)
            .wrapping_add(words[i - 7])
            .wrapping_add(b);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..80 {
        let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
        let ch = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(SHA512_K[i])
            .wrapping_add(words[i]);
        let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (word, addend) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *word = word.wrapping_add(addend);
    }
}

fn sha512_parts(parts: &[&[u8]]) -> [u8; 64] {
    let mut state = [
        0x6a09e667f3bcc908,
        0xbb67ae8584caa73b,
        0x3c6ef372fe94f82b,
        0xa54ff53a5f1d36f1,
        0x510e527fade682d1,
        0x9b05688c2b3e6c1f,
        0x1f83d9abfb41bd6b,
        0x5be0cd19137e2179,
    ];
    let mut block = [0; 128];
    let mut used = 0;
    let mut length = 0u128;
    for part in parts {
        length += part.len() as u128;
        let mut remaining = *part;
        while !remaining.is_empty() {
            let amount = (128 - used).min(remaining.len());
            block[used..used + amount].copy_from_slice(&remaining[..amount]);
            remaining = &remaining[amount..];
            used += amount;
            if used == 128 {
                sha512_compress(&mut state, &block);
                used = 0;
            }
        }
    }
    block[used] = 0x80;
    block[used + 1..].fill(0);
    if used >= 112 {
        sha512_compress(&mut state, &block);
        block.fill(0);
    }
    block[112..].copy_from_slice(&(length * 8).to_be_bytes());
    sha512_compress(&mut state, &block);
    let mut out = [0; 64];
    for (bytes, word) in out.chunks_exact_mut(8).zip(state) {
        bytes.copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn hmac_sha512(key: &[u8], data: &[u8]) -> [u8; 64] {
    let mut key_block = [0; 128];
    if key.len() > 128 {
        key_block[..64].copy_from_slice(&sha512_parts(&[key]));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner = [0x36; 128];
    let mut outer = [0x5c; 128];
    for i in 0..128 {
        inner[i] ^= key_block[i];
        outer[i] ^= key_block[i];
    }
    sha512_parts(&[&outer, &sha512_parts(&[&inner, data])])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(text: &str) -> Vec<u8> {
        if text == "-" {
            return Vec::new();
        }
        text.as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }
    fn hex(data: &[u8]) -> String {
        data.iter().map(|b| format!("{b:02x}")).collect()
    }
    fn fixture_rows(kind: char) -> Vec<Vec<&'static str>> {
        include_str!("../fixtures/bip32_public_vectors.tsv")
            .lines()
            .filter(|line| line.starts_with(kind) && line.as_bytes().get(1) == Some(&b'\t'))
            .map(|line| line.split('\t').collect())
            .collect()
    }
    fn generator_key() -> ExtendedPublicKey {
        ExtendedPublicKey::parse(fixture_rows('C')[0][3]).unwrap()
    }
    fn digest(tweak: [u8; 32], chain: u8) -> [u8; 64] {
        let mut out = [chain; 64];
        out[..32].copy_from_slice(&tweak);
        out
    }

    #[test]
    fn openssl_sha512_padding_and_hmac_key_boundaries() {
        let rows = fixture_rows('H');
        assert_eq!(rows.len(), 17);
        for row in rows {
            let data = bytes(row[2]);
            assert_eq!(hex(&sha512_parts(&[&data])), row[3], "{}", row[1]);
            for stride in [1, 7, 64, 111, 128, 129] {
                let parts: Vec<_> = data.chunks(stride).collect();
                assert_eq!(
                    hex(&sha512_parts(&parts)),
                    row[3],
                    "{} stride {stride}",
                    row[1]
                );
            }
        }
        let rows = fixture_rows('M');
        assert_eq!(rows.len(), 68);
        for row in rows {
            assert_eq!(
                hex(&hmac_sha512(&bytes(row[2]), &bytes(row[3]))),
                row[4],
                "{}",
                row[1]
            );
        }
        // RFC4231 section 4.2 (test case 1), a published external HMAC vector.
        assert_eq!(
            hex(&hmac_sha512(&[0x0b; 20], b"Hi There")),
            "87aa7cdea5ef619d4ff0b4241a1d6cb02379f4e2ce4ec2787ad0b30545e17cde\
             daa833b7d6b8a702038b274eaea3f4e4be9d914eeb61f1702e696c203a126854"
        );
    }

    #[test]
    fn genuine_source_forced_hmac_boundary_cases_match() {
        let rows = fixture_rows('C');
        assert_eq!(rows.len(), 10);
        for row in rows {
            let parent = ExtendedPublicKey::parse(row[3]).unwrap();
            let first: [u8; 32] = bytes(row[4]).try_into().unwrap();
            let mut indices = Vec::new();
            let mut hmac = |key: &[u8], data: &[u8]| {
                assert_eq!(key, parent.chain_code);
                assert_eq!(&data[..33], parent.public_key);
                indices.push(u32::from_be_bytes(data[33..].try_into().unwrap()));
                let tweak = if indices.len() == 1 {
                    first
                } else {
                    let mut one = [0; 32];
                    one[31] = 1;
                    one
                };
                digest(tweak, indices.len() as u8)
            };
            let child = parent.derive_inner(7, true, &mut hmac).unwrap();
            assert_eq!(child.to_base58(), row[6], "{} {}", row[1], row[2]);
            assert_eq!(child.child_number, row[5].parse::<u32>().unwrap());
            assert_eq!(indices.len(), row[7].parse::<usize>().unwrap());
            assert_eq!(
                indices,
                if row[2] == "infinity" {
                    vec![7, 8]
                } else {
                    vec![7]
                }
            );
        }
    }

    #[test]
    fn strict_invalid_il_and_infinity_retry_but_zero_is_valid() {
        let parent = generator_key();
        let mut one = [0; 32];
        one[31] = 1;
        let mut order_minus_one = ORDER;
        order_minus_one[31] -= 1;
        for first in [ORDER, [0xff; 32], order_minus_one] {
            let mut calls = Vec::new();
            let mut hmac = |_: &[u8], data: &[u8]| {
                calls.push(u32::from_be_bytes(data[33..].try_into().unwrap()));
                digest(
                    if calls.len() == 1 { first } else { one },
                    calls.len() as u8,
                )
            };
            let child = parent.derive_inner(7, false, &mut hmac).unwrap();
            assert_eq!(calls, [7, 8]);
            assert_eq!(child.child_number, 8);
            assert_eq!(child.chain_code, [2; 32]);
            assert_eq!(
                child.public_key,
                public_key_tweak_add(&parent.public_key, &one)
                    .unwrap()
                    .unwrap()
            );
        }
        let mut hmac = |_: &[u8], _: &[u8]| digest([0; 32], 9);
        let child = parent.derive_inner(7, false, &mut hmac).unwrap();
        assert_eq!(child.public_key, parent.public_key);
        assert_eq!(child.chain_code, [9; 32]);
        assert_eq!(child.child_number, 7);
        assert_eq!(child.depth, 1);
        assert_eq!(child.parent_fingerprint, parent.fingerprint());
    }

    #[test]
    fn retries_and_hardened_boundary_have_finite_limits() {
        let parent = generator_key();
        let mut calls = 0;
        let mut hmac = |_: &[u8], _: &[u8]| {
            calls += 1;
            digest(ORDER, 0)
        };
        assert_eq!(
            parent.derive_inner(0, false, &mut hmac),
            Err(Bip32Error::RetryLimit)
        );
        assert_eq!(calls, 128);
        let mut calls = 0;
        let mut hmac = |_: &[u8], _: &[u8]| {
            calls += 1;
            digest(ORDER, 0)
        };
        assert_eq!(
            parent.derive_inner(HARDENED - 1, false, &mut hmac),
            Err(Bip32Error::ChildIndexExhausted)
        );
        assert_eq!(calls, 1);
        let mut hmac = |_: &[u8], _: &[u8]| panic!("No HMAC for rejected index/depth");
        assert_eq!(
            parent.derive_inner(HARDENED, false, &mut hmac),
            Err(Bip32Error::HardenedChild)
        );
        assert_eq!(
            parent.derive_inner(u32::MAX, false, &mut hmac),
            Err(Bip32Error::HardenedChild)
        );
        let mut deepest = parent;
        deepest.depth = 255;
        assert_eq!(
            deepest.derive_inner(0, false, &mut hmac),
            Err(Bip32Error::DepthExhausted)
        );
    }

    #[test]
    fn valid_checksum_does_not_bypass_curve_and_encoding_checks() {
        let root = generator_key();
        let mut raw = decode_extended(&root.to_base58()).unwrap();
        let invalid_points = [
            [0; 33],
            {
                let mut k = [0; 33];
                k[0] = 2;
                k
            },
            {
                let mut k = [0; 33];
                k[0] = 2;
                k[32] = 7;
                k
            },
            {
                let mut k = [0xff; 33];
                k[0] = 2;
                k
            },
            {
                let mut k = [0xff; 33];
                k[0] = 2;
                k[29..].copy_from_slice(&[0xff, 0xff, 0xfc, 0x2f]);
                k[28] = 0xfe;
                k
            },
            {
                let mut k = root.public_key;
                k[0] = 4;
                k
            },
            {
                let mut k = root.public_key;
                k[0] = 6;
                k
            },
        ];
        for point in invalid_points {
            raw[45..].copy_from_slice(&point);
            assert_eq!(
                ExtendedPublicKey::parse(&encode_check(&raw)),
                Err(Bip32Error::InvalidPublicKey)
            );
        }
    }
}
