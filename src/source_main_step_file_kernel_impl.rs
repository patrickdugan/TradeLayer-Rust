// File continuation metadata only. Every IO/DB/financial primitive borrows the
// one existing Core; no file map, Runtime or Journal lives in these frames.
pub const FILE_RETRY_PROFILE: &str = "TL-MAIN-EXPLICIT-TIMER-FILE-RETRY-1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FileStepPrimitive {
    Persistence,
    CopyOnce(V, V),
    DeleteOnce(V),
    Exhausted {
        delete: bool,
        src: V,
        dst: V,
    },
    Path(V, V),
    Checkpoint(V),
    CreateBegin(V, V),
    CreateFinish {
        height: V,
        hash: V,
        context: V,
        copied: Vec<V>,
    },
    RestoreBegin(V),
    RestoreFiles(V),
    RestoreFinish(V),
    BlockPrelude(V, V),
    ReorgPlan,
    RecordHeader(V, V),
    BlockTail(V, V),
}
impl FileStepPrimitive {
    fn descriptor(&self) -> (&'static str, Vec<V>) {
        match self {
            Self::Persistence => ("File.PersistenceInstance", vec![]),
            Self::CopyOnce(src, dst) => ("File.CopyOnce", vec![src.clone(), dst.clone()]),
            Self::DeleteOnce(src) => ("File.DeleteOnce", vec![src.clone()]),
            Self::Exhausted { delete, src, dst } => (
                "File.RetryExhausted",
                vec![V::Bool(*delete), src.clone(), dst.clone()],
            ),
            Self::Path(base, name) => ("File.SourcePathJoin", vec![base.clone(), name.clone()]),
            Self::Checkpoint(height) => ("File.CheckpointPrepare", vec![height.clone()]),
            Self::CreateBegin(height, hash) => {
                ("File.CreateBegin", vec![height.clone(), hash.clone()])
            }
            Self::CreateFinish {
                height,
                hash,
                context,
                copied,
            } => (
                "File.CreateFinish",
                vec![
                    height.clone(),
                    hash.clone(),
                    context.clone(),
                    V::Array(copied.clone()),
                ],
            ),
            Self::RestoreBegin(meta) => ("File.RestoreBegin", vec![meta.clone()]),
            Self::RestoreFiles(meta) => ("File.RestoreFiles", vec![meta.clone()]),
            Self::RestoreFinish(meta) => ("File.RestoreFinish", vec![meta.clone()]),
            Self::BlockPrelude(block, height) => {
                ("File.BlockPrelude", vec![block.clone(), height.clone()])
            }
            Self::ReorgPlan => ("File.ReorgPlan", vec![]),
            Self::RecordHeader(block, height) => {
                ("File.RecordHeader", vec![block.clone(), height.clone()])
            }
            Self::BlockTail(block, height) => {
                ("File.BlockTail", vec![block.clone(), height.clone()])
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FileStepFrame {
    Retry {
        delete: bool,
        src: V,
        dst: V,
        attempt: u8,
        phase: u8,
        acquire: bool,
    },
    Checkpoint {
        height: V,
        phase: u8,
    },
    Create {
        height: V,
        hash: V,
        context: V,
        copied: Vec<V>,
        at: usize,
        phase: u8,
    },
    Restore {
        meta: V,
        context: V,
        files: V,
        at: usize,
        phase: u8,
    },
    Block {
        block: V,
        height: V,
        plan: V,
        phase: u8,
    },
}
fn file_call(
    owner: &mut impl MainStepOwner,
    id: u64,
    label: &str,
    op: FileStepPrimitive,
    e: &mut MainStepEvaluationV1,
) -> Advance<V> {
    MainStepKernelV1::operation(owner, id, label, MainStepPrimitive::File(Box::new(op)), e)
}
fn file_list(value: V) -> Advance<Vec<V>> {
    match value {
        V::Array(values) => Ok(values),
        _ => Err(AdvanceError::Stop(SourceRuleError::Backend)),
    }
}
impl FileStepFrame {
    fn retry(delete: bool, src: V, dst: V) -> Self {
        Self::Retry {
            delete,
            src,
            dst,
            attempt: 0,
            phase: 1,
            acquire: false,
        }
    }
    fn create(height: V, hash: V) -> Self {
        Self::Create {
            height,
            hash,
            context: V::Undefined,
            copied: vec![],
            at: 0,
            phase: 0,
        }
    }
    fn restore(meta: V) -> Self {
        Self::Restore {
            meta,
            context: V::Undefined,
            files: V::Undefined,
            at: 0,
            phase: 0,
        }
    }
    fn block(block: V, height: V) -> Self {
        Self::Block {
            block,
            height,
            plan: V::Undefined,
            phase: 0,
        }
    }
    fn from_call(method: &str, args: &[V]) -> Result<Option<Self>, MainStepError> {
        let value = match method {
            "Main.processBlock" => Self::block(arg(args, 0), arg(args, 1)),
            "Persistence.safeCopy" | "Persistence.safeDelete" => {
                if !matches!(arg(args, 0), V::String(_))
                    || (method.ends_with("safeCopy") && !matches!(arg(args, 1), V::String(_)))
                {
                    return Err(MainStepError::InvalidInput);
                }
                Self::Retry {
                    delete: method.ends_with("safeDelete"),
                    src: arg(args, 0),
                    dst: arg(args, 1),
                    attempt: 0,
                    phase: 0,
                    acquire: true,
                }
            }
            "Persistence.createSnapshot" => Self::create(arg(args, 0), arg(args, 1)),
            "Persistence.restoreSnapshot" => Self::restore(arg(args, 0)),
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
    fn phase_label(&self) -> String {
        let (name, phase) = match self {
            Self::Retry { delete, phase, .. } => (
                if *delete {
                    "Persistence.safeDelete"
                } else {
                    "Persistence.safeCopy"
                },
                phase,
            ),
            Self::Checkpoint { phase, .. } => ("Persistence.maybeCheckpoint", phase),
            Self::Create { phase, .. } => ("Persistence.createSnapshot", phase),
            Self::Restore { phase, .. } => ("Persistence.restoreSnapshot", phase),
            Self::Block { phase, .. } => ("Main.processBlock", phase),
        };
        format!("{name}/{phase}")
    }
    fn advance(
        &mut self,
        k: &mut MainStepKernelV1,
        id: u64,
        owner: &mut impl MainStepOwner,
        e: &mut MainStepEvaluationV1,
    ) -> Advance<Option<V>> {
        let label = self.phase_label();
        let mut call = |op| file_call(owner, id, &label, op, e);
        match self {
            Self::Retry {
                delete,
                src,
                dst,
                attempt,
                phase,
                acquire,
            } => match *phase {
                0 => {
                    if *acquire {
                        call(FileStepPrimitive::Persistence)?;
                    }
                    *phase = 1;
                    Ok(None)
                }
                1 => {
                    let op = if *delete {
                        FileStepPrimitive::DeleteOnce(src.clone())
                    } else {
                        FileStepPrimitive::CopyOnce(src.clone(), dst.clone())
                    };
                    match call(op) {
                        Ok(value) => Ok(Some(value)),
                        Err(AdvanceError::Throw(error)) => {
                            if strict(&error.code, &s("EBUSY")) || strict(&error.code, &s("EPERM"))
                            {
                                *phase = 2;
                                k.wait_timer(id, n(50.), e)?;
                                Ok(None)
                            } else if *delete && strict(&error.code, &s("ENOENT")) {
                                Ok(Some(V::Undefined))
                            } else {
                                Err(AdvanceError::Throw(error))
                            }
                        }
                        Err(error) => Err(error),
                    }
                }
                2 => {
                    *attempt += 1;
                    *phase = if *attempt == 5 { 3 } else { 1 };
                    Ok(None)
                }
                3 => call(FileStepPrimitive::Exhausted {
                    delete: *delete,
                    src: src.clone(),
                    dst: dst.clone(),
                })
                .map(Some),
                _ => Err(AdvanceError::Stop(SourceRuleError::Backend)),
            },
            Self::Checkpoint { height, phase } => match *phase {
                0 => {
                    let plan = call(FileStepPrimitive::Checkpoint(height.clone()))?;
                    if !truth(&plan.get("run")) {
                        return Ok(Some(V::Undefined));
                    }
                    *phase = 1;
                    k.child(
                        id,
                        Frame::File(Box::new(Self::create(height.clone(), plan.get("hash")))),
                    )?;
                    Ok(None)
                }
                1 => {
                    k.child_value(id)?;
                    Ok(Some(V::Undefined))
                }
                _ => Err(AdvanceError::Stop(SourceRuleError::Backend)),
            },
            Self::Create {
                height,
                hash,
                context,
                copied,
                at,
                phase,
            } => match *phase {
                0 => {
                    call(FileStepPrimitive::Persistence)?;
                    *context = call(FileStepPrimitive::CreateBegin(height.clone(), hash.clone()))?;
                    if !truth(&context.get("run")) {
                        return Ok(Some(V::Undefined));
                    }
                    *phase = 1;
                    Ok(None)
                }
                1 => {
                    let files = file_list(context.get("files"))?;
                    if *at == files.len() {
                        *phase = 3;
                        return Ok(None);
                    }
                    let file = files[*at].clone();
                    let V::String(ref name) = file else {
                        return Err(AdvanceError::Stop(SourceRuleError::Backend));
                    };
                    if !name.ends_with(".db") {
                        *at += 1;
                        return Ok(None);
                    }
                    let src = call(FileStepPrimitive::Path(context.get("dbDir"), file.clone()))?;
                    let dst = call(FileStepPrimitive::Path(context.get("dir"), file))?;
                    *phase = 2;
                    k.child(id, Frame::File(Box::new(Self::retry(false, src, dst))))?;
                    Ok(None)
                }
                2 => {
                    k.child_value(id)?;
                    let files = file_list(context.get("files"))?;
                    copied.push(files[*at].clone());
                    *at += 1;
                    *phase = 1;
                    Ok(None)
                }
                3 => {
                    call(FileStepPrimitive::CreateFinish {
                        height: height.clone(),
                        hash: hash.clone(),
                        context: context.clone(),
                        copied: copied.clone(),
                    })?;
                    Ok(Some(V::Undefined))
                }
                _ => Err(AdvanceError::Stop(SourceRuleError::Backend)),
            },
            Self::Restore {
                meta,
                context,
                files,
                at,
                phase,
            } => match *phase {
                0 => {
                    call(FileStepPrimitive::Persistence)?;
                    *context = call(FileStepPrimitive::RestoreBegin(meta.clone()))?;
                    *phase = 1;
                    Ok(None)
                }
                1 => {
                    let current = file_list(context.get("current"))?;
                    if *at == current.len() {
                        *at = 0;
                        *phase = 3;
                        return Ok(None);
                    }
                    let src = call(FileStepPrimitive::Path(
                        context.get("dbDir"),
                        current[*at].clone(),
                    ))?;
                    let dst = call(FileStepPrimitive::Path(
                        context.get("backup"),
                        current[*at].clone(),
                    ))?;
                    *phase = 2;
                    k.child(id, Frame::File(Box::new(Self::retry(false, src, dst))))?;
                    Ok(None)
                }
                2 => {
                    k.child_value(id)?;
                    *at += 1;
                    *phase = 1;
                    Ok(None)
                }
                3 => {
                    let current = file_list(context.get("current"))?;
                    if *at == current.len() {
                        *at = 0;
                        *phase = 5;
                        return Ok(None);
                    }
                    let V::String(ref name) = current[*at] else {
                        return Err(AdvanceError::Stop(SourceRuleError::Backend));
                    };
                    if !name.ends_with(".db") {
                        *at += 1;
                        return Ok(None);
                    }
                    let path = call(FileStepPrimitive::Path(
                        context.get("dbDir"),
                        current[*at].clone(),
                    ))?;
                    *phase = 4;
                    k.child(
                        id,
                        Frame::File(Box::new(Self::retry(true, path, V::Undefined))),
                    )?;
                    Ok(None)
                }
                4 => {
                    k.child_value(id)?;
                    *at += 1;
                    *phase = 3;
                    Ok(None)
                }
                5 => {
                    *files = call(FileStepPrimitive::RestoreFiles(meta.clone()))?;
                    *at = 0;
                    *phase = 6;
                    Ok(None)
                }
                6 => {
                    let selected = file_list(files.clone())?;
                    if *at == selected.len() {
                        *phase = 8;
                        return Ok(None);
                    }
                    let src = call(FileStepPrimitive::Path(
                        context.get("dir"),
                        selected[*at].clone(),
                    ))?;
                    let dst = call(FileStepPrimitive::Path(
                        context.get("dbDir"),
                        selected[*at].clone(),
                    ))?;
                    *phase = 7;
                    k.child(id, Frame::File(Box::new(Self::retry(false, src, dst))))?;
                    Ok(None)
                }
                7 => {
                    k.child_value(id)?;
                    *at += 1;
                    *phase = 6;
                    Ok(None)
                }
                8 => {
                    call(FileStepPrimitive::RestoreFinish(meta.clone()))?;
                    Ok(Some(V::Undefined))
                }
                _ => Err(AdvanceError::Stop(SourceRuleError::Backend)),
            },
            Self::Block {
                block,
                height,
                plan,
                phase,
            } => match *phase {
                0 => {
                    let reorg = call(FileStepPrimitive::BlockPrelude(
                        block.clone(),
                        height.clone(),
                    ))?;
                    *phase = if truth(&reorg) { 1 } else { 3 };
                    Ok(None)
                }
                1 => {
                    *plan = call(FileStepPrimitive::ReorgPlan)?;
                    if truth(&plan.get("restore")) {
                        *phase = 2;
                        k.child(id, Frame::File(Box::new(Self::restore(plan.get("meta")))))?;
                    } else {
                        *phase = 9;
                    }
                    Ok(None)
                }
                2 => {
                    k.child_value(id)?;
                    *phase = 9;
                    Ok(None)
                }
                9 => {
                    let restored = plan.get("info").get("restoredFrom");
                    if matches!(restored, V::Number(_)) {
                        Ok(Some(obj(&[
                            ("reOrg", V::Bool(true)),
                            ("restore", restored),
                        ])))
                    } else {
                        *phase = 3;
                        Ok(None)
                    }
                }
                3 => {
                    call(FileStepPrimitive::RecordHeader(
                        block.clone(),
                        height.clone(),
                    ))?;
                    *phase = 4;
                    k.child(
                        id,
                        Frame::File(Box::new(Self::Checkpoint {
                            height: height.clone(),
                            phase: 0,
                        })),
                    )?;
                    Ok(None)
                }
                4 => {
                    k.child_value(id)?;
                    call(FileStepPrimitive::BlockTail(block.clone(), height.clone())).map(Some)
                }
                _ => Err(AdvanceError::Stop(SourceRuleError::Backend)),
            },
        }
    }
    fn value(&self) -> V {
        let mut fields = vec![
            ("kind", s("FileContinuation")),
            ("pc", s(self.phase_label())),
        ];
        match self {
            Self::Retry {
                delete,
                src,
                dst,
                attempt,
                phase,
                acquire,
            } => fields.extend([
                ("delete", V::Bool(*delete)),
                ("src", src.clone()),
                ("dst", dst.clone()),
                ("attempt", s(attempt.to_string())),
                ("phase", s(phase.to_string())),
                ("acquire", V::Bool(*acquire)),
            ]),
            Self::Checkpoint { height, phase } => {
                fields.extend([("height", height.clone()), ("phase", s(phase.to_string()))])
            }
            Self::Create {
                height,
                hash,
                context,
                copied,
                at,
                phase,
            } => fields.extend([
                ("height", height.clone()),
                ("hash", hash.clone()),
                ("context", context.clone()),
                ("copied", V::Array(copied.clone())),
                ("at", s(at.to_string())),
                ("phase", s(phase.to_string())),
            ]),
            Self::Restore {
                meta,
                context,
                files,
                at,
                phase,
            } => fields.extend([
                ("meta", meta.clone()),
                ("context", context.clone()),
                ("files", files.clone()),
                ("at", s(at.to_string())),
                ("phase", s(phase.to_string())),
            ]),
            Self::Block {
                block,
                height,
                plan,
                phase,
            } => fields.extend([
                ("block", block.clone()),
                ("height", height.clone()),
                ("plan", plan.clone()),
                ("phase", s(phase.to_string())),
            ]),
        };
        obj(&fields)
    }
    fn visit_fields(
        &self,
        path: &str,
        emit: &mut impl FnMut(&str, &V) -> FieldResult,
    ) -> FieldResult {
        emit(&format!("{path}/pc"), &s(self.phase_label()))?;
        match self {
            Self::Retry {
                delete,
                src,
                dst,
                attempt,
                phase,
                acquire,
            } => {
                emit(&format!("{path}/delete"), &V::Bool(*delete))?;
                field_value(&format!("{path}/src"), src, emit)?;
                field_value(&format!("{path}/dst"), dst, emit)?;
                field_uint(&format!("{path}/attempt"), (*attempt).into(), emit)?;
                field_uint(&format!("{path}/phase"), (*phase).into(), emit)?;
                emit(&format!("{path}/acquire"), &V::Bool(*acquire))?;
            }
            Self::Checkpoint { height, phase } => {
                field_value(&format!("{path}/height"), height, emit)?;
                field_uint(&format!("{path}/phase"), (*phase).into(), emit)?;
            }
            Self::Create {
                height,
                hash,
                context,
                copied,
                at,
                phase,
            } => {
                for (key, value) in [("height", height), ("hash", hash), ("context", context)] {
                    field_value(&format!("{path}/{key}"), value, emit)?;
                }
                field_uint(&format!("{path}/copied/length"), copied.len() as u64, emit)?;
                for (i, file) in copied.iter().enumerate() {
                    field_value(&format!("{path}/copied/{i}"), file, emit)?;
                }
                field_uint(&format!("{path}/at"), *at as u64, emit)?;
                field_uint(&format!("{path}/phase"), (*phase).into(), emit)?;
            }
            Self::Restore {
                meta,
                context,
                files,
                at,
                phase,
            } => {
                for (key, value) in [("meta", meta), ("context", context), ("files", files)] {
                    field_value(&format!("{path}/{key}"), value, emit)?;
                }
                field_uint(&format!("{path}/at"), *at as u64, emit)?;
                field_uint(&format!("{path}/phase"), (*phase).into(), emit)?;
            }
            Self::Block {
                block,
                height,
                plan,
                phase,
            } => {
                for (key, value) in [("block", block), ("height", height), ("plan", plan)] {
                    field_value(&format!("{path}/{key}"), value, emit)?;
                }
                field_uint(&format!("{path}/phase"), (*phase).into(), emit)?;
            }
        };
        Ok(())
    }
}
