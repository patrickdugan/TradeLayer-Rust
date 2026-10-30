// Exact pre-optimization actual method, test-only reference.
impl ActiveServiceWalV1 {
    fn prepare_reference_uncached(
        &self,
        expected: &ActiveServiceWalPinV1,
        record: V,
        blobs: Vec<Vec<u8>>,
        references: Vec<ActiveServiceWalBlobV1>,
    ) -> Result<PreparedActiveServiceWalWriteV1> {
        if self.pin()? != *expected {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let record = bytes(&record)?;
        if blobs.len() > 16 || references.len() > 64 {
            return Err(ActiveServiceWalErrorV1::Capacity);
        }
        for raw in &blobs {
            ActiveServiceWalBlobV1::of(raw)?;
        }
        // Every retained dependency is pinned now. New blobs are verified at
        // write; already-existing referenced blobs must really be present.
        for pin in &references {
            if !blobs
                .iter()
                .any(|raw| ActiveServiceWalBlobV1::of(raw).ok().as_ref() == Some(pin))
            {
                self.read_blob(pin)?;
            }
        }
        let mut entries = self
            .healthy()?
            .entries()?
            .map(|(k, v)| (k.to_owned(), v.to_vec()))
            .collect::<BTreeMap<_, _>>();
        entries.insert(RECORD.into(), record.clone());
        let mut post = expected.clone();
        post.prefix.physical_sequence = post
            .prefix
            .physical_sequence
            .checked_add(1)
            .ok_or(ActiveServiceWalErrorV1::Capacity)?;
        post.prefix.content = complete(&entries)?;
        Ok(PreparedActiveServiceWalWriteV1 {
            owner: self.owner,
            before: expected.clone(),
            post,
            record,
            blobs,
            references,
            attempted: false,
        })
    }
}
