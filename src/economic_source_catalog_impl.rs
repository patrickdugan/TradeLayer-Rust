// Actual source metadata owners are independently pinned at bootstrap. They
// never synthesize Ledger balances or replace authenticated L1 admission.
impl EconomicState {
    pub fn source_catalog_owners(&self) -> Option<&crate::source_catalogs::SourceCatalogOwners> {
        self.source_consensus.catalogs.as_ref()
    }

    pub fn from_joint_with_source_catalogs_unverified(
        config: EconomicConfig,
        joint: JointProtocolState,
        owners: crate::source_catalogs::SourceCatalogOwners,
    ) -> Result<Self> {
        config.validate()?;
        let source = config
            .source_consensus
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        let catalog_pin = source
            .catalog_bootstrap_commitment
            .ok_or(EconomicError::InvalidConfig)?;
        let catalog_bootstrap = owners
            .checkpoint_bytes()
            .map_err(|_| EconomicError::InvalidConfig)?;
        if crate::source_catalogs::bootstrap_commitment(&catalog_bootstrap) != catalog_pin
            || Self::source_bootstrap_commitment(&joint)? != source.bootstrap_commitment
        {
            return Err(EconomicError::InvalidConfig);
        }
        let bootstrap = joint
            .snapshot_for_economic_bytes()
            .map_err(|_| EconomicError::InvalidSnapshot)?;
        let mut base = config.clone();
        base.source_consensus = None;
        let mut state = Self::from_joint_unverified(base, joint)?;
        state.config = config;
        state.source_consensus = SourceConsensusState {
            bootstrap,
            frames: Vec::new(),
            catalog_bootstrap,
            catalogs: Some(owners),
        };
        state.verify()?;
        Ok(state)
    }

    fn source_catalog_bootstrap(
        &self,
    ) -> Result<Option<crate::source_catalogs::SourceCatalogOwners>> {
        let pin = self
            .config
            .source_consensus
            .as_ref()
            .and_then(|s| s.catalog_bootstrap_commitment);
        match pin {
            None if self.source_consensus.catalog_bootstrap.is_empty()
                && self.source_consensus.catalogs.is_none() =>
            {
                Ok(None)
            }
            Some(pin) => {
                if crate::source_catalogs::bootstrap_commitment(
                    &self.source_consensus.catalog_bootstrap,
                ) != pin
                {
                    return Err(EconomicError::Invariant);
                }
                let initial = crate::source_catalogs::SourceCatalogOwners::from_checkpoint_bytes(
                    &self.source_consensus.catalog_bootstrap,
                )
                .map_err(|_| EconomicError::InvalidSnapshot)?;
                self.source_consensus
                    .catalogs
                    .as_ref()
                    .ok_or(EconomicError::Invariant)?
                    .checkpoint_bytes()
                    .map_err(|_| EconomicError::InvalidSnapshot)?;
                Ok(Some(initial))
            }
            _ => Err(EconomicError::Invariant),
        }
    }

    fn source_metadata_ids(&self) -> Result<BTreeSet<TxId>> {
        if self.config.source_consensus.is_some() {
            if !self.catalog_inner_metadata_ids.is_empty() {
                return Err(EconomicError::Invariant);
            }
            return Self::metadata_ids_from_source(&self.source_consensus);
        }
        let frames = if self.config.native_entitlements.is_some() {
            Some(&self.native_entitlements.frames)
        } else if self.config.contract_binding.is_some() {
            Some(&self.contract_binding.frames)
        } else {
            None
        };
        if let Some(frames) = frames {
            if !self.catalog_inner_metadata_ids.is_empty() {
                return Err(EconomicError::Invariant);
            }
            let mut ids = BTreeSet::new();
            for input in frames.iter().flat_map(|frame| &frame.inputs) {
                if let EconomicInput::SourceMetadata { txid, ty } = input {
                    if !matches!(ty, 7..=9) || !ids.insert(*txid) {
                        return Err(EconomicError::Invariant);
                    }
                }
            }
            Ok(ids)
        } else {
            Ok(self.catalog_inner_metadata_ids.clone())
        }
    }

    fn metadata_ids_from_source(source: &SourceConsensusState) -> Result<BTreeSet<TxId>> {
        let mut ids = BTreeSet::new();
        if source.catalogs.is_none() {
            return Ok(ids);
        }
        for frame in &source.frames {
            for receipt in &frame.receipts {
                if receipt.disposition != SourceDisposition::Applied {
                    continue;
                }
                let tx = frame
                    .transactions
                    .iter()
                    .find(|tx| tx.position == receipt.position && tx.txid == receipt.txid)
                    .ok_or(EconomicError::Invariant)?;
                let metadata = match tx.transaction_type() {
                    Some(7 | 9) => true,
                    Some(8) => tx.payload.get(1..).and_then(|s| s.split(',').nth(1)) == Some("1"),
                    _ => false,
                };
                if metadata && !ids.insert(tx.txid) {
                    return Err(EconomicError::Invariant);
                }
            }
        }
        Ok(ids)
    }
}

impl EconomicBlockStage {
    pub(crate) fn retain_source_catalog_reads(
        &mut self,
        owners: Option<crate::source_catalogs::SourceCatalogOwners>,
    ) -> Result<()> {
        if owners.is_some() != self.state.source_consensus.catalogs.is_some() {
            return Err(EconomicError::Invariant);
        }
        self.state.source_consensus.catalogs = owners;
        Ok(())
    }

    /// The source runner has just recomputed this evaluation. Bind it to the
    /// original command before any real catalog mutation or Ledger call.
    pub(crate) fn try_apply_source_catalog_unverified(
        &mut self,
        arguments: &[SourceValue],
        evaluation: &crate::types_source::TypesEvaluation,
    ) -> Option<core::result::Result<(), SourceCatalogExecutionError>> {
        self.state.source_consensus.catalogs.as_ref()?;
        let ty = match arguments.get(1) {
            Some(SourceValue::Number(bits)) => f64::from_bits(*bits),
            _ => return None,
        };
        if !matches!(ty as u8, 1 | 7..=9 | 11 | 12) {
            return None;
        }
        let result = (|| {
            let (params, txid, actor, ty) =
                crate::types_economic::bound_source_params(arguments, evaluation, self.height)
                    .map_err(|e| SourceCatalogExecutionError::Mapping(format!("{e:?}")))?;
            if ty == 8
                && !catalog_truth(&params.get("whitelist"))
                && !catalog_truth(&params.get("token"))
                && catalog_truth(&params.get("oracle"))
            {
                return Ok(false);
            }
            let mut candidate = self.clone();
            let mut owners = candidate
                .state
                .source_consensus
                .catalogs
                .clone()
                .ok_or(EconomicError::Invariant)?;
            let get = |name| params.get(name);
            match ty {
                1 => {
                    let managed = catalog_nullish(get("managed"), get("isManaged"));
                    let nft = catalog_nullish(get("nft"), get("isNFT"));
                    let kind = if !matches!(
                        get("proceduralType"),
                        SourceValue::Undefined | SourceValue::Null
                    ) && catalog_truth(&managed)
                    {
                        "Procedural"
                    } else if catalog_truth(&nft) {
                        "Non-Fungible"
                    } else if catalog_truth(&managed) {
                        "Managed"
                    } else {
                        "Fixed"
                    };
                    let clearlist = if let SourceValue::Array(values) = get("whitelists") {
                        catalog_or(
                            values.first().cloned().unwrap_or(SourceValue::Undefined),
                            SourceValue::number(0.0),
                        )
                    } else {
                        catalog_or(get("whitelistId"), SourceValue::number(0.0))
                    };
                    let id = catalog_property_call(
                        &mut owners,
                        "createToken",
                        &[
                            get("ticker"),
                            get("initialAmount"),
                            SourceValue::String(kind.into()),
                            clearlist,
                            get("senderAddress"),
                            get("backupAddress"),
                            SourceValue::Object(vec![(
                                "proceduralType".into(),
                                get("proceduralType"),
                            )]),
                        ],
                        &mut CatalogNoTally,
                    )?;
                    let SourceValue::Number(bits) = id else {
                        return Err(SourceCatalogExecutionError::Mapping(
                            "source allocation is not a numeric property ID".into(),
                        ));
                    };
                    let n = f64::from_bits(bits);
                    if n < 1.0 || n > f64::from(u32::MAX) || n.fract() != 0.0 {
                        return Err(SourceCatalogExecutionError::Mapping(
                            "source allocation is outside Ledger property IDs".into(),
                        ));
                    }
                    let id =
                        PropertyId::try_from(n as u32).map_err(|_| EconomicError::InvalidInput)?;
                    let mapped = crate::types_economic::map_source_issuance_unverified(
                        arguments,
                        evaluation,
                        self.height,
                        id,
                    )
                    .map_err(|e| SourceCatalogExecutionError::Mapping(format!("{e:?}")))?;
                    if mapped.catalog_type != kind {
                        return Err(EconomicError::Invariant.into());
                    }
                    candidate.try_apply_unverified(&mapped.input)?;
                }
                7 => {
                    catalog_clearlist_call(
                        &mut owners,
                        "createClearlist",
                        vec![
                            get("senderAddress"),
                            get("name"),
                            get("url"),
                            get("description"),
                            get("backupAddress"),
                        ],
                    )?;
                    candidate.try_apply_unverified(&EconomicInput::SourceMetadata { txid, ty })?;
                }
                8 => {
                    if catalog_truth(&get("whitelist")) {
                        catalog_clearlist_call(
                            &mut owners,
                            "updateAdmin",
                            vec![get("id"), get("newAddress"), get("updateBackup")],
                        )?;
                    } else if catalog_truth(&get("token")) {
                        // The pinned Logic refers to PropertyList, while its
                        // imported owner is PropertyManager. Preserve the bug.
                        return Err(SourceCatalogExecutionError::Thrown(
                            crate::validity_source::SourceException {
                                name: "ReferenceError".into(),
                                message: "PropertyList is not defined".into(),
                            },
                        ));
                    }
                    candidate.try_apply_unverified(&EconomicInput::SourceMetadata { txid, ty })?;
                }
                9 => {
                    let id = get("id");
                    let is_zero =
                        matches!(id,SourceValue::Number(bits) if f64::from_bits(bits)==0.0);
                    let target = get("targetAddress");
                    let banlist = is_zero
                        && matches!(&target,SourceValue::String(s) if s.trim_matches(catalog_js_space).eq_ignore_ascii_case("BANLIST"));
                    if banlist {
                        if get("sender")
                            != SourceValue::String(
                                candidate
                                    .state
                                    .joint
                                    .registry()
                                    .protocol_admin_unverified()
                                    .as_str()
                                    .into(),
                            )
                        {
                            return Err(SourceCatalogExecutionError::Thrown(crate::validity_source::SourceException {
                                name:"Error".into(),message:"Only protocol admin can update global banlist via list id 0".into(),
                            }));
                        }
                        let values = match get("metaData") {
                            SourceValue::Array(values) => values,
                            SourceValue::String(s) => s
                                .split(|c: char| {
                                    c == ',' || c == ';' || c == '|' || catalog_js_space(c)
                                })
                                .filter(|s| !s.is_empty())
                                .map(|s| SourceValue::String(s.to_ascii_uppercase()))
                                .collect(),
                            SourceValue::Undefined
                            | SourceValue::Null
                            | SourceValue::Bool(false) => Vec::new(),
                            _ => return Err(SourceCatalogExecutionError::Rule(
                                crate::validity_source::SourceRuleError::UnsupportedNumericDomain,
                            )),
                        };
                        if values.iter().all(
                            |v| matches!(v,SourceValue::String(s) if s.encode_utf16().count()==2),
                        ) {
                            catalog_clearlist_call(
                                &mut owners,
                                "setBanlist",
                                vec![SourceValue::Array(values), get("block")],
                            )?;
                        }
                    } else if !catalog_truth(&get("revoke")) {
                        catalog_clearlist_call(
                            &mut owners,
                            "addAttestation",
                            vec![id.clone(), target.clone(), get("metaData"), get("block")],
                        )?;
                        if catalog_truth(&get("merkleRoot")) {
                            catalog_clearlist_call(
                                &mut owners,
                                "storeMerkleRoot",
                                vec![id, get("merkleRoot"), target, get("block")],
                            )?;
                        }
                    } else if get("revoke") == SourceValue::Bool(true) {
                        catalog_clearlist_call(
                            &mut owners,
                            "revokeAttestation",
                            vec![id, target, get("metaData"), get("block")],
                        )?;
                    }
                    candidate.try_apply_unverified(&EconomicInput::SourceMetadata { txid, ty })?;
                }
                11 | 12 => {
                    let admin = catalog_property_call(
                        &mut owners,
                        "isManagedAndAdmin",
                        &[get("propertyId"), get("senderAddress")],
                        &mut CatalogNoTally,
                    )?;
                    let property = catalog_property_call(
                        &mut owners,
                        "getPropertyData",
                        &[get("propertyId")],
                        &mut CatalogNoTally,
                    )?;
                    if matches!(property.get("type"),SourceValue::Number(bits) if f64::from_bits(bits)==7.0)
                    {
                        return Err(SourceCatalogExecutionError::Mapping(
                            "procedural issuance/redemption requires the actual procedural owner"
                                .into(),
                        ));
                    }
                    if !catalog_truth(&admin) {
                        return Err(SourceCatalogExecutionError::Thrown(
                            crate::validity_source::SourceException {
                                name: "Error".into(),
                                message: "Sender is not admin of a managed property".into(),
                            },
                        ));
                    }
                    let input = crate::types_economic::map_economic_input_unverified(
                        arguments,
                        evaluation,
                        self.height,
                        candidate.state.config.contract,
                    )
                    .map_err(|e| SourceCatalogExecutionError::Mapping(format!("{e:?}")))?;
                    let redeem = ty == 12;
                    let recipient = if redeem {
                        SourceValue::String(actor.as_str().into())
                    } else {
                        get("addressToGrantTo")
                    };
                    let amount = get(if redeem {
                        "amountDestroyed"
                    } else {
                        "amountGranted"
                    });
                    let signed = if redeem {
                        let SourceValue::Number(bits) = amount else {
                            return Err(EconomicError::InvalidInput.into());
                        };
                        SourceValue::number(-f64::from_bits(bits))
                    } else {
                        amount.clone()
                    };
                    let expected = crate::validity_source::DependencyCall {
                        target: "Tally.updateBalance".into(),
                        arguments: vec![
                            recipient.clone(),
                            get("propertyId"),
                            signed,
                            SourceValue::number(0.0),
                            SourceValue::number(0.0),
                            SourceValue::number(0.0),
                            SourceValue::String(
                                if redeem { "redeemToken" } else { "grantToken" }.into(),
                            ),
                            get("block"),
                        ],
                    };
                    let mut tally = CatalogTallyGate {
                        stage: &mut candidate,
                        input: &input,
                        expected,
                        applied: false,
                        failure: None,
                    };
                    let call_result = catalog_property_call(
                        &mut owners,
                        if redeem {
                            "redeemTokens"
                        } else {
                            "grantTokens"
                        },
                        &[get("propertyId"), recipient, amount, get("block")],
                        &mut tally,
                    );
                    if let Some(error) = tally.failure.take() {
                        return Err(error.into());
                    }
                    call_result?;
                    if !tally.applied {
                        return Err(EconomicError::Invariant.into());
                    }
                }
                _ => return Err(EconomicError::InvalidInput.into()),
            }
            candidate.state.source_consensus.catalogs = Some(owners);
            *self = candidate;
            Ok(true)
        })();
        match result {
            Ok(false) => None,
            Ok(true) => Some(Ok(())),
            Err(e) => Some(Err(e)),
        }
    }
}

pub(crate) enum SourceCatalogExecutionError {
    Mapping(String),
    Rule(crate::validity_source::SourceRuleError),
    Thrown(crate::validity_source::SourceException),
    Economic(EconomicError),
}
impl From<EconomicError> for SourceCatalogExecutionError {
    fn from(error: EconomicError) -> Self {
        Self::Economic(error)
    }
}
fn catalog_truth(value: &SourceValue) -> bool {
    match value {
        SourceValue::Undefined | SourceValue::Null | SourceValue::Bool(false) => false,
        SourceValue::Number(bits) => {
            let n = f64::from_bits(*bits);
            n != 0.0 && !n.is_nan()
        }
        SourceValue::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn catalog_js_space(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn catalog_nullish(value: SourceValue, fallback: SourceValue) -> SourceValue {
    if matches!(value, SourceValue::Undefined | SourceValue::Null) {
        fallback
    } else {
        value
    }
}
fn catalog_or(value: SourceValue, fallback: SourceValue) -> SourceValue {
    if catalog_truth(&value) {
        value
    } else {
        fallback
    }
}
fn catalog_property_call(
    owners: &mut crate::source_catalogs::SourceCatalogOwners,
    method: &str,
    args: &[SourceValue],
    backend: &mut impl crate::validity_source::SourceBackend,
) -> core::result::Result<SourceValue, SourceCatalogExecutionError> {
    let evaluation = owners
        .properties
        .execute(method, args, backend)
        .map_err(|e| SourceCatalogExecutionError::Rule(crate::source_catalogs::catalog_error(e)))?;
    match evaluation.outcome {
        crate::validity_source::SourceOutcome::Returned(value) => Ok(value),
        crate::validity_source::SourceOutcome::Thrown(error) => {
            Err(SourceCatalogExecutionError::Thrown(error))
        }
    }
}
fn catalog_clearlist_call(
    owners: &mut crate::source_catalogs::SourceCatalogOwners,
    method: &str,
    args: Vec<SourceValue>,
) -> core::result::Result<SourceValue, SourceCatalogExecutionError> {
    use crate::validity_source::DependencyReturn;
    match owners
        .clearlists
        .invoke_source(method, &args)
        .map_err(SourceCatalogExecutionError::Rule)?
    {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => Ok(v),
        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
            Err(SourceCatalogExecutionError::Thrown(e))
        }
    }
}
struct CatalogNoTally;
impl crate::validity_source::SourceBackend for CatalogNoTally {
    fn invoke(
        &mut self,
        _: &crate::validity_source::DependencyCall,
    ) -> core::result::Result<
        crate::validity_source::DependencyReturn,
        crate::validity_source::SourceRuleError,
    > {
        Err(crate::validity_source::SourceRuleError::UnknownDependency)
    }
}
struct CatalogTallyGate<'a> {
    stage: &'a mut EconomicBlockStage,
    input: &'a EconomicInput,
    expected: crate::validity_source::DependencyCall,
    applied: bool,
    failure: Option<EconomicError>,
}
impl crate::validity_source::SourceBackend for CatalogTallyGate<'_> {
    fn invoke(
        &mut self,
        call: &crate::validity_source::DependencyCall,
    ) -> core::result::Result<
        crate::validity_source::DependencyReturn,
        crate::validity_source::SourceRuleError,
    > {
        use crate::validity_source::{DependencyReturn, SourceRuleError};
        if self.applied || call != &self.expected {
            return Err(SourceRuleError::UnknownDependency);
        }
        if let Err(error) = self.stage.try_apply_unverified(self.input) {
            self.failure = Some(error);
            return Err(SourceRuleError::Backend);
        }
        self.applied = true;
        Ok(DependencyReturn::Pending(SourceValue::Undefined))
    }
}
