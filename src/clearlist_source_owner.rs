//! Owned pinned clearlist.js collections, singleton maps, and source operations.
//! Full-row storage deliberately retains the source's storage and query defects.
//! This metadata owner contains no Ledger and does not authenticate its callers.
use crate::hash256::sha256;
use crate::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceException, SourceOutcome,
    SourceRuleError, SourceValue as V,
};

#[path = "clearlist_source_owner_values.rs"]
mod values;
use values::*;

pub const SOURCE_SHA256: &str = "fbdb702e5e89bac113226223f42bf762af13fff40dee4e0c4ab6cd4a50d451e1";
pub const MAX_ROWS: usize = 1024;
pub const MAX_STATE_BYTES: usize = 16 * 1024 * 1024;
pub const METHODS: [&str; 25] = [
    "createClearlist",
    "createclearlist",
    "loadClearlists",
    "getClearlistById",
    "getList",
    "verifyAdmin",
    "updateAdmin",
    "getNextId",
    "addAttestation",
    "addAttestationWithXpub",
    "deriveAddressesFromXpub",
    "isAddressInClearlistOrDerived",
    "storeMerkleRoot",
    "registerXpubForRoot",
    "verifyMerkleProof",
    "buildMerkleTree",
    "generateMerkleProof",
    "revokeAttestation",
    "getCountryCodeByAddress",
    "setBanlist",
    "setBanList",
    "getBanlist",
    "getAttestations",
    "getAttestationHistory",
    "isAddressInClearlist",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClearlistDbCall {
    pub collection: String,
    pub operation: String,
    pub query: V,
    pub update: V,
    pub options: V,
}
impl ClearlistDbCall {
    pub fn value(&self) -> V {
        obj(&[
            ("collection", s(&self.collection)),
            ("operation", s(&self.operation)),
            ("query", self.query.clone()),
            ("update", self.update.clone()),
            ("options", self.options.clone()),
        ])
    }
}
fn call_size(call: &ClearlistDbCall) -> usize {
    fn size(value: &V) -> usize {
        let nested = match value {
            V::String(s) | V::BigNumber(s) => s.len(),
            V::Array(a) => a.iter().map(size).fold(0usize, usize::saturating_add),
            V::Object(o) => o
                .iter()
                .map(|(k, v)| k.len().saturating_add(64).saturating_add(size(v)))
                .fold(0usize, usize::saturating_add),
            _ => 0,
        };
        64usize.saturating_add(nested)
    }
    call.collection
        .len()
        .saturating_add(call.operation.len())
        .saturating_add(size(&call.query))
        .saturating_add(size(&call.update))
        .saturating_add(size(&call.options))
        .saturating_add(128)
}

/// One injected adapter failure, after `skip` matching calls have succeeded.
/// It models an external DB rejection; source catch boundaries are still applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClearlistDbFailure {
    pub collection: String,
    pub operation: String,
    pub skip: usize,
    pub exception: SourceException,
}

/// Insertion order is observable for queries, equal-time history and Map loads.
/// DB documents and loaded Map values are separate cloned values, as in NeDB.
#[derive(Debug, Clone)]
pub struct ClearlistSourceOwner {
    list_rows: Vec<V>,
    attestation_rows: Vec<V>,
    lists: Vec<(V, V)>,
    banlist: V,
    xpub_cache: Vec<(V, V)>,
    roots: Vec<(V, V)>,
    derivation_library: crate::bip32_public::SourceLibrary,
    failures: Vec<ClearlistDbFailure>,
    calls: Vec<ClearlistDbCall>,
    calls_bytes: usize,
}
impl PartialEq for ClearlistSourceOwner {
    fn eq(&self, other: &Self) -> bool {
        self.list_rows == other.list_rows
            && self.attestation_rows == other.attestation_rows
            && self.lists == other.lists
            && self.banlist == other.banlist
            && self.xpub_cache == other.xpub_cache
            && self.roots == other.roots
            && self.derivation_library == other.derivation_library
    }
}
impl Eq for ClearlistSourceOwner {}
impl Default for ClearlistSourceOwner {
    fn default() -> Self {
        Self {
            list_rows: vec![],
            attestation_rows: vec![],
            lists: vec![],
            banlist: V::Array(
                ["US", "KP", "SY", "RU", "IR", "CU"]
                    .into_iter()
                    .map(s)
                    .collect(),
            ),
            xpub_cache: vec![],
            roots: vec![],
            derivation_library: crate::bip32_public::SourceLibrary::Litecoin,
            failures: vec![],
            calls: vec![],
            calls_bytes: 0,
        }
    }
}
impl ClearlistSourceOwner {
    pub fn new() -> Self {
        Self::default()
    }
    /// The source falls back only if loading the Litecoin module fails.
    /// Selection is explicit; a malformed key never switches libraries.
    pub fn with_source_library(library: crate::bip32_public::SourceLibrary) -> Self {
        Self {
            derivation_library: library,
            ..Self::default()
        }
    }
    pub fn source_library(&self) -> crate::bip32_public::SourceLibrary {
        self.derivation_library
    }
    pub fn rows(&self, collection: &str) -> Result<&[V], SourceRuleError> {
        match collection {
            "clearlists" => Ok(&self.list_rows),
            "attestations" => Ok(&self.attestation_rows),
            _ => Err(SourceRuleError::UnknownDependency),
        }
    }
    /// Adapter import/recovery preserves every row field and existing order.
    pub fn import_rows(&mut self, collection: &str, rows: Vec<V>) -> Result<(), SourceRuleError> {
        validate_rows(&rows)?;
        let mut candidate = self.clone();
        match collection {
            "clearlists" => candidate.list_rows = rows,
            "attestations" => candidate.attestation_rows = rows,
            _ => return Err(SourceRuleError::UnknownDependency),
        }
        candidate.encode_checkpoint(true)?;
        *self = candidate;
        Ok(())
    }
    pub fn fail_database_call(
        &mut self,
        failure: ClearlistDbFailure,
    ) -> Result<(), SourceRuleError> {
        if !matches!(failure.collection.as_str(), "clearlists" | "attestations")
            || !matches!(
                failure.operation.as_str(),
                "getDatabase" | "findAsync" | "findOneAsync" | "updateAsync"
            )
        {
            return Err(SourceRuleError::InvalidInput);
        }
        if self.failures.len() >= MAX_ROWS {
            return Err(SourceRuleError::Capacity);
        }
        s(&failure.exception.name).validate()?;
        s(&failure.exception.message).validate()?;
        let bytes = self
            .failures
            .iter()
            .map(|f| {
                f.exception
                    .name
                    .len()
                    .saturating_add(f.exception.message.len())
            })
            .fold(0usize, usize::saturating_add);
        if bytes
            .saturating_add(failure.exception.name.len())
            .saturating_add(failure.exception.message.len())
            > MAX_STATE_BYTES
        {
            return Err(SourceRuleError::Capacity);
        }
        self.failures.push(failure);
        Ok(())
    }
    pub fn take_db_calls(&mut self) -> Vec<ClearlistDbCall> {
        self.calls_bytes = 0;
        core::mem::take(&mut self.calls)
    }
    pub fn banlist_cache(&self) -> &V {
        &self.banlist
    }
    pub fn state_value(&self) -> V {
        let pairs = |p: &[(V, V)]| {
            V::Array(
                p.iter()
                    .map(|(a, b)| V::Array(vec![a.clone(), b.clone()]))
                    .collect(),
            )
        };
        obj(&[
            ("clearlists", V::Array(self.list_rows.clone())),
            ("attestations", V::Array(self.attestation_rows.clone())),
            ("clearlistsMap", pairs(&self.lists)),
            ("banlist", self.banlist.clone()),
            ("xpubCache", pairs(&self.xpub_cache)),
            ("merkleXpubMap", pairs(&self.roots)),
        ])
    }
    /// Complete owner checkpoint for replay, rollback and economic snapshots.
    /// It retains volatile singleton maps. DB-only process restart uses `durable_checkpoint`.
    pub fn checkpoint(&self) -> Result<Vec<u8>, SourceRuleError> {
        self.encode_checkpoint(true)
    }
    pub fn durable_checkpoint(&self) -> Result<Vec<u8>, SourceRuleError> {
        self.encode_checkpoint(false)
    }
    pub fn from_checkpoint(bytes: &[u8]) -> Result<Self, SourceRuleError> {
        Self::decode_checkpoint(bytes, true)
    }
    pub fn restore_durable(bytes: &[u8]) -> Result<Self, SourceRuleError> {
        Self::decode_checkpoint(bytes, false)
    }
    pub fn restore_durable_with_source_library(
        bytes: &[u8],
        library: crate::bip32_public::SourceLibrary,
    ) -> Result<Self, SourceRuleError> {
        let mut owner = Self::restore_durable(bytes)?;
        owner.derivation_library = library;
        Ok(owner)
    }
    pub fn replay(&mut self, calls: &[SourceCall]) -> Result<Vec<SourceOutcome>, SourceRuleError> {
        if calls.len() > MAX_ROWS {
            return Err(SourceRuleError::Capacity);
        }
        calls.iter().map(|call| self.evaluate(call)).collect()
    }
    pub fn evaluate(&mut self, call: &SourceCall) -> Result<SourceOutcome, SourceRuleError> {
        if call.arguments.len() > 8 {
            return Err(SourceRuleError::Capacity);
        }
        for v in &call.arguments {
            v.validate()?;
        }
        if !METHODS.contains(&call.method.as_str()) {
            return Err(SourceRuleError::UnknownMethod);
        }
        // Local capability limits have no JavaScript equivalent. Preserve a
        // valid owner when rejecting that domain; source exceptions themselves
        // still retain every partial write performed before the throw.
        let before = self.clone();
        let result = self.run(&call.method, &call.arguments);
        if let Err(Fault::Local(e)) = &result {
            let error = e.clone();
            *self = before;
            return Err(error);
        }
        if let Err(e) = self.encode_checkpoint(true) {
            *self = before;
            return Err(e);
        }
        match result {
            Ok(v) => Ok(SourceOutcome::Returned(v)),
            Err(Fault::Thrown(e)) => Ok(SourceOutcome::Thrown(e)),
            Err(Fault::Local(e)) => Err(e),
        }
    }
    /// Actual dependency results, including the source getter's undefined value.
    pub fn invoke_source(
        &mut self,
        method: &str,
        arguments: &[V],
    ) -> Result<DependencyReturn, SourceRuleError> {
        let sync = matches!(
            method,
            "deriveAddressesFromXpub"
                | "registerXpubForRoot"
                | "verifyMerkleProof"
                | "buildMerkleTree"
                | "generateMerkleProof"
        );
        Ok(
            match self.evaluate(&SourceCall {
                method: method.into(),
                arguments: arguments.to_vec(),
            })? {
                SourceOutcome::Returned(v) if sync => DependencyReturn::Immediate(v),
                SourceOutcome::Returned(v) => DependencyReturn::Pending(v),
                SourceOutcome::Thrown(e) if sync => DependencyReturn::Throw(e),
                SourceOutcome::Thrown(e) => DependencyReturn::PendingThrow(e),
            },
        )
    }
    fn db_call(
        &mut self,
        collection: &str,
        operation: &str,
        query: V,
        update: V,
        options: V,
    ) -> Eval<()> {
        // The trace is drainable and diagnostic, so long-running owners do not
        // depend on retaining an unbounded execution history.
        let call = ClearlistDbCall {
            collection: collection.into(),
            operation: operation.into(),
            query,
            update,
            options,
        };
        let size = call_size(&call);
        while !self.calls.is_empty()
            && (self.calls.len() >= MAX_ROWS * 16
                || self.calls_bytes.saturating_add(size) > MAX_STATE_BYTES)
        {
            self.calls_bytes = self
                .calls_bytes
                .saturating_sub(call_size(&self.calls.remove(0)));
        }
        self.calls_bytes = self.calls_bytes.saturating_add(size);
        self.calls.push(call);
        if let Some(i) = self
            .failures
            .iter()
            .position(|f| f.collection == collection && f.operation == operation)
        {
            if self.failures[i].skip == 0 {
                return Err(Fault::Thrown(self.failures.remove(i).exception));
            }
            self.failures[i].skip -= 1;
        }
        Ok(())
    }
    fn database(&mut self, collection: &str) -> Eval<()> {
        self.db_call(
            collection,
            "getDatabase",
            V::Undefined,
            V::Undefined,
            V::Undefined,
        )
    }
    fn find(&mut self, collection: &str, query: V, one: bool) -> Eval<V> {
        self.db_call(
            collection,
            if one { "findOneAsync" } else { "findAsync" },
            query.clone(),
            V::Undefined,
            V::Undefined,
        )?;
        let rows = self.rows(collection)?;
        let found: Vec<V> = rows
            .iter()
            .filter(|r| matches_query(r, &query))
            .cloned()
            .collect();
        Ok(if one {
            found.into_iter().next().unwrap_or(V::Null)
        } else {
            V::Array(found)
        })
    }
    fn update(&mut self, collection: &str, key: V, set: V, upsert: bool) -> Eval<()> {
        primitive_key(&key)?;
        let query = obj(&[("_id", key.clone())]);
        let options = if upsert {
            obj(&[("upsert", V::Bool(true))])
        } else {
            obj(&[])
        };
        self.db_call(
            collection,
            "updateAsync",
            query.clone(),
            obj(&[("$set", set.clone())]),
            options,
        )?;
        let rows = if collection == "clearlists" {
            &mut self.list_rows
        } else {
            &mut self.attestation_rows
        };
        let index = rows.iter().position(|r| strict(&r.get("_id"), &key));
        let i = if let Some(i) = index {
            i
        } else if upsert {
            if rows.len() == MAX_ROWS {
                return Err(SourceRuleError::Capacity.into());
            }
            rows.len()
        } else {
            return Ok(());
        };
        let mut candidate = rows.get(i).cloned().unwrap_or(query);
        if let V::Object(fields) = set {
            for (k, v) in fields {
                set_prop(&mut candidate, &k, v)?;
            }
        }
        candidate.validate()?;
        if i == rows.len() {
            rows.push(candidate);
        } else {
            rows[i] = candidate;
        }
        Ok(())
    }
    fn load(&mut self) -> Eval<V> {
        // Source catches adapter and destructuring failures, retaining any Map
        // entries loaded before the failure and every stale cache entry.
        let result = (|| -> Eval<V> {
            self.database("clearlists")?;
            let rows = self.find("clearlists", obj(&[]), false)?;
            for row in array(&rows)? {
                let key = prop(row, "_id")?;
                primitive_key(&key)?;
                map_set(&mut self.lists, key, prop(row, "data")?)?;
            }
            Ok(rows)
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Undefined),
            other => other,
        }
    }
    fn next_id(&mut self) -> Eval<V> {
        self.load()?;
        let mut max = 0.0_f64;
        for (key, _) in &self.lists {
            let n = parse_int(key)?;
            if n > max {
                max = n;
            }
        }
        Ok(V::number(max + 1.0))
    }
    fn create(&mut self, args: &[V]) -> Eval<V> {
        let mut id = arg(args, 5);
        if !truth(&id) {
            id = self.next_id()?;
        }
        let data = obj(&[
            ("id", id.clone()),
            ("admin", arg(args, 0)),
            ("name", arg(args, 1)),
            ("description", arg(args, 3)),
            ("backup", arg(args, 4)),
        ]);
        self.database("clearlists")?;
        self.update("clearlists", id, obj(&[("data", data.clone())]), true)?;
        Ok(data)
    }
    fn attestation(&mut self, args: &[V], explicit: bool) -> Eval<V> {
        let address = arg(args, 1);
        let meta = arg(args, 2);
        let xpub = if explicit {
            let x = arg(args, 3);
            if truth(&x) {
                x
            } else {
                V::Null
            }
        } else if let V::String(t) = &meta {
            t.strip_prefix("xpub:").map(s).unwrap_or(V::Null)
        } else {
            V::Null
        };
        let data = obj(&[
            ("listId", arg(args, 0)),
            ("address", address.clone()),
            ("status", s("active")),
            ("data", meta),
            ("xpub", xpub),
            ("timestamp", arg(args, if explicit { 4 } else { 3 })),
        ]);
        self.database("attestations")?;
        self.update(
            "attestations",
            address.clone(),
            obj(&[("data", data)]),
            true,
        )?;
        Ok(address)
    }
    fn direct_query(list: V, address: V) -> V {
        obj(&[
            ("data.address", address),
            ("data.status", s("active")),
            (
                "$or",
                V::Array(vec![
                    obj(&[("data.listId", list.clone())]),
                    obj(&[("data.clearlistId", list)]),
                ]),
            ),
        ])
    }
    fn direct(&mut self, list: V, address: V) -> Eval<V> {
        self.database("attestations")?;
        let rows = self.find("attestations", Self::direct_query(list, address), false)?;
        Ok(V::Bool(!array(&rows)?.is_empty()))
    }
    fn derive(&mut self, xpub: V) -> Eval<V> {
        primitive_key(&xpub)?;
        if let Some(v) = map_get(&self.xpub_cache, &xpub) {
            return Ok(v.clone());
        }
        // A rejected public key produces [] without a cache entry, matching
        // the source catch. Accepted derivations use the native BIP32 owner.
        let V::String(text) = &xpub else {
            return Ok(V::Array(vec![]));
        };
        match crate::bip32_public::derive_addresses_with_source_library(
            text,
            self.derivation_library,
        ) {
            Ok(addresses) => {
                let result = V::Array(addresses.into_iter().map(V::String).collect());
                map_set(&mut self.xpub_cache, xpub, result.clone())?;
                Ok(result)
            }
            Err(crate::bip32_public::Bip32Error::UnsupportedPrivateKey) => {
                Err(SourceRuleError::UnsupportedNumericDomain.into())
            }
            Err(
                crate::bip32_public::Bip32Error::RetryLimit
                | crate::bip32_public::Bip32Error::ChildIndexExhausted,
            ) => Err(SourceRuleError::Capacity.into()),
            Err(crate::bip32_public::Bip32Error::Arithmetic) => {
                Err(SourceRuleError::UnsupportedNumericDomain.into())
            }
            Err(_) => Ok(V::Array(vec![])),
        }
    }
    fn derived(&mut self, list: V, address: V) -> Eval<V> {
        if truth(&self.direct(list.clone(), address.clone())?) {
            return Ok(V::Bool(true));
        }
        self.database("attestations")?;
        let exists = obj(&[("$exists", V::Bool(true)), ("$ne", V::Null)]);
        let candidates = self.find(
            "attestations",
            obj(&[
                ("data.listId", list.clone()),
                ("data.status", s("active")),
                ("data.xpub", exists.clone()),
            ]),
            false,
        )?;
        for row in array(&candidates)? {
            let x = prop(&prop(row, "data")?, "xpub")?;
            if truth(&x) && includes(&self.derive(x)?, &address)? {
                return Ok(V::Bool(true));
            }
        }
        let roots = self.find(
            "attestations",
            obj(&[
                ("data.listId", list),
                ("data.status", s("active")),
                ("data.merkleRoot", exists),
            ]),
            false,
        )?;
        for row in array(&roots)? {
            let root = prop(&prop(row, "data")?, "merkleRoot")?;
            let xpubs = map_get(&self.roots, &root)
                .cloned()
                .unwrap_or(V::Array(vec![]));
            for x in array(&xpubs)? {
                if includes(&self.derive(x.clone())?, &address)? {
                    return Ok(V::Bool(true));
                }
            }
        }
        Ok(V::Bool(false))
    }
    fn revoke(&mut self, args: &[V]) -> Eval<V> {
        let id = arg(args, 0);
        let address = arg(args, 1);
        self.database("attestations")?;
        let records = self.find(
            "attestations",
            Self::direct_query(id.clone(), address.clone()),
            false,
        )?;
        let mut selected = array(&records)?.to_vec();
        if selected.is_empty() {
            let first = self.find(
                "attestations",
                obj(&[("_id", s(&format!("attestation:{}", text(&address)?)))]),
                true,
            )?;
            let legacy = if truth(&first) {
                first
            } else {
                self.find("attestations", obj(&[("_id", address)]), true)?
            };
            if !truth(&legacy) {
                return Err(error("Attestation not found"));
            }
            selected.push(legacy);
        }
        for rec in selected {
            let mut data = prop(&rec, "data")?;
            set_prop(&mut data, "status", s("revoked"))?;
            set_prop(&mut data, "revokeReason", arg(args, 2))?;
            set_prop(&mut data, "timestamp", arg(args, 3))?;
            set_prop(&mut data, "id", id.clone())?;
            self.update(
                "attestations",
                prop(&rec, "_id")?,
                obj(&[("data", data)]),
                false,
            )?;
        }
        Ok(id)
    }
    fn country(&mut self, address: V) -> Eval<V> {
        let result = (|| -> Eval<V> {
            self.database("attestations")?;
            let rows = self.find(
                "attestations",
                obj(&[("data.address", address.clone())]),
                false,
            )?;
            for att in array(&rows)? {
                let data = att.get("data");
                let row = if truth(&data) { data } else { obj(&[]) };
                let status = or(&[row.get("status"), s("active")]);
                let list = row.get("listId");
                let list = if list != V::Undefined {
                    list
                } else {
                    row.get("clearlistId")
                };
                let list_number = number(&list)?;
                let meta = row.get("data");
                if !strict(&status, &s("active")) || list_number != 0.0 {
                    continue;
                }
                let candidate = if matches!(&meta, V::String(_)) {
                    meta.clone()
                } else if truth(&meta) && matches!(&meta, V::Object(_) | V::Array(_)) {
                    or(&[
                        meta.get("countryCode"),
                        meta.get("country"),
                        meta.get("code"),
                        meta.get("value"),
                    ])
                } else {
                    V::Undefined
                };
                if let V::String(code) = candidate {
                    let code = code.trim_matches(js_space);
                    if !code.is_empty() {
                        return Ok(obj(&[
                            ("address", address),
                            ("countryCode", s(&code.to_uppercase())),
                            (
                                "blockHeight",
                                or(&[row.get("blockHeight"), row.get("timestamp"), V::Null]),
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
    fn run(&mut self, method: &str, args: &[V]) -> Eval<V> {
        let a = arg(args, 0);
        let b = arg(args, 1);
        match method {
            "createClearlist" => self.create(args),
            "createclearlist" => {
                if truth(&a) && matches!(&a, V::Object(_) | V::Array(_)) {
                    self.create(&[
                        or(&[
                            a.get("adminAddress"),
                            a.get("admin"),
                            a.get("admin_address"),
                        ]),
                        a.get("name"),
                        a.get("url"),
                        a.get("description"),
                        or(&[a.get("backupAddress"), a.get("backup")]),
                        a.get("id"),
                    ])
                } else {
                    self.create(args)
                }
            }
            "loadClearlists" => self.load(),
            "getClearlistById" => {
                self.load()?;
                primitive_key(&a)?;
                Ok(map_get(&self.lists, &a).cloned().unwrap_or(V::Bool(false)))
            }
            "getList" => {
                self.database("attestations")?;
                let row = self.find("attestations", obj(&[("_id", a)]), true)?;
                Ok(if truth(&row) {
                    row.get("data")
                } else {
                    V::Null
                })
            }
            "verifyAdmin" => {
                primitive_key(&a)?;
                let row = map_get(&self.lists, &a).cloned().unwrap_or(V::Undefined);
                if !truth(&row) {
                    return Err(error("Clearlist not found"));
                }
                Ok(V::Bool(strict(
                    &or(&[row.get("adminAddress"), row.get("admin")]),
                    &b,
                )))
            }
            "updateAdmin" => {
                let key = s(&text(&a)?);
                primitive_key(&a)?;
                let index =
                    map_index(&self.lists, &a).ok_or_else(|| error("Clearlist not found"))?;
                if !truth(&self.lists[index].1) {
                    return Err(error("Clearlist not found"));
                }
                if truth(&arg(args, 2)) {
                    set_prop(&mut self.lists[index].1, "backupAddress", b.clone())?;
                    set_prop(&mut self.lists[index].1, "backup", b)?;
                } else {
                    set_prop(&mut self.lists[index].1, "adminAddress", b.clone())?;
                    set_prop(&mut self.lists[index].1, "admin", b)?;
                }
                let row = self.lists[index].1.clone();
                self.database("clearlists")?;
                self.update("clearlists", key, obj(&[("data", row)]), true)?;
                Ok(V::Undefined)
            }
            "getNextId" => self.next_id(),
            "addAttestation" => self.attestation(args, false),
            "addAttestationWithXpub" => self.attestation(args, true),
            "deriveAddressesFromXpub" => self.derive(a),
            "isAddressInClearlistOrDerived" => self.derived(a, b),
            "storeMerkleRoot" => {
                self.database("attestations")?;
                let key = s(&format!("merkle:{}:{}", text(&a)?, text(&b)?));
                let data = obj(&[
                    ("listId", a),
                    ("address", arg(args, 2)),
                    ("status", s("active")),
                    ("merkleRoot", b.clone()),
                    ("timestamp", arg(args, 3)),
                ]);
                self.update("attestations", key, obj(&[("data", data)]), true)?;
                primitive_key(&b)?;
                if map_get(&self.roots, &b).is_none() {
                    map_set(&mut self.roots, b, V::Array(vec![]))?;
                }
                Ok(V::Undefined)
            }
            "registerXpubForRoot" => {
                primitive_key(&a)?;
                let Some(index) = map_index(&self.roots, &a) else {
                    return Ok(V::Bool(false));
                };
                if !verify_proof(&b, &arg(args, 2), &a)? {
                    return Ok(V::Bool(false));
                }
                let V::Array(xpubs) = &mut self.roots[index].1 else {
                    return Err(SourceRuleError::InvalidInput.into());
                };
                if !xpubs.iter().any(|v| same_key(v, &b)) {
                    if xpubs.len() == MAX_ROWS {
                        return Err(SourceRuleError::Capacity.into());
                    }
                    xpubs.push(b);
                }
                Ok(V::Bool(true))
            }
            "verifyMerkleProof" => Ok(V::Bool(verify_proof(&a, &b, &arg(args, 2))?)),
            "buildMerkleTree" => build_tree(&a),
            "generateMerkleProof" => generate_proof(&a, &b),
            "revokeAttestation" => self.revoke(args),
            "getCountryCodeByAddress" => self.country(a),
            "setBanlist" | "setBanList" => {
                self.database("clearlists")?;
                self.update(
                    "clearlists",
                    s("globalBanlist"),
                    obj(&[("data", a.clone()), ("timestamp", b)]),
                    true,
                )?;
                self.banlist = a;
                Ok(V::Undefined)
            }
            "getBanlist" => {
                self.database("clearlists")?;
                self.find("clearlists", obj(&[("_id", s("globalBanlist"))]), false)?;
                Ok(V::Undefined)
            }
            "getAttestations" => {
                self.database("attestations")?;
                self.find(
                    "attestations",
                    obj(&[(
                        "$or",
                        V::Array(vec![
                            obj(&[("data.listId", a.clone())]),
                            obj(&[("data.clearlistId", a)]),
                        ]),
                    )]),
                    false,
                )
            }
            "getAttestationHistory" => {
                self.database("attestations")?;
                let rows = self.find(
                    "attestations",
                    obj(&[("data.address", a), ("data.listId", b)]),
                    false,
                )?;
                let mut rows = array(&rows)?.to_vec();
                sort_history(&mut rows)?;
                Ok(V::Array(rows))
            }
            "isAddressInClearlist" => self.direct(a, b),
            _ => Err(SourceRuleError::UnknownMethod.into()),
        }
    }
}
impl SourceBackend for ClearlistSourceOwner {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let method = call
            .target
            .strip_prefix("ClearList.")
            .or_else(|| call.target.strip_prefix("clearlistManager."))
            .ok_or(SourceRuleError::UnknownDependency)?;
        self.invoke_source(method, &call.arguments)
    }
}

fn digest(text: &str) -> String {
    sha256(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn crypto_hash(value: &V) -> Eval<String> {
    if let V::String(text) = value {
        Ok(digest(text))
    } else {
        Err(type_error(format!("The \"data\" argument must be of type string or an instance of Buffer, TypedArray, or DataView. Received {}",received(value))))
    }
}
fn verify_proof(xpub: &V, proof: &V, expected: &V) -> Eval<bool> {
    let mut hash = crypto_hash(xpub)?;
    let siblings = iterable(proof, "proof")?;
    for sibling in siblings {
        let position = prop(&sibling, "position")?;
        let other = prop(&sibling, "hash")?;
        let combined = if strict(&position, &s("right")) {
            format!("{hash}{}", text(&other)?)
        } else {
            format!("{}{hash}", text(&other)?)
        };
        hash = digest(&combined);
    }
    Ok(strict(&s(&hash), expected))
}
fn build_tree(xpubs: &V) -> Eval<V> {
    if !truth(xpubs) || strict(&xpubs.get("length"), &V::number(0.0)) {
        return Ok(obj(&[("root", V::Null), ("tree", V::Array(vec![]))]));
    }
    let V::Array(xpubs) = xpubs else {
        return Err(type_error("xpubs.map is not a function"));
    };
    let mut leaves = xpubs.iter().map(crypto_hash).collect::<Eval<Vec<_>>>()?;
    if leaves.len() % 2 != 0 {
        leaves.push(leaves.last().cloned().unwrap_or_default());
    }
    let mut tree = vec![leaves.clone()];
    while leaves.len() > 1 {
        let mut level = vec![];
        for pair in leaves.chunks(2) {
            level.push(digest(&format!(
                "{}{}",
                pair[0],
                pair.get(1).unwrap_or(&pair[0])
            )));
        }
        tree.push(level.clone());
        leaves = level;
    }
    Ok(obj(&[
        ("root", leaves.first().map(|v| s(v)).unwrap_or(V::Undefined)),
        (
            "tree",
            V::Array(
                tree.into_iter()
                    .map(|l| V::Array(l.into_iter().map(V::String).collect()))
                    .collect(),
            ),
        ),
    ]))
}
fn generate_proof(xpubs: &V, target: &V) -> Eval<V> {
    let tree = build_tree(xpubs)?.get("tree");
    let tree = array(&tree)?;
    if tree.is_empty() {
        return Ok(V::Null);
    }
    let leaf = s(&crypto_hash(target)?);
    let Some(mut idx) = array(&tree[0])?.iter().position(|v| strict(v, &leaf)) else {
        return Ok(V::Null);
    };
    let mut proof = vec![];
    for level in &tree[..tree.len() - 1] {
        let layer = array(level)?;
        let right = idx % 2 == 1;
        let sibling = if right { idx - 1 } else { idx + 1 };
        if sibling < layer.len() {
            proof.push(obj(&[
                ("hash", layer[sibling].clone()),
                ("position", s(if right { "left" } else { "right" })),
            ]));
        }
        idx /= 2;
    }
    Ok(V::Array(proof))
}

/// Stateless reuse of verified source pure helpers. No rows or owner are created.
pub(crate) fn source_clearlist_pure_unverified(
    method: &str,
    args: &[V],
) -> Result<SourceOutcome, SourceRuleError> {
    let result = (|| -> Eval<V> {
        match method {
            "verifyMerkleProof" => Ok(V::Bool(verify_proof(
                &arg(args, 0),
                &arg(args, 1),
                &arg(args, 2),
            )?)),
            "buildMerkleTree" => build_tree(&arg(args, 0)),
            "generateMerkleProof" => generate_proof(&arg(args, 0), &arg(args, 1)),
            "sortHistory" => {
                let mut rows = array(&arg(args, 0))?.to_vec();
                sort_history(&mut rows)?;
                Ok(V::Array(rows))
            }
            _ => Err(SourceRuleError::UnknownMethod.into()),
        }
    })();
    match result {
        Ok(v) => Ok(SourceOutcome::Returned(v)),
        Err(Fault::Thrown(e)) => Ok(SourceOutcome::Thrown(e)),
        Err(Fault::Local(e)) => Err(e),
    }
}
