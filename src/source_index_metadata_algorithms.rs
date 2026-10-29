//! Shared original TxIndex metadata bodies, generic only over the IO carrier.
//! There is no second index, cached result, supplied validity or balance owner.
use crate::validity_source::SourceValue as V;
pub(crate) enum SourceIndexMetadataMethod {
    FindMax,
    Initialize,
}
pub(crate) trait SourceIndexMetadataIo {
    type Error;
    fn database(&mut self) -> Result<V, Self::Error>;
    fn find_one(&mut self, handle: &V, id: &str) -> Result<V, Self::Error>;
    fn insert_genesis(&mut self, handle: &V, height: V) -> Result<V, Self::Error>;
    fn log_values(&mut self, prefix: &str, value: V) -> Result<(), Self::Error>;
    fn log_throw(&mut self, prefix: &str, thrown: &Self::Error) -> Result<(), Self::Error>;
}
fn truth(value: &V) -> bool {
    match value {
        V::Undefined | V::Null | V::Bool(false) => false,
        V::Number(bits) => {
            let n = f64::from_bits(*bits);
            n != 0.0 && !n.is_nan()
        }
        V::String(value) => !value.is_empty(),
        _ => true,
    }
}
pub(crate) fn source_find_max<I: SourceIndexMetadataIo>(io: &mut I) -> Result<V, I::Error> {
    let result = (|| {
        let handle = io.database()?;
        let doc = io.find_one(&handle, "MaxHeight")?;
        Ok(if truth(&doc) {
            doc.get("value")
        } else {
            V::Null
        })
    })();
    if let Err(error) = &result {
        io.log_throw("Error finding MaxIndexedBlock:", error)?;
    }
    result
}
pub(crate) fn source_initialize_genesis<I: SourceIndexMetadataIo>(
    io: &mut I,
    height: V,
) -> Result<V, I::Error> {
    // The original factory await is outside both source try/catch blocks.
    let handle = io.database()?;
    let already_initialized = (|| {
        let existing = io.find_one(&handle, "genesisBlock")?;
        if truth(&existing) {
            io.log_values(
                "Genesis block is already initialized:",
                existing.get("value"),
            )?;
            return Ok(true);
        }
        Ok(false)
    })();
    match already_initialized {
        Ok(true) => return Ok(V::Undefined),
        Ok(false) => (),
        Err(error) => {
            io.log_throw("Error checking for existing genesis block:", &error)?;
            return Err(error);
        }
    }
    let inserted = (|| {
        io.insert_genesis(&handle, height.clone())?;
        io.log_values("Genesis block initialized:", height)?;
        Ok(V::Undefined)
    })();
    if let Err(error) = &inserted {
        io.log_throw("Error initializing genesis block:", error)?;
    }
    inserted
}
