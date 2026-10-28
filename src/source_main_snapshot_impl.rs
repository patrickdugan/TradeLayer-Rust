// V12-only original Persistence file IO. These bytes are recorded artifacts,
// never a second financial row owner or an implicit database cache reload.
impl ContractCore {
    fn source_main_snapshot_main_instance(&mut self) -> R<V> {
        if self.source_main_service_get("startupModules")? != V::Undefined {
            return self.source_main_startup_main_instance();
        }
        // configureProduction actually owns the already-constructed Main.
        // This is the literal warm getInstance branch, not the cold constructor.
        if self.main_production.is_none() {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        self.emit(obj(&[
            ("kind", s("source-function")),
            ("target", s("Main.getInstance")),
            ("args", V::Array(vec![])),
        ]));
        Ok(obj(&[("$mainInstance", n(1.))]))
    }
    fn source_main_snapshot_main_genesis(&self, receiver: V) -> R<V> {
        if receiver != obj(&[("$mainInstance", n(1.))]) {
            return Err(SourceRuleError::InvalidInput.into());
        }
        Ok(self
            .main_production
            .as_ref()
            .ok_or(SourceRuleError::InvalidInput)?
            .genesis
            .clone())
    }

    fn source_main_fs_emit(&mut self, method: &str, args: Vec<V>, value: V) -> V {
        self.emit(obj(&[
            ("kind", s("filesystem")),
            ("method", s(method)),
            ("args", V::Array(args)),
            ("returned", value.clone()),
        ]));
        value
    }
    fn source_main_fs_error(
        &mut self,
        method: &str,
        args: Vec<V>,
        code: &str,
        path: &str,
    ) -> Fault {
        let message = format!("{code}: {path}");
        self.emit(obj(&[
            ("kind", s("filesystem")),
            ("method", s(method)),
            ("args", V::Array(args)),
            (
                "thrown",
                obj(&[
                    ("name", s("Error")),
                    ("message", s(&message)),
                    ("code", s(code)),
                ]),
            ),
        ]));
        error(message)
    }
    fn source_main_fs_files(&self) -> R<Vec<V>> {
        array(&self.source_main_service_get("files")?)
    }
    fn source_main_fs_names(&mut self, path: &str, dirents: bool) -> R<Vec<V>> {
        let directories = array(&self.source_main_service_get("directories")?)?;
        let mut args = vec![s(path)];
        if dirents {
            args.push(obj(&[("withFileTypes", V::Bool(true))]));
        }
        self.source_main_fs_attempt("readdir", &args)?;
        if !directories.contains(&s(path)) {
            return Err(self.source_main_fs_error("readdir", args, "ENOENT", path));
        }
        let mut keys = directories;
        keys.extend(
            self.source_main_fs_files()?
                .iter()
                .map(|row| arg(&array(row).unwrap_or_default(), 0)),
        );
        let prefix = format!("{path}/");
        let mut names = vec![];
        for key in keys {
            let key = text(&key)?;
            if let Some(rest) = key.strip_prefix(&prefix) {
                if !rest.is_empty() && !rest.contains('/') && !names.contains(&s(rest)) {
                    names.push(s(rest));
                }
            }
        }
        self.source_main_fs_emit("readdir", args, V::Array(names.clone()));
        Ok(names)
    }
    fn source_main_fs_read(&mut self, path: &str) -> R<V> {
        let args = vec![s(path), s("utf8")];
        self.source_main_fs_attempt("readFile", &args)?;
        let files = self.source_main_fs_files()?;
        let Some(row) = files
            .iter()
            .find(|row| arg(&array(row).unwrap_or_default(), 0) == s(path))
        else {
            return Err(self.source_main_fs_error("readFile", args, "ENOENT", path));
        };
        Ok(self.source_main_fs_emit("readFile", args, arg(&array(row)?, 1)))
    }
    fn source_main_fs_write(&mut self, path: &str, bytes: V) -> R<()> {
        self.source_main_fs_attempt("writeFile", &[s(path), bytes.clone()])?;
        let mut files = self.source_main_fs_files()?;
        if let Some(row) = files
            .iter_mut()
            .find(|row| arg(&array(row).unwrap_or_default(), 0) == s(path))
        {
            *row = V::Array(vec![s(path), bytes.clone()]);
        } else {
            files.push(V::Array(vec![s(path), bytes.clone()]));
        }
        self.source_main_service_set("files", V::Array(files))?;
        self.source_main_fs_emit("writeFile", vec![s(path), bytes], V::Undefined);
        Ok(())
    }
    fn source_main_fs_copy_once(&mut self, src: &str, dst: &str) -> R<()> {
        let mut files = self.source_main_fs_files()?;
        let args = vec![s(src), s(dst)];
        self.source_main_fs_attempt("copyFile", &args)?;
        let Some(row) = files
            .iter()
            .find(|row| arg(&array(row).unwrap_or_default(), 0) == s(src))
        else {
            return Err(self.source_main_fs_error("copyFile", args, "ENOENT", src));
        };
        let bytes = arg(&array(row)?, 1);
        if let Some(row) = files
            .iter_mut()
            .find(|row| arg(&array(row).unwrap_or_default(), 0) == s(dst))
        {
            *row = V::Array(vec![s(dst), bytes]);
        } else {
            files.push(V::Array(vec![s(dst), bytes]));
        }
        self.source_main_service_set("files", V::Array(files))?;
        self.source_main_fs_emit("copyFile", args, V::Undefined);
        Ok(())
    }
    fn source_main_fs_remove_once(&mut self, path: &str, tree: bool) -> R<()> {
        let (method, args) = if tree {
            (
                "rm",
                vec![
                    s(path),
                    obj(&[("recursive", V::Bool(true)), ("force", V::Bool(true))]),
                ],
            )
        } else {
            ("unlink", vec![s(path)])
        };
        self.source_main_fs_attempt(method, &args)?;
        let mut files = self.source_main_fs_files()?;
        let mut dirs = array(&self.source_main_service_get("directories")?)?;
        let found = files
            .iter()
            .any(|r| arg(&array(r).unwrap_or_default(), 0) == s(path));
        if !tree && !found {
            return Err(self.source_main_fs_error("unlink", vec![s(path)], "ENOENT", path));
        }
        let prefix = format!("{path}/");
        files.retain(|r| {
            let k = text(&arg(&array(r).unwrap_or_default(), 0)).unwrap_or_default();
            k != path && (!tree || !k.starts_with(&prefix))
        });
        if tree {
            dirs.retain(|k| {
                let k = text(k).unwrap_or_default();
                k != path && !k.starts_with(&prefix)
            });
        }
        self.source_main_service_set("files", V::Array(files))?;
        self.source_main_service_set("directories", V::Array(dirs))?;
        let (method, args) = if tree {
            (
                "rm",
                vec![
                    s(path),
                    obj(&[("recursive", V::Bool(true)), ("force", V::Bool(true))]),
                ],
            )
        } else {
            ("unlink", vec![s(path)])
        };
        self.source_main_fs_emit(method, args, V::Undefined);
        Ok(())
    }
    fn source_main_snapshot_list(&mut self) -> R<Vec<V>> {
        let p = self.source_main_service_get("persistence")?;
        let dir = text(&p.get("snapshotsDir"))?;
        let result = (|| -> R<Vec<V>> {
            let names = self.source_main_fs_names(&dir, true)?;
            let dirs = array(&self.source_main_service_get("directories")?)?;
            let mut snaps = vec![];
            for name in names {
                let name = text(&name)?;
                if !dirs.contains(&s(format!("{dir}/{name}"))) {
                    continue;
                }
                let loaded = (|| -> R<V> {
                    let raw = self.source_main_fs_read(&format!("{dir}/{name}/meta.json"))?;
                    let mut meta = parse(&text(&raw)?)?;
                    if matches!(meta, V::Null | V::Undefined) {
                        return Err(type_error("Cannot set properties of null (setting 'dir')"));
                    }
                    if !matches!(meta, V::Object(_)) {
                        if matches!(meta, V::Array(_)) {
                            return Err(SourceRuleError::UnknownDependency.into());
                        }
                        return Err(type_error(
                            "Cannot create property 'dir' on primitive snapshot metadata",
                        ));
                    }
                    set(&mut meta, "dir", s(name))?;
                    Ok(meta)
                })();
                match loaded {
                    Ok(v) => snaps.push(v),
                    Err(Fault::Thrown(_)) => (),
                    Err(e) => return Err(e),
                }
            }
            snaps.sort_by(|a, b| {
                number(&a.get("blockHeight"))
                    .unwrap_or(f64::NAN)
                    .partial_cmp(&number(&b.get("blockHeight")).unwrap_or(f64::NAN))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            Ok(snaps)
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(vec![]),
            other => other,
        }
    }
    fn source_main_snapshot_prune(&mut self, keep: f64) -> R<()> {
        let snaps = self.source_main_snapshot_list()?;
        let len = snaps.len() as f64;
        if len <= keep {
            return Ok(());
        }
        let end = len - keep;
        let n = if end.is_nan() {
            0
        } else {
            end.trunc().clamp(0., len) as usize
        };
        let p = self.source_main_service_get("persistence")?;
        let dir = text(&p.get("snapshotsDir"))?;
        for snap in snaps.into_iter().take(n) {
            self.source_main_fs_remove(&format!("{dir}/{}", text(&snap.get("dir"))?), true)?;
        }
        Ok(())
    }
    fn source_main_snapshot_checkpoint(&mut self, height: V) -> R<()> {
        let p = self.source_main_service_get("persistence")?;
        let interval = number(&p.get("snapshotInterval"))?;
        if interval <= 0. || number(&height)? % interval != 0. {
            return Ok(());
        }
        let hash = self.source_activation_consensus_state_hash()?;
        let store = (|| -> R<()> {
            self.database("consensus")?;
            self.update(
                "consensus",
                obj(&[("_id", s(format!("block-{}", text(&height)?)))]),
                obj(&[(
                    "$set",
                    obj(&[
                        ("blockHeight", height.clone()),
                        ("consensusHash", hash.clone()),
                    ]),
                )]),
                true,
            )?;
            Ok(())
        })();
        match store {
            Ok(()) | Err(Fault::Thrown(_)) => (),
            Err(e) => return Err(e),
        }
        self.source_main_snapshot_create(height, hash)
    }
    fn source_main_snapshot_create(&mut self, height: V, hash: V) -> R<()> {
        let p = self.source_main_service_get("persistence")?;
        if number(&p.get("snapshotInterval"))? <= 0. {
            return Ok(());
        }
        let selected = if truth(&hash) {
            hash.clone()
        } else {
            s("nohash")
        };
        let V::String(selected) = selected else {
            return Err(type_error(
                "(consensusHash || \"nohash\").slice is not a function",
            ));
        };
        let short = selected.chars().take(12).collect::<String>();
        let dir = format!(
            "{}/{}-{short}",
            text(&p.get("snapshotsDir"))?,
            text(&height)?
        );
        self.source_main_service_mkdir(dir.clone())?;
        let dbdir = text(&p.get("dbDir"))?;
        let files = self.source_main_fs_names(&dbdir, false)?;
        let mut copied = vec![];
        for file in files {
            let file = text(&file)?;
            if !file.ends_with(".db") {
                continue;
            }
            self.source_main_fs_copy(&format!("{dbdir}/{file}"), &format!("{dir}/{file}"))?;
            copied.push(s(file));
        }
        let meta = obj(&[
            ("blockHeight", height.clone()),
            ("consensusHash", hash),
            ("createdAt", s(source_main_snapshot_iso(self.time_millis)?)),
            ("files", V::Array(copied)),
        ]);
        self.source_main_fs_write(
            &format!("{dir}/meta.json"),
            s(source_main_snapshot_pretty(&meta)?),
        )?;
        self.source_main_snapshot_prune(2.)?;
        self.source_main_height_write("MaxProcessedHeight", height.clone())?;
        self.database("persistence")?;
        self.db(
            "persistence",
            "removeAsync",
            vec![
                obj(&[("height", obj(&[("$lt", n(number(&height)? - 1000.))]))]),
                obj(&[("multi", V::Bool(true))]),
            ],
        )?;
        Ok(())
    }

    fn source_main_snapshot_restore(&mut self, meta: V) -> R<()> {
        let p = self.source_main_service_get("persistence")?;
        let snapdir = text(&p.get("snapshotsDir"))?;
        let selected = funding_prop(&meta, "dir")?;
        let selected = if truth(&selected) {
            selected
        } else {
            s(text(&funding_prop(&meta, "blockHeight")?)?)
        };
        let dir =
            source_main_snapshot_join(&snapdir, &source_main_snapshot_path_argument(&selected)?);
        let backups = text(&p.get("backupsDir"))?;
        let prune = (|| -> R<()> {
            let names = self.source_main_fs_names(&backups, false)?;
            let mut old: Vec<(V, String)> = vec![];
            for name in names {
                let name = text(&name)?;
                let path = format!("{backups}/{name}");
                let stat = self.source_main_fs_stat(&path)?;
                if truth(&stat.get("directory")) && name.starts_with("backup-") {
                    old.push((stat.get("mtimeMs"), path));
                }
            }
            source_v8_stable_sort(&mut old, |a, b| -> R<core::cmp::Ordering> {
                Ok((amm_quote_number(&b.0)? - amm_quote_number(&a.0)?)
                    .partial_cmp(&0.0)
                    .unwrap_or(core::cmp::Ordering::Equal))
            })?;
            for (_, path) in old.into_iter().skip(1) {
                match self.source_main_fs_remove(&path, true) {
                    Ok(()) | Err(Fault::Thrown(_)) => (),
                    Err(e) => return Err(e),
                }
            }
            Ok(())
        })();
        match prune {
            Ok(()) | Err(Fault::Thrown(_)) => (),
            Err(e) => return Err(e),
        }
        let backup = format!("{backups}/backup-{}", self.time_millis);
        self.source_main_service_mkdir(backup.clone())?;
        let dbdir = text(&p.get("dbDir"))?;
        let current = self.source_main_fs_names(&dbdir, false)?;
        for file in &current {
            let file = text(file)?;
            self.source_main_fs_copy(&format!("{dbdir}/{file}"), &format!("{backup}/{file}"))?;
        }
        for file in current {
            let file = text(&file)?;
            if file.ends_with(".db") {
                self.source_main_fs_remove(&format!("{dbdir}/{file}"), false)?;
            }
        }
        let files = match funding_prop(&meta, "files")? {
            V::Array(files) => files,
            V::String(files) => files.chars().map(|c| s(c.to_string())).collect(),
            _ => return Err(type_error("meta.files is not iterable")),
        };
        for file in files {
            let file = source_main_snapshot_path_argument(&file)?;
            self.source_main_fs_copy(
                &source_main_snapshot_join(&dir, &file),
                &source_main_snapshot_join(&dbdir, &file),
            )?;
        }
        // Original saveTrackHeight is unawaited. In the admitted restore/Main
        // return graph no later IO precedes its update. Drain at that boundary;
        // native rejection remains an unhandled rejection, not restore failure.
        let save = self.source_main_height_write("TrackHeight", meta.get("blockHeight"));
        match save {
            Ok(_) => (),
            Err(Fault::Thrown(e)) => self
                .source_unhandled_rejections
                .push(obj(&[("name", s(e.name)), ("message", s(e.message))])),
            Err(e) => return Err(e),
        }
        Ok(())
    }
    fn source_main_snapshot_reorg_finish(&mut self, ancestor: f64, last: V) -> R<V> {
        let snaps = self.source_main_snapshot_list()?;
        let mut best = None;
        for snap in &snaps {
            if number(&snap.get("blockHeight"))? <= number(&last.get("height"))?
                && best.as_ref().is_none_or(|old: &V| {
                    number(&snap.get("blockHeight")).unwrap_or(f64::NAN)
                        > number(&old.get("blockHeight")).unwrap_or(f64::NAN)
                })
            {
                best = Some(snap.clone());
            }
        }
        if best.is_none() {
            best = snaps.first().cloned();
        }
        let restored = if let Some(best) = best {
            self.source_main_snapshot_restore(best.clone())?;
            best.get("blockHeight")
        } else {
            n(0.)
        };
        Ok(obj(&[
            ("restoredFrom", restored),
            ("commonAncestor", n(ancestor)),
            ("lastLocal", last.get("height")),
        ]))
    }
}
impl SourceContractRuntime {
    fn source_main_snapshot_dispatch(&mut self, target: &str, a: &[V]) -> R<V> {
        match target {
            "Main.delay" => {
                self.core.source_main_service_timer(arg(a, 0));
                Ok(V::Undefined)
            }
            "Main.setPause" => {
                let paused = !truth(&self.core.source_main_service_get("pause")?);
                self.core
                    .source_main_service_set("pause", V::Bool(paused))?;
                Ok(V::Bool(paused))
            }
            "Main.getGenesisBlock" => {
                let receiver = self.core.source_main_snapshot_main_instance()?;
                self.core.source_main_snapshot_main_genesis(receiver)
            }
            "Main.getLastBlock" => {
                Err(type_error("TxUtils.getLatestBlockHeight is not a function"))
            }
            target if target.starts_with("Persistence.") => {
                if target == "Persistence.getInstance" {
                    self.core.source_main_service_persistence_options(a)?;
                    return Ok(obj(&[("$persistenceInstance", n(1.))]));
                }
                self.core.source_main_service_persistence()?;
                match target {
                    "Persistence.listSnapshots" => {
                        Ok(V::Array(self.core.source_main_snapshot_list()?))
                    }
                    "Persistence.createSnapshot" => {
                        self.core
                            .source_main_snapshot_create(arg(a, 0), arg(a, 1))?;
                        Ok(V::Undefined)
                    }
                    "Persistence.maybeCheckpoint" => {
                        self.core.source_main_snapshot_checkpoint(arg(a, 0))?;
                        Ok(V::Undefined)
                    }
                    "Persistence.restoreSnapshot" => {
                        self.core.source_main_snapshot_restore(arg(a, 0))?;
                        Ok(V::Undefined)
                    }
                    "Persistence.pruneOldSnapshots" => {
                        let keep = if arg(a, 0) == V::Undefined {
                            2.
                        } else {
                            number(&arg(a, 0))?
                        };
                        self.core.source_main_snapshot_prune(keep)?;
                        Ok(V::Undefined)
                    }
                    _ => self.core.source_main_file_direct(target, a),
                }
            }
            _ => self.source_main_startup_dispatch(target, a),
        }
    }
}
fn source_main_snapshot_join(a: &str, b: &str) -> String {
    let joined = format!("{a}/{b}");
    let mut parts = vec![];
    for part in joined.split('/') {
        match part {
            "" | "." => (),
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    format!("/{}", parts.join("/"))
}
fn source_main_snapshot_iso(millis: u64) -> R<String> {
    // ECMAScript Date finite positive millisecond domain used by the actual
    // recorded clock. Date arithmetic does not read the host clock.
    if millis > 8_640_000_000_000_000 {
        return Err(Fault::Thrown(crate::validity_source::SourceException {
            name: "RangeError".into(),
            message: "Invalid time value".into(),
        }));
    }
    let seconds = millis / 1000;
    let days = (seconds / 86400) as i64;
    let daytime = seconds % 86400;
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += i64::from(m <= 2);
    let year = if y > 9999 {
        format!("+{y:06}")
    } else {
        format!("{y:04}")
    };
    Ok(format!(
        "{year}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        daytime / 3600,
        (daytime / 60) % 60,
        daytime % 60,
        millis % 1000
    ))
}
fn source_main_snapshot_pretty(value: &V) -> R<String> {
    let compact = json(value)?;
    let chars = compact.chars().collect::<Vec<_>>();
    let mut out = String::new();
    let (mut quote, mut escape, mut indent) = (false, false, 0usize);
    for (i, c) in chars.iter().copied().enumerate() {
        if quote {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                quote = false;
            }
            continue;
        }
        match c {
            '"' => {
                quote = true;
                out.push(c)
            }
            '{' | '[' => {
                out.push(c);
                if chars.get(i + 1) != Some(&if c == '{' { '}' } else { ']' }) {
                    indent += 1;
                    out.push('\n');
                    out.push_str(&"  ".repeat(indent));
                }
            }
            '}' | ']' => {
                if chars.get(i.wrapping_sub(1)) != Some(&if c == '}' { '{' } else { '[' }) {
                    indent = indent.saturating_sub(1);
                    out.push('\n');
                    out.push_str(&"  ".repeat(indent));
                }
                out.push(c)
            }
            ',' => {
                out.push_str(",\n");
                out.push_str(&"  ".repeat(indent));
            }
            ':' => out.push_str(": "),
            other => out.push(other),
        }
    }
    Ok(out)
}

fn source_main_snapshot_path_argument(value: &V) -> R<String> {
    let description = match value {
        V::String(value) => return Ok(value.clone()),
        V::Undefined => "undefined".to_string(),
        V::Null => "null".to_string(),
        V::Bool(value) => format!("type boolean ({value})"),
        V::Number(bits) => {
            let value = if f64::from_bits(*bits) == 0. && f64::from_bits(*bits).is_sign_negative() {
                "-0".to_string()
            } else {
                text(value)?
            };
            format!("type number ({value})")
        }
        V::Array(_) => "an instance of Array".to_string(),
        V::Object(_) => "an instance of Object".to_string(),
        V::Promise => "an instance of Promise".to_string(),
        V::BigNumber(_) => "an instance of BigNumber".to_string(),
    };
    Err(type_error(format!(
        "The \"path\" argument must be of type string. Received {description}"
    )))
}

include!("source_main_file_io_impl.rs");
