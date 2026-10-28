// Inputs are audited archived LF source bytes, not expected hash results.
const SOURCE_ACTIVATION_FILES: &[(&str, &str, &str)] = &[
    (
        "activation",
        include_str!("../fixtures/native_genesis/source/activation.js"),
        "268f08438eb80765587b8ce24d719f41de573648e108ef64df557faad870ea0e",
    ),
    (
        "amm",
        include_str!("../fixtures/legacy_amm_source.txt"),
        "bfb4f5d94c1d4754ae9a17b646c42c4ffe62e077590d0a184ac8ddc0b38255b1",
    ),
    (
        "channels",
        include_str!("../fixtures/legacy_channels_source.txt"),
        "b3fa22beab9a9fdb63246daa12582f86591a4217863240321f88fdff0160ec79",
    ),
    (
        "clearing",
        include_str!("../fixtures/legacy_clearing_source.txt"),
        "612d1ec49c1d5cc412d0072a194da4975c5232ba5294b81cd3097df11bff1528",
    ),
    (
        "clearlist",
        include_str!("../fixtures/legacy_clearlist_audit_reference.txt"),
        "fbdb702e5e89bac113226223f42bf762af13fff40dee4e0c4ab6cd4a50d451e1",
    ),
    (
        "consensus",
        include_str!("../fixtures/legacy_consensus_source.txt"),
        "af4c2fe7c37680be018dcb0df9f2cf8b348baac8aec1fe2335a220ce27dc0230",
    ),
    (
        "contractRegistry",
        include_str!("../fixtures/legacy_contract_registry_source.txt"),
        "d14c8db7d2a40471d51c5d51931eccb5a0d4f4380c9a0589d5eb1c0990636121",
    ),
    (
        "insurance",
        include_str!("../fixtures/insurance_owner_review/insurance.js"),
        "5abbe7934ca4c2f4180ed910989ae8f90d752bf9e17cb8944b8b49e33d03c39c",
    ),
    (
        "logic",
        include_str!("../fixtures/native_genesis/source/logic.js"),
        "f09d476e581d38cf7278f1424aadba7266f25e0fbc23763ceba4afa79c5b3e1d",
    ),
    (
        "main",
        include_str!("../fixtures/native_genesis/source/main.js"),
        "006d59ba3d28d802b0e0c5be2d9d661b58caf1c919a704b3f0d40bc9613c4009",
    ),
    (
        "marginMap",
        include_str!("../fixtures/legacy_margin_map_source.txt"),
        "ad5af4a639a50623319236cce42e8728027109ac3b8ae4616387798110cfb636",
    ),
    (
        "options",
        include_str!("../fixtures/legacy_options_source.txt"),
        "02277caaece5339a9ea0dac15437d14ce3fac8dc5ee92a727a0f544efcdbb703",
    ),
    (
        "oracle",
        include_str!("../fixtures/legacy_oracle_source.txt"),
        "70778913a22a323f64a1b2de0b28095a204fba113f39597dedb65938eeb30e3d",
    ),
    (
        "orderbook",
        include_str!("../fixtures/legacy_orderbook_source.txt"),
        "c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98",
    ),
    (
        "persistence",
        include_str!("../fixtures/legacy_persistence_source.txt"),
        "628c5b3cfd919fd518aedc2b757cdc256f7ccc36c447090f30aebd8a677ce2e1",
    ),
    (
        "property",
        include_str!("../fixtures/reference_fidelity/property.js"),
        "ef0ee7ba5c362227a8bfd44192e351c107871a93a3dc93b04ce6d1a5ebfc70b1",
    ),
    (
        "reOrg",
        include_str!("../fixtures/legacy_reorg_source.txt"),
        "3e4cabb0dc560fd8c601b72e05f0e698ce02adc646eaef241189fd192d906a82",
    ),
    (
        "tally",
        include_str!("../fixtures/native_genesis/source/tally.js"),
        "cfde9e5845f810bf62d9f1a07d4db4ea011b11d3d2bb63f041d39bea557cddef",
    ),
    (
        "txDecoder",
        include_str!("../fixtures/legacy_types_source_txDecoder.txt"),
        "a573e2ad3e843b2c573cb3f61c8d67b64ff05f1a554d36e3ad6bd5764d86d076",
    ),
    (
        "txIndex",
        include_str!("../fixtures/legacy_txindex_source_txIndex.txt"),
        "095449d90f511df44aff7b316105f57d4c0508ea8fb0b6f9c4a5f358edf6ca51",
    ),
    (
        "types",
        include_str!("../fixtures/legacy_types_source_types.txt"),
        "a49c1b1f846fe3eab91f4434c9f016586ffddf1bb3fb4492516a065df396a0e1",
    ),
    (
        "validity",
        include_str!("../fixtures/legacy_validity_source.txt"),
        "ed9b7fed5b733740c26033030379abe0f4a63d488ff431dfc7b1f10eb4f93b00",
    ),
    (
        "vaults",
        include_str!("../fixtures/legacy_vaults_source.txt"),
        "3dcf0554923864729bedcfc8244a06e4235d10a3241256eb3ba0a9d3d1534983",
    ),
    (
        "vesting",
        include_str!("../fixtures/native_genesis/source/vesting.js"),
        "4c134b6871f450d3190a0958b629731d960540069d0c8c37a6da33d6e1f70d2b",
    ),
    (
        "volumeIndex",
        include_str!("../fixtures/legacy_volume_index_source.txt"),
        "d4d4329254125ce8e4217d22b0ba57a1581c6279df999af60281b0365dffb08c",
    ),
];
fn activation_json_quote(value: &str) -> String {
    let mut output = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{0008}' => output.push_str("\\b"),
            '\u{000c}' => output.push_str("\\f"),
            value if (value as u32) < 32 => output.push_str(&format!("\\u{:04x}", value as u32)),
            value => output.push(value),
        }
    }
    output.push('"');
    output
}
impl ContractCore {
    fn source_activation_code_hash(&mut self) -> R<V> {
        let mut combined = String::new();
        for (name, raw, expected) in SOURCE_ACTIVATION_FILES {
            let normalized = raw.replace("\r\n", "\n");
            let actual = activation_sha_hex(normalized.as_bytes());
            if actual != *expected {
                return Err(SourceRuleError::InvalidInput.into());
            }
            if combined.len() + normalized.len() > 4 * 1024 * 1024 {
                return Err(SourceRuleError::Capacity.into());
            }
            self.emit(obj(&[
                ("kind", s("source-file")),
                ("method", s("existsSync")),
                ("name", s(*name)),
                ("returned", V::Bool(true)),
            ]));
            self.emit(obj(&[
                ("kind", s("source-file")),
                ("method", s("readFile")),
                ("name", s(*name)),
                ("sha256", s(&actual)),
            ]));
            combined.push_str(&normalized);
        }
        Ok(s(activation_sha_hex(
            activation_json_quote(&combined).as_bytes(),
        )))
    }
}
