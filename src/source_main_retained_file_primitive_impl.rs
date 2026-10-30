// Mechanically derived original primitive match; only whole BlockTail
// refuses BEFORE IO until its actual retained native handoff is admitted.
impl SourceContractRuntime {
    fn native_main_retained_file_primitive(
        &mut self,
        operation: &crate::source_main_step_kernel::FileStepPrimitive,
    ) -> R<V> {
        use crate::source_main_step_kernel::FileStepPrimitive as Op;
        let runtime = self;
        match operation {
            Op::Persistence => runtime
                .core
                .source_main_service_persistence()
                .map(|_| V::Undefined),
            Op::CopyOnce(src, dst) => {
                runtime
                    .core
                    .source_main_fs_copy_once(&text(src)?, &text(dst)?)?;
                Ok(V::Undefined)
            }
            Op::DeleteOnce(path) => {
                runtime
                    .core
                    .source_main_fs_remove_once(&text(path)?, false)?;
                Ok(V::Undefined)
            }
            Op::Exhausted { delete, src, dst } => Err(error(if *delete {
                format!("safeDelete failed: {}", source_channel_string(src)?)
            } else {
                format!(
                    "safeCopy failed: {} → {}",
                    source_channel_string(src)?,
                    source_channel_string(dst)?
                )
            })),
            Op::Path(base, name) => Ok(s(source_main_snapshot_join(
                &text(base)?,
                &source_main_snapshot_path_argument(name)?,
            ))),
            Op::Checkpoint(height) => runtime
                .core
                .source_main_file_checkpoint_prepare(height.clone()),
            Op::CreateBegin(height, hash) => runtime
                .core
                .source_main_file_create_begin(height.clone(), hash.clone()),
            Op::CreateFinish {
                height,
                hash,
                context,
                copied,
            } => runtime.core.source_main_file_create_finish(
                height.clone(),
                hash.clone(),
                context.clone(),
                copied.clone(),
            ),
            Op::RestoreBegin(meta) => runtime.core.source_main_file_restore_begin(meta.clone()),
            Op::RestoreFiles(meta) => runtime.core.source_main_file_restore_files(meta.clone()),
            Op::RestoreFinish(meta) => runtime.core.source_main_file_restore_finish(meta.clone()),
            Op::BlockPrelude(block, height) => {
                runtime.core.source_main_service_persistence()?;
                runtime.core.source_main_service_reorg_check(
                    height.clone(),
                    funding_prop(block, "previousblockhash")?,
                )
            }
            Op::ReorgPlan => runtime.core.source_main_file_reorg_plan(),
            Op::RecordHeader(block, height) => {
                runtime.core.database("persistence")?;
                runtime.core.update(
                    "persistence",
                    obj(&[("_id", s(format!("block-{}", text(height)?)))]),
                    obj(&[(
                        "$set",
                        obj(&[
                            ("height", height.clone()),
                            ("hash", funding_prop(block, "hash")?),
                            ("prevHash", funding_prop(block, "previousblockhash")?),
                            ("createdAt", n(runtime.core.time_millis as f64)),
                        ]),
                    )]),
                    true,
                )?;
                Ok(V::Undefined)
            }
            Op::BlockTail(..) => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}
