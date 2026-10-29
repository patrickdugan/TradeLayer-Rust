impl SourceMainNativeDurableActiveServiceV1 {
    fn install_inline_native_replacement(&mut self) -> ActiveWalResult<()> {
        let recovery = self
            .pending
            .as_mut()
            .and_then(|p| p.native_recovery.as_mut())
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        if recovery.installed {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let replacement = recovery
            .inline_replacement
            .take()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let mapping = recovery
            .inline_mapping
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let Some(ActiveWalOwner::Active(service)) = self.owner.as_mut() else {
            recovery.inline_replacement = Some(replacement);
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        };
        let Some(frame) = service.frame.as_mut() else {
            recovery.inline_replacement = Some(replacement);
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        };
        if let Err((error, replacement)) = service
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_service_install_inline_supersession(
                &service.owner.index,
                frame,
                replacement,
                mapping,
            )
        {
            recovery.inline_replacement = Some(replacement);
            return Err(active_wal_backend(error.to_string()));
        }
        recovery.installed = true;
        service.attempted_write = frame.pending_native_context();
        Ok(())
    }
}
