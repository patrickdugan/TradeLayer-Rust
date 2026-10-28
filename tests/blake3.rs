//! Official vectors and independently executed pinned portable-C comparison.
use tradelayer_core::blake3::blake3_256;
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn decode(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}
// BLAKE3-team/BLAKE3 df610ddc3b93841ffc59a87e3da659a15910eb46.
const OFFICIAL: &[(usize, &str)] = &[
    (
        0,
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
    ),
    (
        1,
        "2d3adedff11b61f14c886e35afa036736dcd87a74d27b5c1510225d0f592e213",
    ),
    (
        2,
        "7b7015bb92cf0b318037702a6cdd81dee41224f734684c2c122cd6359cb1ee63",
    ),
    (
        3,
        "e1be4d7a8ab5560aa4199eea339849ba8e293d55ca0a81006726d184519e647f",
    ),
    (
        4,
        "f30f5ab28fe047904037f77b6da4fea1e27241c5d132638d8bedce9d40494f32",
    ),
    (
        5,
        "b40b44dfd97e7a84a996a91af8b85188c66c126940ba7aad2e7ae6b385402aa2",
    ),
    (
        6,
        "06c4e8ffb6872fad96f9aaca5eee1553eb62aed0ad7198cef42e87f6a616c844",
    ),
    (
        7,
        "3f8770f387faad08faa9d8414e9f449ac68e6ff0417f673f602a646a891419fe",
    ),
    (
        8,
        "2351207d04fc16ade43ccab08600939c7c1fa70a5c0aaca76063d04c3228eaeb",
    ),
    (
        63,
        "e9bc37a594daad83be9470df7f7b3798297c3d834ce80ba85d6e207627b7db7b",
    ),
    (
        64,
        "4eed7141ea4a5cd4b788606bd23f46e212af9cacebacdc7d1f4c6dc7f2511b98",
    ),
    (
        65,
        "de1e5fa0be70df6d2be8fffd0e99ceaa8eb6e8c93a63f2d8d1c30ecb6b263dee",
    ),
    (
        127,
        "d81293fda863f008c09e92fc382a81f5a0b4a1251cba1634016a0f86a6bd640d",
    ),
    (
        128,
        "f17e570564b26578c33bb7f44643f539624b05df1a76c81f30acd548c44b45ef",
    ),
    (
        129,
        "683aaae9f3c5ba37eaaf072aed0f9e30bac0865137bae68b1fde4ca2aebdcb12",
    ),
    (
        1023,
        "10108970eeda3eb932baac1428c7a2163b0e924c9a9e25b35bba72b28f70bd11",
    ),
    (
        1024,
        "42214739f095a406f3fc83deb889744ac00df831c10daa55189b5d121c855af7",
    ),
    (
        1025,
        "d00278ae47eb27b34faecf67b4fe263f82d5412916c1ffd97c8cb7fb814b8444",
    ),
    (
        2048,
        "e776b6028c7cd22a4d0ba182a8bf62205d2ef576467e838ed6f2529b85fba24a",
    ),
    (
        2049,
        "5f4d72f40d7a5f82b15ca2b2e44b1de3c2ef86c426c95c1af0b6879522563030",
    ),
    (
        3072,
        "b98cb0ff3623be03326b373de6b9095218513e64f1ee2edd2525c7ad1e5cffd2",
    ),
    (
        3073,
        "7124b49501012f81cc7f11ca069ec9226cecb8a2c850cfe644e327d22d3e1cd3",
    ),
    (
        4096,
        "015094013f57a5277b59d8475c0501042c0b642e531b0a1c8f58d2163229e969",
    ),
    (
        4097,
        "9b4052b38f1c5fc8b1f9ff7ac7b27cd242487b3d890d15c96a1c25b8aa0fb995",
    ),
    (
        5120,
        "9cadc15fed8b5d854562b26a9536d9707cadeda9b143978f319ab34230535833",
    ),
    (
        5121,
        "628bd2cb2004694adaab7bbd778a25df25c47b9d4155a55f8fbd79f2fe154cff",
    ),
    (
        6144,
        "3e2e5b74e048f3add6d21faab3f83aa44d3b2278afb83b80b3c35164ebeca205",
    ),
    (
        6145,
        "f1323a8631446cc50536a9f705ee5cb619424d46887f3c376c695b70e0f0507f",
    ),
    (
        7168,
        "61da957ec2499a95d6b8023e2b0e604ec7f6b50e80a9678b89d2628e99ada77a",
    ),
    (
        7169,
        "a003fc7a51754a9b3c7fae0367ab3d782dccf28855a03d435f8cfe74605e7817",
    ),
    (
        8192,
        "aae792484c8efe4f19e2ca7d371d8c467ffb10748d8a5a1ae579948f718a2a63",
    ),
    (
        8193,
        "bab6c09cb8ce8cf459261398d2e7aef35700bf488116ceb94a36d0f5f1b7bc3b",
    ),
    (
        16384,
        "f875d6646de28985646f34ee13be9a576fd515f76b5b0a26bb324735041ddde4",
    ),
    (
        31744,
        "62b6960e1a44bcc1eb1a611a8d6235b6b4b78f32e7abc4fb4c6cdcce94895c47",
    ),
    (
        102400,
        "bc3e3d41a1146b069abffad3c0d44860cf664390afce4d9661f7902e7943e085",
    ),
];
// Independent official portable C, unmodified init/update/finalize.
const C_REFERENCE: &[(usize, u64, &str)] = &[
    (
        9,
        165,
        "5af65fbbe512c2ed5bc6987939c06f86292d4fad94b89c78824559134d9628c6",
    ),
    (
        31,
        172,
        "09fee5dd7060f46d9a4d5bfa934c3746ffd7e48691277d55bf3c60cf943d1198",
    ),
    (
        32,
        183,
        "086b910417ef6bbf92e901296d69dfc92612bbaf79df628740fbb63b47e5a6bc",
    ),
    (
        33,
        190,
        "39586f4d0285e237648b7e0f6e3e58400d8330e91d365fc99feaeceef0543f52",
    ),
    (
        55,
        129,
        "4856848c85f8bb9e96549542d0801b4b277e3f349f72f631bfe5927b3f5ea68b",
    ),
    (
        56,
        136,
        "04968fe916d1197e8ebefa3d0411d3ff4f0a36360d765d20cf4af1bfbf0c5ab0",
    ),
    (
        95,
        147,
        "7aec750810f152162c059af176d3251a71638f6ca673fab848d704c2c25e3477",
    ),
    (
        96,
        154,
        "678afaa760ba3bb35f468593ef274a5f11e75a934a465c4fba341a5ca17c44a0",
    ),
    (
        255,
        237,
        "7910ecfed0f15e64cfca5c2155ce1ee60bfd98baed375f7fc8ae4a83de40eebf",
    ),
    (
        256,
        244,
        "2ae32541141232eb7ef2b31c602daaea532fafe67ca05a4e68e376608959caae",
    ),
    (
        257,
        255,
        "06946155ddd25a81ad3fa0a419aa5df54df9500f9b2a8c0a19ff6f6e61abc78a",
    ),
    (
        511,
        198,
        "429c061ca550fde7503db8d4a8a99de192c22767f65b6245ca6780b6f6dcc081",
    ),
    (
        512,
        201,
        "5ef71a5ba33e51d64ff6e8046cba72d6fc11489cd145a0f2842489f0c3d14b19",
    ),
    (
        513,
        208,
        "e767405f78b3a2031790eb55330d69d61d175eccfafe0d7b94d1c2f024890a79",
    ),
    (
        1022,
        219,
        "0bd2f10f69042711182804321c1775ac10aad13d9662b49ec594c57ca14fb00e",
    ),
    (
        1026,
        34,
        "03c2e8f72781a75c83753926eb021aebdc740c561a738dc742e7d93a75ea7c0c",
    ),
    (
        2047,
        53,
        "0a012aa674917ba134b57907e562e68cd2a381c1ee1c48bea497bd1ee1cc43e1",
    ),
    (
        2050,
        60,
        "daef8329f16068a43f4673f62897d806b4c7ee12f8ccfba6ad94b74f6d750bea",
    ),
    (
        3071,
        7,
        "305f931f71d48108918c5809bc7e15477fdaf381eade3988e6389dbea5ec5190",
    ),
    (
        4095,
        14,
        "59f0837d0f6ebb64cf8c0aa067922b60ba7ae2160f3a118eaec7e89c3f5287aa",
    ),
    (
        8191,
        17,
        "e08751c57a1546e0924fe9cfe063f879b99b7c23904cd0153fd02f5ba0eadc33",
    ),
    (
        16383,
        24,
        "7eb496729eaad24b5fb021548a0a5b8ffd0f1e7027df204a1dac58b4d64b3de6",
    ),
    (
        16385,
        99,
        "b434dc9f12d179fcdfcb689cf7625fbd36f732e41f75fb4fe0a075248325e370",
    ),
    (
        32767,
        106,
        "6dfa3ac01c6d1856ceada6c1e2ca8d20f7273c076ea61c28becb01ff759fab2f",
    ),
    (
        32768,
        125,
        "1b0b9f8d3a265506917318e0b4ad056496496b7781a53dd448594ac6076c4bb4",
    ),
    (
        32769,
        68,
        "a2bd44b1ef7f5c067b515c7b92a36a080862c73973b9d89193ba37340a30d165",
    ),
    (
        65535,
        79,
        "20022a441beccb31aff268f8817db872ff1a46db2b719b1663bf221134ec673f",
    ),
    (
        65536,
        86,
        "15806894824344d704d089d21b2594097070d0856cf56891e14ae1f5367319f6",
    ),
    (
        65537,
        89,
        "2e67c14800d8d3449ba6556eb10043b846f177c4e06e3235eecb673b88a754e3",
    ),
    (
        1048577,
        416,
        "c45b007ed93b3c73b101d8d2f930cedd100ca0f598b34016b19ba34bb81d2be3",
    ),
];
// Core-produced canonical Kernel::Serialize preimages, not raw-span guesses.
const KERNELS: &[(&str,&str,&str,&str)] = &[
    ("pegin_envelope","139d3c83d384e516031c12ae0ab49e9da8078d7c96e8d957e8bc422d5df79aa6b72783f8786a6ac55408036da1264df90c50a67e1894f9fd2841016f5909b7670197677906e956f9494ade6c4ae55d8c6542fcd2629297ece8c85bddbe5a6041cd749695f420feda5a1ae07355770092d137bd393ba7e98f4a09bfb0f9d0f89bd20e2af63efdd9185600","1c3c0e4085ebce76eb6b2d50e81407907041a55c8c088d7cf802f94e934e8d59","598d4e934ef902f87c8d088c5ca54170900714e8502d6beb76ceeb85400e3c1c"),
    ("mweb_only_transfer","119d3c025ff22f12df584cfffb1c9e35b65eb1d57e9cb6f9776ef35915f54569f22dfa1e08c0e5122ecc5c170d3f09b4ed9bca7d803a86931143762605a6e35d71651a36f9d1708331b11efa52af75cf9d75387fa6fd69b1fcd172bd46c4970cbb0fa5839741c124a73e6f41ff38223b960fc6ead3afe416b9b3ba1dc6037c809baabd1301","aa21bef8b7b754dd68a792a7aa40b3202586ca221476d4df1825a4a6b0e82f32","322fe8b0a6a42518dfd4761422ca862520b340aaa792a768dd54b7b7f8be21aa"),
    ("pegout_envelope","15926c0183e1ac001976a914bd2dd046c3673a9b998ebedf73ef5a780d8d5c0588ac035fdd81fec50c57ee398009d7c83af3af9ee60c330170bfcf1c261805abb885ec0866604129ebbae5fdd117ca238389c8e3e33dfe6c4e062617a3698b693a68a5ba7c8f96e11c0e22a205dd1ec93589343a05739c00f4f7417dd75164211438df98556c660f27637e17f057e61122dc2f823659a7c6cc813e0de17d19969378d19a","79195e316cf4ea4fa5b48aeb354cf1b3211da6b273922484d244601e0e5af42c","2cf45a0e1e6044d284249273b2a61d21b3f14c35eb8ab4a54feaf46c315e1979"),
];
#[test]
fn all_official_empty_block_chunk_and_uneven_tree_vectors() {
    assert_eq!(OFFICIAL.len(), 35);
    for &(length, wanted) in OFFICIAL {
        let input: Vec<_> = (0..length).map(|i| (i % 251) as u8).collect();
        assert_eq!(hex(&blake3_256(&input)), wanted, "official length {length}");
    }
}
#[test]
fn independently_executed_c_boundaries_large_trees_and_unaligned_slices() {
    for &(length, seed, wanted) in C_REFERENCE {
        let input: Vec<_> = (0..length)
            .map(|i| {
                ((i as u64)
                    .wrapping_mul(0x9E3779B97F4A7C15)
                    .wrapping_add(seed)
                    .rotate_left(13)
                    >> 17) as u8
            })
            .collect();
        assert_eq!(
            hex(&blake3_256(&input)),
            wanted,
            "C length {length}, seed {seed}"
        );
        let mut padded = vec![0xA5; 3];
        padded.extend_from_slice(&input);
        padded.push(0x5A);
        assert_eq!(
            hex(&blake3_256(&padded[3..3 + length])),
            wanted,
            "borrowed unaligned length {length}"
        );
    }
}
#[test]
fn genuine_litecoin_kernel_preimages_match_natural_and_display_bytes() {
    for &(name, input, natural, display) in KERNELS {
        let bytes = decode(input);
        let digest = blake3_256(&bytes);
        assert_eq!(
            hex(&digest),
            natural,
            "{name} exact canonical serialization, no prefix"
        );
        assert_eq!(
            hex(&digest.into_iter().rev().collect::<Vec<_>>()),
            display,
            "{name} uint256 display reversal"
        );
    }
}
#[test]
fn independent_parallel_calls_preserve_exact_input_and_have_no_shared_state() {
    std::thread::scope(|scope| {
        for &(name, input, wanted, _) in KERNELS {
            scope.spawn(move || {
                let bytes = decode(input);
                let before = bytes.clone();
                for _ in 0..3 {
                    assert_eq!(hex(&blake3_256(&bytes)), wanted, "{name}");
                }
                assert_eq!(bytes, before);
            });
        }
    });
}
