//! Actual source metadata owners. Their declarations never create Ledger funds.
use crate::clearlist_source_owner::ClearlistSourceOwner;
use crate::codec::{verify_crc, with_crc, Cursor};
use crate::hash256::double_sha256_parts;
use crate::property_catalog::{CatalogError, PropertyCatalog};
use crate::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceOutcome, SourceRuleError,
};

pub const MAX_SOURCE_CATALOG_BYTES: usize = 32 * 1024 * 1024;
const MAGIC: &[u8; 5] = b"TLSC\x01";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCatalogOwners {
    pub properties: PropertyCatalog,
    pub clearlists: ClearlistSourceOwner,
}
impl SourceCatalogOwners {
    pub fn new(properties: PropertyCatalog, clearlists: ClearlistSourceOwner) -> Self {
        Self {
            properties,
            clearlists,
        }
    }
    pub fn checkpoint_bytes(&self) -> Result<Vec<u8>, SourceRuleError> {
        let property = self.properties.checkpoint_bytes().map_err(catalog_error)?;
        let clearlist = self.clearlists.checkpoint()?;
        let length = 17usize
            .checked_add(property.len())
            .and_then(|n| n.checked_add(clearlist.len()))
            .ok_or(SourceRuleError::Capacity)?;
        if length > MAX_SOURCE_CATALOG_BYTES {
            return Err(SourceRuleError::Capacity);
        }
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&(property.len() as u32).to_be_bytes());
        out.extend_from_slice(&property);
        out.extend_from_slice(&(clearlist.len() as u32).to_be_bytes());
        out.extend_from_slice(&clearlist);
        with_crc(out).map_err(|_| SourceRuleError::Capacity)
    }
    pub fn from_checkpoint_bytes(bytes: &[u8]) -> Result<Self, SourceRuleError> {
        if bytes.len() > MAX_SOURCE_CATALOG_BYTES {
            return Err(SourceRuleError::Capacity);
        }
        let raw = verify_crc(bytes).map_err(|_| SourceRuleError::InvalidInput)?;
        let mut r = Cursor::new(raw);
        if r.fixed::<5>().map_err(|_| SourceRuleError::InvalidInput)? != *MAGIC {
            return Err(SourceRuleError::InvalidInput);
        }
        let property_length = r.u32().map_err(|_| SourceRuleError::InvalidInput)? as usize;
        let properties = PropertyCatalog::from_checkpoint_bytes(
            r.take(property_length)
                .map_err(|_| SourceRuleError::InvalidInput)?,
        )
        .map_err(catalog_error)?;
        let clearlist_length = r.u32().map_err(|_| SourceRuleError::InvalidInput)? as usize;
        let clearlists = ClearlistSourceOwner::from_checkpoint(
            r.take(clearlist_length)
                .map_err(|_| SourceRuleError::InvalidInput)?,
        )?;
        r.end().map_err(|_| SourceRuleError::InvalidInput)?;
        let state = Self {
            properties,
            clearlists,
        };
        if state.checkpoint_bytes()? != bytes {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(state)
    }
    /// Source validators receive real pending catalog reads. This permits only
    /// getter/cache-load methods, never a caller-selected metadata mutation.
    pub(crate) fn invoke_read(
        &mut self,
        call: &DependencyCall,
    ) -> Result<DependencyReturn, SourceRuleError> {
        if let Some(method) = call.target.strip_prefix("Property.") {
            if !matches!(
                method,
                "getPropertyData"
                    | "isManagedAndAdmin"
                    | "doesTickerExist"
                    | "getPropertyIndex"
                    | "getAMM"
                    | "isSyntheticToken"
            ) {
                return Err(SourceRuleError::UnknownDependency);
            }
            let evaluation = self
                .properties
                .execute(method, &call.arguments, &mut NoTally)
                .map_err(catalog_error)?;
            return Ok(match evaluation.outcome {
                SourceOutcome::Returned(value) => DependencyReturn::Pending(value),
                SourceOutcome::Thrown(error) => DependencyReturn::PendingThrow(error),
            });
        }
        if let Some(method) = call.target.strip_prefix("ClearList.") {
            if !matches!(
                method,
                "getList"
                    | "getClearlistById"
                    | "getAttestations"
                    | "getAttestationHistory"
                    | "getCountryCodeByAddress"
                    | "getBanlist"
                    | "isAddressInClearlist"
                    | "isAddressInClearlistOrDerived"
                    | "verifyAdmin"
                    | "getNextId"
                    | "deriveAddressesFromXpub"
            ) {
                return Err(SourceRuleError::UnknownDependency);
            }
            return self.clearlists.invoke_source(method, &call.arguments);
        }
        Err(SourceRuleError::UnknownDependency)
    }
}
pub fn bootstrap_commitment(bytes: &[u8]) -> [u8; 32] {
    double_sha256_parts(&[b"TradeLayer/SourceCatalogBootstrap/v1/", bytes])
}
pub(crate) fn catalog_error(error: CatalogError) -> SourceRuleError {
    match error {
        CatalogError::Capacity => SourceRuleError::Capacity,
        CatalogError::UnsupportedDomain => SourceRuleError::UnsupportedNumericDomain,
        CatalogError::Backend(error) => error,
        _ => SourceRuleError::InvalidInput,
    }
}
struct NoTally;
impl SourceBackend for NoTally {
    fn invoke(&mut self, _: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        Err(SourceRuleError::UnknownDependency)
    }
}
