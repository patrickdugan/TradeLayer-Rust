// V12 recorded file IO. Facts are raw IO failure/mtime observations only.
impl ContractCore {
    fn source_main_fs_attempt(&mut self, method: &str, args: &[V]) -> R<()> {
        let mut state = self.source_main_service_get("fileIO")?;
        if state == V::Undefined {
            return Ok(());
        }
        let mut counts = state.get("counts");
        let value = counts.get(method);
        let ordinal = if value == V::Undefined {
            1.
        } else {
            number(&value)? + 1.
        };
        set(&mut counts, method, n(ordinal))?;
        set(&mut state, "counts", counts)?;
        self.source_main_service_set("fileIO", state.clone())?;
        let failures = match state.get("facts").get("failures") {
            V::Undefined => vec![],
            v => array(&v)?,
        };
        let Some(f) = failures
            .into_iter()
            .find(|f| f.get("method") == s(method) && same(&f.get("ordinal"), &n(ordinal)))
        else {
            return Ok(());
        };
        let message = if f.get("message") == V::Undefined {
            Ok(String::new())
        } else {
            source_channel_string(&f.get("message"))
        };
        let (e, code) = match message {
            Ok(message) => (
                crate::validity_source::SourceException {
                    name: "Error".into(),
                    message,
                },
                f.get("code"),
            ),
            Err(Fault::Thrown(e)) => (e, V::Undefined),
            Err(e) => return Err(e),
        };
        self.emit(obj(&[
            ("kind", s("filesystem")),
            ("method", s(method)),
            ("args", V::Array(args.to_vec())),
            (
                "thrown",
                obj(&[
                    ("name", s(&e.name)),
                    ("message", s(&e.message)),
                    ("code", code),
                ]),
            ),
        ]));
        Err(Fault::Thrown(e))
    }
    fn source_main_fs_failure_code(&self, start: usize) -> V {
        self.effects[start..]
            .iter()
            .rev()
            .find(|v| v.get("kind") == s("filesystem") && v.get("thrown") != V::Undefined)
            .map_or(V::Undefined, |v| v.get("thrown").get("code"))
    }
    fn source_main_fs_copy(&mut self, src: &str, dst: &str) -> R<()> {
        for _ in 0..5 {
            let start = self.effects.len();
            match self.source_main_fs_copy_once(src, dst) {
                Ok(()) => return Ok(()),
                Err(Fault::Thrown(e)) => {
                    let code = self.source_main_fs_failure_code(start);
                    if code == s("EBUSY") || code == s("EPERM") {
                        self.source_main_service_timer(n(50.));
                        continue;
                    }
                    return Err(Fault::Thrown(e));
                }
                Err(e) => return Err(e),
            }
        }
        Err(error(format!("safeCopy failed: {src} → {dst}")))
    }
    fn source_main_fs_remove(&mut self, path: &str, tree: bool) -> R<()> {
        if tree {
            return self.source_main_fs_remove_once(path, true);
        }
        for _ in 0..5 {
            let start = self.effects.len();
            match self.source_main_fs_remove_once(path, false) {
                Ok(()) => return Ok(()),
                Err(Fault::Thrown(e)) => {
                    let code = self.source_main_fs_failure_code(start);
                    if code == s("EBUSY") || code == s("EPERM") {
                        self.source_main_service_timer(n(50.));
                        continue;
                    }
                    if code == s("ENOENT") {
                        return Ok(());
                    }
                    return Err(Fault::Thrown(e));
                }
                Err(e) => return Err(e),
            }
        }
        Err(error(format!("safeDelete failed: {path}")))
    }
    fn source_main_fs_stat(&mut self, path: &str) -> R<V> {
        self.source_main_fs_attempt("stat", &[s(path)])?;
        let directories = array(&self.source_main_service_get("directories")?)?;
        let directory = directories.contains(&s(path));
        if !directory
            && !self
                .source_main_fs_files()?
                .iter()
                .any(|r| arg(&array(r).unwrap_or_default(), 0) == s(path))
        {
            return Err(self.source_main_fs_error("stat", vec![s(path)], "ENOENT", path));
        }
        let inputs = self
            .source_main_service_get("fileIO")?
            .get("facts")
            .get("mtimes");
        let inputs = if inputs == V::Undefined {
            vec![]
        } else {
            array(&inputs)?
        };
        let millis = inputs
            .into_iter()
            .find(|x| x.get("path") == s(path))
            .map_or(n(self.time_millis as f64), |v| v.get("millis"));
        Ok(self.source_main_fs_emit(
            "stat",
            vec![s(path)],
            obj(&[("mtimeMs", millis), ("directory", V::Bool(directory))]),
        ))
    }
}
impl ContractCore {
    fn source_main_file_io_configure(&mut self, input: V) -> R<()> {
        if input == V::Undefined {
            return Ok(());
        }
        let V::Object(ref fields) = input else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        if fields
            .iter()
            .any(|(k, _)| !["failures", "mtimes"].contains(&k.as_str()))
        {
            return Err(SourceRuleError::InvalidInput.into());
        }
        for row in match input.get("failures") {
            V::Undefined => vec![],
            V::Array(rows) => rows,
            _ => return Err(SourceRuleError::InvalidInput.into()),
        } {
            let V::Object(ref fields) = row else {
                return Err(SourceRuleError::InvalidInput.into());
            };
            let V::Number(bits) = row.get("ordinal") else {
                return Err(SourceRuleError::InvalidInput.into());
            };
            let ordinal = f64::from_bits(bits);
            if fields
                .iter()
                .any(|(k, _)| !["method", "ordinal", "code", "message"].contains(&k.as_str()))
                || !matches!(row.get("method"),V::String(ref v)if["mkdir","readdir","readFile","writeFile","copyFile","unlink","rm","stat"].contains(&v.as_str()))
                || !ordinal.is_finite()
                || ordinal < 1.
                || ordinal.fract() != 0.
            {
                return Err(SourceRuleError::InvalidInput.into());
            }
        }
        for row in match input.get("mtimes") {
            V::Undefined => vec![],
            V::Array(rows) => rows,
            _ => return Err(SourceRuleError::InvalidInput.into()),
        } {
            let V::Object(ref fields) = row else {
                return Err(SourceRuleError::InvalidInput.into());
            };
            if fields
                .iter()
                .any(|(k, _)| !["path", "millis"].contains(&k.as_str()))
                || !matches!(row.get("path"), V::String(_))
                || !matches!(row.get("millis"), V::Number(_))
            {
                return Err(SourceRuleError::InvalidInput.into());
            }
        }
        self.source_main_service_set("fileIO", obj(&[("facts", input), ("counts", obj(&[]))]))
    }
    fn source_main_file_last_header(&mut self) -> R<V> {
        self.database("persistence")?;
        let mut rows = array(&self.db(
            "persistence",
            "findAsync",
            vec![obj(&[("height", obj(&[("$exists", V::Bool(true))]))])],
        )?)?;
        source_v8_stable_sort(&mut rows, |a, b| -> R<core::cmp::Ordering> {
            Ok(
                (amm_quote_number(&a.get("height"))? - amm_quote_number(&b.get("height"))?)
                    .partial_cmp(&0.0)
                    .unwrap_or(core::cmp::Ordering::Equal),
            )
        })?;
        Ok(rows.last().cloned().unwrap_or(V::Null))
    }
    fn source_main_file_direct(&mut self, target: &str, a: &[V]) -> R<V> {
        match target {
            "Persistence.init" => {
                self.source_main_service_persistence_init()?;
                Ok(obj(&[("$persistenceInstance", n(1.))]))
            }
            "Persistence.safeCopy" => {
                self.source_main_fs_copy(
                    &source_main_snapshot_path_argument(&arg(a, 0))?,
                    &source_main_snapshot_path_argument(&arg(a, 1))?,
                )?;
                Ok(V::Undefined)
            }
            "Persistence.safeDelete" => {
                self.source_main_fs_remove(
                    &source_main_snapshot_path_argument(&arg(a, 0))?,
                    false,
                )?;
                Ok(V::Undefined)
            }
            "Persistence.updateMaxProcessed" => {
                self.source_main_height_write("MaxProcessedHeight", arg(a, 0))?;
                Ok(arg(a, 0))
            }
            "Persistence.saveTrackHeight" => {
                self.source_main_height_write("TrackHeight", arg(a, 0))
            }
            "Persistence.recordBlockHeader" => {
                self.database("persistence")?;
                self.update(
                    "persistence",
                    obj(&[("_id", s(format!("block-{}", text(&arg(a, 0))?)))]),
                    obj(&[(
                        "$set",
                        obj(&[
                            ("height", arg(a, 0)),
                            ("hash", arg(a, 1)),
                            ("prevHash", arg(a, 2)),
                            ("createdAt", n(self.time_millis as f64)),
                        ]),
                    )]),
                    true,
                )?;
                Ok(V::Undefined)
            }
            "Persistence.getBlockHeader" => {
                self.database("persistence")?;
                self.find(
                    "persistence",
                    obj(&[("_id", s(format!("block-{}", text(&arg(a, 0))?)))]),
                )
            }
            "Persistence.getLastKnownBlock" => self.source_main_file_last_header(),
            "Persistence.checkForReorgForNewBlock" => {
                self.source_main_service_reorg_check(arg(a, 0), arg(a, 1))
            }
            "Persistence.detectAndHandleReorg" => self.source_main_service_reorg_no_snapshots(),
            "Persistence.cleanupPersistence" => {
                self.database("persistence")?;
                self.db(
                    "persistence",
                    "removeAsync",
                    vec![
                        obj(&[("height", obj(&[("$lt", n(number(&arg(a, 0))? - 1000.))]))]),
                        obj(&[("multi", V::Bool(true))]),
                    ],
                )?;
                Ok(V::Undefined)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}

include!("source_v8_sort_impl.rs");
