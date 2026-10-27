// Literal source tail. No lexical/global helper or proof verdict is installed.
impl ContractCore {
    fn source_tail_batch(&mut self, a: &[V]) -> R<V> {
        let parsed = source_channel_string(&arg(a, 1)).and_then(|text| parse(&text));
        match parsed {
            Err(Fault::Thrown(_)) => return Err(error("Invalid rollup data format")),
            Err(local) => return Err(local),
            Ok(_) => (),
        }
        // logic.js exposes Logic.isValidRollupDataStructure, but invokes the
        // absent unqualified identifier here. The real verifier is never reached.
        Err(Fault::Thrown(crate::validity_source::SourceException {
            name: "ReferenceError".into(),
            message: "isValidRollupDataStructure is not defined".into(),
        }))
    }
}
