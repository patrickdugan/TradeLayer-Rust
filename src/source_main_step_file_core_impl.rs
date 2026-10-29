// Literal source prefixes/suffixes split only at safeCopy/safeDelete awaits.
// No supplied financial result or alternate state owner.
impl ContractCore {
    fn source_main_file_checkpoint_prepare(&mut self, height: V) -> R<V> {
        let p = self.source_main_service_get("persistence")?;
        let interval = number(&p.get("snapshotInterval"))?;
        if interval <= 0. || number(&height)? % interval != 0. {
            return Ok(obj(&[("run", V::Bool(false))]));
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
        Ok(obj(&[("run", V::Bool(true)), ("hash", hash)]))
    }
    fn source_main_file_create_begin(&mut self, height: V, hash: V) -> R<V> {
        let p = self.source_main_service_get("persistence")?;
        if number(&p.get("snapshotInterval"))? <= 0. {
            return Ok(obj(&[("run", V::Bool(false))]));
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
        Ok(obj(&[
            ("run", V::Bool(true)),
            ("dir", s(dir)),
            ("dbDir", s(dbdir)),
            ("files", V::Array(files)),
        ]))
    }
    fn source_main_file_create_finish(
        &mut self,
        height: V,
        hash: V,
        context: V,
        copied: Vec<V>,
    ) -> R<V> {
        let dir = text(&context.get("dir"))?;
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
        Ok(V::Undefined)
    }
    fn source_main_file_restore_begin(&mut self, meta: V) -> R<V> {
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
        Ok(obj(&[
            ("dir", s(dir)),
            ("dbDir", s(dbdir)),
            ("backup", s(backup)),
            ("current", V::Array(current)),
        ]))
    }
    fn source_main_file_restore_files(&mut self, meta: V) -> R<V> {
        let files = match funding_prop(&meta, "files")? {
            V::Array(files) => files,
            V::String(files) => files.chars().map(|c| s(c.to_string())).collect(),
            _ => return Err(type_error("meta.files is not iterable")),
        };
        Ok(V::Array(files))
    }
    fn source_main_file_restore_finish(&mut self, meta: V) -> R<V> {
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
        Ok(V::Undefined)
    }
    fn source_main_file_reorg_plan(&mut self) -> R<V> {
        self.database("persistence")?;
        let mut rows = array(&self.db(
            "persistence",
            "findAsync",
            vec![obj(&[("height", obj(&[("$exists", V::Bool(true))]))])],
        )?)?;
        rows.sort_by(|a, b| {
            number(&a.get("height"))
                .unwrap_or(f64::NAN)
                .partial_cmp(&number(&b.get("height")).unwrap_or(f64::NAN))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let Some(last) = rows.last().cloned() else {
            return Ok(V::Null);
        };
        let mut h = number(&last.get("height"))?;
        let canonical = self.source_main_production_io("Client.getBlockHash", &[n(h)])?;
        if canonical == last.get("hash") {
            return Ok(V::Null);
        }
        let mut ancestor = 0.;
        let mut count = 0;
        while h > 0. {
            if h > 9_007_199_254_740_991. || !h.is_finite() {
                return Err(SourceRuleError::UnsupportedNumericDomain.into());
            }
            if count >= 1024 {
                return Err(SourceRuleError::Capacity.into());
            }
            count += 1;
            self.database("persistence")?;
            let local = self.find(
                "persistence",
                obj(&[("_id", s(format!("block-{}", text(&n(h))?)))]),
            )?;
            let node = self.source_main_production_io("Client.getBlockHash", &[n(h)])?;
            if truth(&local) && local.get("hash") == node {
                ancestor = h;
                break;
            }
            h -= 1.;
        }
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
        let restored = best.as_ref().map_or(n(0.), |meta| meta.get("blockHeight"));
        Ok(obj(&[
            ("restore", V::Bool(best.is_some())),
            ("meta", best.unwrap_or(V::Undefined)),
            (
                "info",
                obj(&[
                    ("restoredFrom", restored),
                    ("commonAncestor", n(ancestor)),
                    ("lastLocal", last.get("height")),
                ]),
            ),
        ]))
    }
}
