// MainCallableV8 only. The same Core collections and four true singleton Maps
// own ClearList metadata; there is no duplicate catalog or spendable balance.
fn cl_key(value: &V) -> R<()> {
    if matches!(
        value,
        V::Object(_) | V::Array(_) | V::BigNumber(_) | V::Promise
    ) {
        Err(SourceRuleError::UnsupportedNumericDomain.into())
    } else {
        Ok(())
    }
}
fn cl_or(values: &[V]) -> V {
    values
        .iter()
        .find(|v| truth(v))
        .cloned()
        .unwrap_or_else(|| values.last().cloned().unwrap_or(V::Undefined))
}
fn cl_prop(value: &V, key: &str) -> R<V> {
    registry_field(value, key)
}
fn cl_set(value: &mut V, key: &str, next: V) -> R<()> {
    if matches!(value, V::Null | V::Undefined) {
        return Err(type_error(format!(
            "Cannot set properties of {} (setting '{key}')",
            text(value)?
        )));
    }
    if !matches!(value, V::Object(_)) {
        return Err(type_error(format!(
            "Cannot create property '{key}' on {} '{}'",
            match value {
                V::String(_) => "string",
                V::Number(_) => "number",
                V::Bool(_) => "boolean",
                _ => return Err(SourceRuleError::UnsupportedNumericDomain.into()),
            },
            text(value)?
        )));
    }
    set(value, key, next)
}
impl ContractCore {
    fn source_clearlist_configure_library(&mut self, a: &[V]) -> R<V> {
        if a.len() != 1 || self.source_clearlist_library_locked {
            return Err(SourceRuleError::InvalidInput.into());
        }
        self.source_clearlist_library = match arg(a, 0) {
            V::String(s) if s == "litecoin" => crate::bip32_public::SourceLibrary::Litecoin,
            V::String(s) if s == "bitcoin-fallback" => {
                crate::bip32_public::SourceLibrary::BitcoinFallback
            }
            _ => return Err(SourceRuleError::InvalidInput.into()),
        };
        Ok(V::Undefined)
    }
    fn source_clearlist_state(&self) -> V {
        let pairs = |map: &[(V, V)]| {
            V::Array(
                map.iter()
                    .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                    .collect(),
            )
        };
        obj(&[
            ("clearlistsMap", pairs(&self.source_clearlist_lists)),
            ("banlist", self.source_clearlist_banlist.clone()),
            ("xpubCache", pairs(&self.source_clearlist_xpub_cache)),
            ("merkleXpubMap", pairs(&self.source_clearlist_roots)),
            (
                "libraryLocked",
                V::Bool(self.source_clearlist_library_locked),
            ),
            (
                "library",
                s(match self.source_clearlist_library {
                    crate::bip32_public::SourceLibrary::Litecoin => "litecoin",
                    crate::bip32_public::SourceLibrary::BitcoinFallback => "bitcoin-fallback",
                }),
            ),
        ])
    }
    fn source_clearlist_find(&mut self, collection: &str, query: V, one: bool) -> R<V> {
        self.db(
            collection,
            if one { "findOneAsync" } else { "findAsync" },
            vec![query],
        )
    }
    fn source_clearlist_update(&mut self, collection: &str, key: V, data: V, upsert: bool) -> R<V> {
        cl_key(&key)?;
        let mut args = vec![obj(&[("_id", key)]), obj(&[("$set", data)])];
        // Source revocation omits the options argument entirely.
        if upsert {
            args.push(obj(&[("upsert", V::Bool(true))]));
        }
        self.db(collection, "updateAsync", args)
    }
    fn source_clearlist_derive(&mut self, key: V) -> R<V> {
        cl_key(&key)?;
        self.source_clearlist_library_locked = true;
        if let Some(value) = oracle_source_map_get(&self.source_clearlist_xpub_cache, &key) {
            return Ok(value.clone());
        }
        let V::String(encoded) = &key else {
            return Ok(V::Array(vec![]));
        };
        match crate::bip32_public::derive_addresses_with_source_library(
            encoded,
            self.source_clearlist_library,
        ) {
            Ok(addresses) => {
                let value = V::Array(addresses.into_iter().map(V::String).collect());
                oracle_source_map_set(&mut self.source_clearlist_xpub_cache, key, value.clone())?;
                Ok(value)
            }
            Err(
                crate::bip32_public::Bip32Error::UnsupportedPrivateKey
                | crate::bip32_public::Bip32Error::Arithmetic,
            ) => Err(SourceRuleError::UnsupportedNumericDomain.into()),
            Err(
                crate::bip32_public::Bip32Error::RetryLimit
                | crate::bip32_public::Bip32Error::ChildIndexExhausted,
            ) => Err(SourceRuleError::Capacity.into()),
            Err(_) => Ok(V::Array(vec![])),
        }
    }
    fn source_clearlist_country(&mut self, address: V) -> R<V> {
        let result = (|| -> R<V> {
            self.database("attestations")?;
            for att in array(&self.source_clearlist_find(
                "attestations",
                obj(&[("data.address", address.clone())]),
                false,
            )?)? {
                let row = cl_or(&[att.get("data"), obj(&[])]);
                let status = cl_or(&[row.get("status"), s("active")]);
                let list = if row.get("listId") == V::Undefined {
                    row.get("clearlistId")
                } else {
                    row.get("listId")
                };
                // The source converts even a revoked row's list id before
                // checking status; a conversion throw is caught by this method.
                let list_id = lower_oracle_number(&list)?;
                if status != s("active") || list_id != 0.0 {
                    continue;
                }
                let meta = row.get("data");
                let code = if matches!(meta, V::String(_)) {
                    meta
                } else if truth(&meta)
                    && matches!(
                        meta,
                        V::Object(_) | V::Array(_) | V::BigNumber(_) | V::Promise
                    )
                {
                    cl_or(&[
                        meta.get("countryCode"),
                        meta.get("country"),
                        meta.get("code"),
                        meta.get("value"),
                    ])
                } else {
                    V::Undefined
                };
                if let V::String(code) = code {
                    let code = code.trim_matches(funding_js_space);
                    if !code.is_empty() {
                        return Ok(obj(&[
                            ("address", address),
                            ("countryCode", s(code.to_uppercase())),
                            (
                                "blockHeight",
                                cl_or(&[row.get("blockHeight"), row.get("timestamp"), V::Null]),
                            ),
                        ]));
                    }
                }
            }
            Ok(V::Null)
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Null),
            other => other,
        }
    }
    fn source_clearlist_call(&mut self, method: &str, a: &[V]) -> R<V> {
        let first = arg(a, 0);
        let second = arg(a, 1);
        match method {
            "loadClearlists" => {
                let result = (|| -> R<V> {
                    self.database("clearlists")?;
                    let rows = self.source_clearlist_find("clearlists", obj(&[]), false)?;
                    for row in array(&rows)? {
                        let key = cl_prop(&row, "_id")?;
                        cl_key(&key)?;
                        oracle_source_map_set(
                            &mut self.source_clearlist_lists,
                            key,
                            cl_prop(&row, "data")?,
                        )?;
                    }
                    Ok(rows)
                })();
                match result {
                    Err(Fault::Thrown(_)) => Ok(V::Undefined),
                    other => other,
                }
            }
            "getNextId" => {
                self.call("ClearList.loadClearlists", vec![])?;
                let mut max = 0.0;
                for (key, _) in &self.source_clearlist_lists {
                    let id = registry_parse_int(key)?;
                    if id > max {
                        max = id;
                    }
                }
                Ok(n(max + 1.0))
            }
            "createClearlist" => {
                let mut id = arg(a, 5);
                if !truth(&id) {
                    id = self.call("ClearList.getNextId", vec![])?;
                }
                let data = obj(&[
                    ("id", id.clone()),
                    ("admin", first),
                    ("name", second),
                    ("description", arg(a, 3)),
                    ("backup", arg(a, 4)),
                ]);
                self.database("clearlists")?;
                self.source_clearlist_update(
                    "clearlists",
                    id,
                    obj(&[("data", data.clone())]),
                    true,
                )?;
                Ok(data)
            }
            "createclearlist" => {
                let args = if truth(&first)
                    && matches!(
                        first,
                        V::Object(_) | V::Array(_) | V::BigNumber(_) | V::Promise
                    ) {
                    vec![
                        cl_or(&[
                            first.get("adminAddress"),
                            first.get("admin"),
                            first.get("admin_address"),
                        ]),
                        first.get("name"),
                        first.get("url"),
                        first.get("description"),
                        cl_or(&[first.get("backupAddress"), first.get("backup")]),
                        first.get("id"),
                    ]
                } else {
                    (0..6).map(|i| arg(a, i)).collect()
                };
                self.call("ClearList.createClearlist", args)
            }
            "getClearlistById" => {
                self.call("ClearList.loadClearlists", vec![])?;
                cl_key(&first)?;
                Ok(oracle_source_map_get(&self.source_clearlist_lists, &first)
                    .cloned()
                    .unwrap_or(V::Bool(false)))
            }
            "getList" => {
                self.database("attestations")?;
                let row =
                    self.source_clearlist_find("attestations", obj(&[("_id", first)]), true)?;
                Ok(if truth(&row) {
                    row.get("data")
                } else {
                    V::Null
                })
            }
            "verifyAdmin" => {
                cl_key(&first)?;
                let row = oracle_source_map_get(&self.source_clearlist_lists, &first)
                    .cloned()
                    .unwrap_or(V::Undefined);
                if !truth(&row) {
                    return Err(error("Clearlist not found"));
                }
                if matches!(
                    second,
                    V::Object(_) | V::Array(_) | V::BigNumber(_) | V::Promise
                ) {
                    return Err(SourceRuleError::UnsupportedNumericDomain.into());
                }
                Ok(V::Bool(same(
                    &cl_or(&[row.get("adminAddress"), row.get("admin")]),
                    &second,
                )))
            }
            "updateAdmin" => {
                let db_key = s(source_channel_string(&first)?);
                cl_key(&first)?;
                let index = self
                    .source_clearlist_lists
                    .iter()
                    .position(|(key, _)| oracle_source_same_key(key, &first))
                    .ok_or_else(|| error("Clearlist not found"))?;
                let row = &mut self.source_clearlist_lists[index].1;
                if !truth(row) {
                    return Err(error("Clearlist not found"));
                }
                if truth(&arg(a, 2)) {
                    cl_set(row, "backupAddress", second.clone())?;
                    cl_set(row, "backup", second)?;
                } else {
                    cl_set(row, "adminAddress", second.clone())?;
                    cl_set(row, "admin", second)?;
                }
                // The same cached object changes before either awaited DB call.
                let row = row.clone();
                self.database("clearlists")?;
                self.source_clearlist_update("clearlists", db_key, obj(&[("data", row)]), true)?;
                Ok(V::Undefined)
            }
            "addAttestation" | "addAttestationWithXpub" => {
                let explicit = method == "addAttestationWithXpub";
                let meta = arg(a, 2);
                let xpub = if explicit {
                    cl_or(&[arg(a, 3), V::Null])
                } else if let V::String(text) = &meta {
                    text.strip_prefix("xpub:").map(s).unwrap_or(V::Null)
                } else {
                    V::Null
                };
                let data = obj(&[
                    ("listId", first),
                    ("address", second.clone()),
                    ("status", s("active")),
                    ("data", meta),
                    ("xpub", xpub),
                    ("timestamp", arg(a, if explicit { 4 } else { 3 })),
                ]);
                self.database("attestations")?;
                self.source_clearlist_update(
                    "attestations",
                    second.clone(),
                    obj(&[("data", data)]),
                    true,
                )?;
                Ok(second)
            }
            "deriveAddressesFromXpub" => self.source_clearlist_derive(first),
            "isAddressInClearlist" => self.clearlist_contains(a),
            "isAddressInClearlistOrDerived" => {
                if truth(&self.call(
                    "ClearList.isAddressInClearlist",
                    vec![first.clone(), second.clone()],
                )?) {
                    return Ok(V::Bool(true));
                }
                self.database("attestations")?;
                let exists = obj(&[("$exists", V::Bool(true)), ("$ne", V::Null)]);
                let rows = self.source_clearlist_find(
                    "attestations",
                    obj(&[
                        ("data.listId", first.clone()),
                        ("data.status", s("active")),
                        ("data.xpub", exists.clone()),
                    ]),
                    false,
                )?;
                for row in array(&rows)? {
                    let xpub = cl_prop(&cl_prop(&row, "data")?, "xpub")?;
                    if truth(&xpub)
                        && array(&self.call("ClearList.deriveAddressesFromXpub", vec![xpub])?)?
                            .iter()
                            .any(|v| same(v, &second))
                    {
                        return Ok(V::Bool(true));
                    }
                }
                let rows = self.source_clearlist_find(
                    "attestations",
                    obj(&[
                        ("data.listId", first),
                        ("data.status", s("active")),
                        ("data.merkleRoot", exists),
                    ]),
                    false,
                )?;
                for row in array(&rows)? {
                    let root = cl_prop(&cl_prop(&row, "data")?, "merkleRoot")?;
                    cl_key(&root)?;
                    let xpubs = oracle_source_map_get(&self.source_clearlist_roots, &root)
                        .cloned()
                        .unwrap_or(V::Array(vec![]));
                    for xpub in array(&xpubs)? {
                        if array(&self.call("ClearList.deriveAddressesFromXpub", vec![xpub])?)?
                            .iter()
                            .any(|v| same(v, &second))
                        {
                            return Ok(V::Bool(true));
                        }
                    }
                }
                Ok(V::Bool(false))
            }
            "storeMerkleRoot" => {
                self.database("attestations")?;
                let key = s(format!(
                    "merkle:{}:{}",
                    source_channel_string(&first)?,
                    source_channel_string(&second)?
                ));
                let data = obj(&[
                    ("listId", first),
                    ("address", arg(a, 2)),
                    ("status", s("active")),
                    ("merkleRoot", second.clone()),
                    ("timestamp", arg(a, 3)),
                ]);
                self.source_clearlist_update("attestations", key, obj(&[("data", data)]), true)?;
                cl_key(&second)?;
                if oracle_source_map_get(&self.source_clearlist_roots, &second).is_none() {
                    oracle_source_map_set(
                        &mut self.source_clearlist_roots,
                        second,
                        V::Array(vec![]),
                    )?;
                }
                Ok(V::Undefined)
            }
            "registerXpubForRoot" => {
                cl_key(&first)?;
                let Some(index) = self
                    .source_clearlist_roots
                    .iter()
                    .position(|(key, _)| oracle_source_same_key(key, &first))
                else {
                    return Ok(V::Bool(false));
                };
                if !truth(&self.call(
                    "ClearList.verifyMerkleProof",
                    vec![second.clone(), arg(a, 2), first],
                )?) {
                    return Ok(V::Bool(false));
                }
                let V::Array(xpubs) = &mut self.source_clearlist_roots[index].1 else {
                    return Err(SourceRuleError::InvalidInput.into());
                };
                if !xpubs.iter().any(|v| oracle_source_same_key(v, &second)) {
                    if xpubs.len() >= MAX_TABLE_ROWS {
                        return Err(SourceRuleError::Capacity.into());
                    }
                    xpubs.push(second);
                }
                Ok(V::Bool(true))
            }
            "verifyMerkleProof" | "buildMerkleTree" | "generateMerkleProof" => {
                if method == "generateMerkleProof" {
                    self.call("ClearList.buildMerkleTree", vec![first])?;
                }
                outcome(crate::clearlist_source_owner::source_clearlist_pure_unverified(method, a)?)
            }
            "revokeAttestation" => {
                self.database("attestations")?;
                let query = obj(&[
                    ("data.address", second.clone()),
                    ("data.status", s("active")),
                    (
                        "$or",
                        V::Array(vec![
                            obj(&[("data.listId", first.clone())]),
                            obj(&[("data.clearlistId", first.clone())]),
                        ]),
                    ),
                ]);
                let mut rows = array(&self.source_clearlist_find("attestations", query, false)?)?;
                if rows.is_empty() {
                    let prior = self.source_clearlist_find(
                        "attestations",
                        obj(&[(
                            "_id",
                            s(format!("attestation:{}", source_channel_string(&second)?)),
                        )]),
                        true,
                    )?;
                    let row = if truth(&prior) {
                        prior
                    } else {
                        self.source_clearlist_find("attestations", obj(&[("_id", second)]), true)?
                    };
                    if !truth(&row) {
                        return Err(error("Attestation not found"));
                    }
                    rows.push(row);
                }
                for row in rows {
                    let mut data = cl_prop(&row, "data")?;
                    cl_set(&mut data, "status", s("revoked"))?;
                    cl_set(&mut data, "revokeReason", arg(a, 2))?;
                    cl_set(&mut data, "timestamp", arg(a, 3))?;
                    cl_set(&mut data, "id", first.clone())?;
                    self.source_clearlist_update(
                        "attestations",
                        cl_prop(&row, "_id")?,
                        obj(&[("data", data)]),
                        false,
                    )?;
                }
                Ok(first)
            }
            "getCountryCodeByAddress" => self.source_clearlist_country(first),
            "setBanlist" => {
                self.database("clearlists")?;
                self.source_clearlist_update(
                    "clearlists",
                    s("globalBanlist"),
                    obj(&[("data", first.clone()), ("timestamp", second)]),
                    true,
                )?;
                self.source_clearlist_banlist = first;
                Ok(V::Undefined)
            }
            "setBanList" => self.call("ClearList.setBanlist", vec![first, second]),
            "getBanlist" => {
                self.database("clearlists")?;
                Ok(self
                    .source_clearlist_find(
                        "clearlists",
                        obj(&[("_id", s("globalBanlist"))]),
                        false,
                    )?
                    .get("data"))
            }
            "getAttestations" => {
                self.database("attestations")?;
                self.source_clearlist_find(
                    "attestations",
                    obj(&[(
                        "$or",
                        V::Array(vec![
                            obj(&[("data.listId", first.clone())]),
                            obj(&[("data.clearlistId", first)]),
                        ]),
                    )]),
                    false,
                )
            }
            "getAttestationHistory" => {
                self.database("attestations")?;
                let rows = self.source_clearlist_find(
                    "attestations",
                    obj(&[("data.address", first), ("data.listId", second)]),
                    false,
                )?;
                outcome(
                    crate::clearlist_source_owner::source_clearlist_pure_unverified(
                        "sortHistory",
                        &[rows],
                    )?,
                )
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_clearlist_logic(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "createClearList" => {
                if !truth(&arg(a, 0)) {
                    return Ok(V::Undefined);
                }
                let row = self.call(
                    "ClearList.createClearlist",
                    (0..5).map(|i| arg(a, i)).collect(),
                )?;
                Ok(obj(&[(
                    "message",
                    s(format!(
                        "clearlist created successfully with ID: {}",
                        source_channel_string(&row.get("id"))?
                    )),
                )]))
            }
            "updateBannedCountries" => {
                let values = arg(a, 0);
                if !matches!(&values,V::Array(codes)if codes.iter().all(|v|matches!(v,V::String(s)if s.encode_utf16().count()==2)))
                {
                    return Ok(V::Undefined);
                }
                self.call("ClearList.setBanlist", vec![values, arg(a, 1)])?;
                Ok(V::Undefined)
            }
            "issueOrRevokeAttestation" => {
                let admin = self.call("Activation.getAdmin", vec![])?;
                let zero = lower_oracle_number(&arg(a, 1))? == 0.0;
                if matches!(arg(a,2),V::Object(fields)if fields.iter().any(|(key,_)|key=="toString"))
                {
                    return Err(type_error(
                        "(targetAddress || \"\").toString is not a function",
                    ));
                }
                let target = source_channel_string(&cl_or(&[arg(a, 2), s("")]))?
                    .trim_matches(funding_js_space)
                    .to_uppercase();
                if zero && target == "BANLIST" {
                    if !same(&arg(a, 0), &admin) {
                        return Err(error(
                            "Only protocol admin can update global banlist via list id 0",
                        ));
                    }
                    let meta = arg(a, 3);
                    let codes = if matches!(meta, V::Array(_)) {
                        meta
                    } else {
                        V::Array(
                            source_channel_string(&cl_or(&[meta, s("")]))?
                                .split(|c: char| {
                                    c == ',' || c == ';' || c == '|' || funding_js_space(c)
                                })
                                .map(|value| value.trim_matches(funding_js_space))
                                .filter(|v| !v.is_empty())
                                .map(|v| s(v.to_uppercase()))
                                .collect(),
                        )
                    };
                    self.call("Logic.updateBannedCountries", vec![codes, arg(a, 5)])?;
                    return Ok(V::Undefined);
                }
                if !truth(&arg(a, 4)) {
                    self.call(
                        "ClearList.addAttestation",
                        vec![arg(a, 1), arg(a, 2), arg(a, 3), arg(a, 5)],
                    )?;
                    if truth(&arg(a, 6)) {
                        self.call(
                            "ClearList.storeMerkleRoot",
                            vec![arg(a, 1), arg(a, 6), arg(a, 2), arg(a, 5)],
                        )?;
                    }
                } else if lower_oracle_number(&arg(a, 4))? == 1.0 {
                    self.call(
                        "ClearList.revokeAttestation",
                        vec![arg(a, 1), arg(a, 2), arg(a, 3), arg(a, 5)],
                    )?;
                }
                Ok(V::Undefined)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}
impl SourceContractRuntime {
    fn source_clearlist_raw_logic(
        &mut self,
        kind: &V,
        p: &V,
    ) -> Result<SourceOutcome, SourceRuleError> {
        let (method, arguments) = if *kind == n(7.) {
            (
                "createClearList",
                vec![
                    p.get("senderAddress"),
                    p.get("name"),
                    p.get("url"),
                    p.get("description"),
                    p.get("backupAddress"),
                    p.get("block"),
                ],
            )
        } else {
            (
                "issueOrRevokeAttestation",
                vec![
                    p.get("sender"),
                    p.get("id"),
                    p.get("targetAddress"),
                    p.get("metaData"),
                    p.get("revoke"),
                    p.get("block"),
                    p.get("merkleRoot"),
                ],
            )
        };
        match self.invoke_dependency(&DependencyCall {
            target: format!("Logic.{method}"),
            arguments,
        })? {
            DependencyReturn::Immediate(_) | DependencyReturn::Pending(_) => {
                Ok(SourceOutcome::Returned(V::Undefined))
            }
            DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
                Ok(SourceOutcome::Thrown(e))
            }
        }
    }
}
