impl NativeMainColdServiceStateV1 {
    fn mixed_cold_validity_selection(&self) -> Option<V> {
        let pending = self
            .frame
            .as_ref()?
            .mixed_historical_validity_descriptor()?;
        Some(
            MixedSourceNativeSelectionV1::Validity(obj(&[
                (
                    "profile",
                    s("TL-MAIN-ACTUAL-ORDINARY-VALIDITY-SELECTION-13-1"),
                ),
                ("sourceProfile", s(self.profile())),
                (
                    "originCommands",
                    s(&self.origin.session.commands.to_string()),
                ),
                ("originHead", s(&hex(&self.origin.session.commitment))),
                ("originPhysical", physical_value(&self.origin.physical)),
                ("serviceActions", s(&self.head.actions.to_string())),
                ("serviceHead", s(&hex(&self.head.commitment))),
                ("pending", pending),
            ]))
            .value(),
        )
    }
}
