impl VerifiedMixedNativeWindowAccessV1<'_> {
    fn mixed_all_escrows(&self) -> Result<Vec<Mode4Escrow>> {
        let config = self.origin().get("generationEscrow");
        let origin = self.mode4_pin(self.origin_history())?;
        let mut out = vec![];
        for root in &self.reader.escrow_roots {
            for (reference, raw) in self.mode4_chain(root.value(), true)? {
                history_keys(
                    &raw,
                    &[
                        "profile",
                        "previous",
                        "nativeReadContext",
                        "expectedEscrowFieldsHex",
                    ],
                    &[],
                )?;
                if raw.get("profile") != s("TL-ACTIVE-GENERATION-ESCROW-CHAIN-13-1") {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
                let pin = SourceIndexGenerationEscrowPinV1::from_canonical_scalar_bytes(
                    self.closure_metadata_namespace(),
                    &mode4_decode_hex(&raw.get("expectedEscrowFieldsHex"))?,
                )
                .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
                if pin.points.len() != 1
                    || pin.points[0].storage != pin.storage.original_current
                    || pin.storage.original_current.namespace != origin.storage.namespace
                    || pin.storage.original_current.persistent_identity
                        != origin.storage.persistent_identity
                    || pin.storage.original_current.canonical_catalog
                        != origin.storage.canonical_catalog
                    || pin.points[0].metadata.context != origin.metadata.context
                    || s(pin.storage.copied_root.to_string_lossy()) != config.get("root")
                    || s(format!("{:?}", pin.storage.durability)) != config.get("durability")
                {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
                // An expected descriptor alone is insufficient: open the actual
                // published copy and verify its record/log/full-KV/row evidence.
                VerifiedOriginalMetadataViewV1::open_copy(&pin, &pin.points[0])
                    .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
                out.push(Mode4Escrow {
                    reference,
                    pin,
                    context: raw.get("nativeReadContext"),
                });
            }
        }
        Ok(out)
    }
}
