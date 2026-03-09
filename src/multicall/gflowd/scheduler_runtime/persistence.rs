use super::*;

impl SchedulerRuntime {
    /// Save scheduler state to disk asynchronously
    pub async fn save_state(&mut self) {
        if !self.state_writable {
            self.append_journal_snapshot().await;
            return;
        }

        let state_dir = self
            .scheduler
            .state_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));

        match serialization::save_state(
            &self.scheduler,
            state_dir,
            serialization::SerializationFormat::MessagePack,
        ) {
            Ok(_) => {
                if self.journal_applied {
                    if let Err(e) = tokio::fs::OpenOptions::new()
                        .write(true)
                        .truncate(true)
                        .open(&self.journal_path)
                        .await
                    {
                        tracing::warn!(
                            "Failed to truncate journal file {}: {}",
                            self.journal_path.display(),
                            e
                        );
                    } else {
                        self.journal_applied = false;
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to save scheduler state: {}", e);
            }
        }
    }

    /// Mark state as dirty without saving immediately
    pub(super) fn mark_dirty(&mut self) {
        if !(self.state_writable || self.journal_writable) {
            return;
        }
        self.dirty = true;
        if let Some(ref saver) = self.state_saver {
            saver.mark_dirty();
        }
    }

    /// Save state only if dirty flag is set, then clear flag
    pub async fn save_state_if_dirty(&mut self) {
        if self.dirty {
            self.save_state().await;
            if self.state_writable || self.journal_writable {
                self.dirty = false;
            }
        }
    }

    /// Set the state saver handle for async background persistence
    pub fn set_state_saver(&mut self, saver: StateSaverHandle) {
        let should_kick = self.dirty;
        self.state_saver = Some(saver);
        if should_kick {
            if let Some(ref saver) = self.state_saver {
                saver.mark_dirty();
            }
        }
    }

    /// Load scheduler state from disk
    pub fn load_state(&mut self) {
        self.state_writable = true;
        self.state_load_error = None;
        self.state_backup_path = None;
        self.journal_applied = false;

        let state_dir = self
            .scheduler
            .state_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        let mut loaded: Option<Scheduler> = None;

        match serialization::load_state_auto(&state_dir) {
            Ok(Some(loaded_scheduler)) => {
                match gflow::core::migrations::migrate_state(loaded_scheduler) {
                    Ok(migrated) => {
                        loaded = Some(migrated);
                    }
                    Err(e) => {
                        let json_path = state_dir.join("state.json");
                        let msgpack_path = state_dir.join("state.msgpack");
                        let backup_path = if msgpack_path.exists() {
                            &msgpack_path
                        } else {
                            &json_path
                        };

                        let (backup_result, backup_err) = backup_state_file(backup_path, "backup");
                        if let Some(err) = backup_err {
                            tracing::error!("Failed to backup state file: {}", err);
                        }

                        self.state_writable = false;
                        self.state_load_error = Some(format!(
                        "State migration failed: {e}. gflowd entered recovery mode (journal) to avoid overwriting your state file."
                    ));
                        self.state_backup_path = backup_result;
                        tracing::error!("{}", self.state_load_error.as_deref().unwrap());

                        if let Ok(Some(scheduler)) = serialization::load_state_auto(&state_dir) {
                            loaded = Some(scheduler);
                        }
                    }
                }
            }
            Ok(None) => {
                tracing::info!(
                    "No existing state file found in {}, starting fresh",
                    state_dir.display()
                );
            }
            Err(e) => {
                let json_path = state_dir.join("state.json");
                let msgpack_path = state_dir.join("state.msgpack");
                let failed_path = if msgpack_path.exists() {
                    &msgpack_path
                } else {
                    &json_path
                };

                let (backup_result, backup_err) = backup_state_file(failed_path, "corrupt");
                if let Some(err) = backup_err {
                    tracing::error!("Failed to backup corrupted state file: {}", err);
                }

                self.state_writable = false;
                self.state_load_error = Some(format!(
                    "Failed to load state file from {}: {e}. gflowd entered recovery mode (journal) to avoid overwriting your state file.",
                    state_dir.display()
                ));
                self.state_backup_path = backup_result;
                tracing::error!("{}", self.state_load_error.as_deref().unwrap());

                self.scheduler.set_next_job_id(2_000_000_000);
            }
        }

        let legacy_json_path = state_dir.join("state.json");
        if should_apply_journal(&legacy_json_path, &self.journal_path) {
            if let Some((snapshot, ts)) = load_last_journal_snapshot(&self.journal_path) {
                tracing::warn!(
                    "Loading scheduler state from journal snapshot (ts={}) at {}",
                    ts,
                    self.journal_path.display()
                );
                loaded = Some(snapshot);
                self.journal_applied = true;
                if self.state_writable {
                    self.dirty = true;
                }
            }
        }

        if let Some(scheduler) = loaded {
            self.apply_loaded_scheduler(scheduler);
        }

        self.reinitialize_runtime_resources();
    }

    pub(super) fn init_journal(&mut self) {
        self.journal_writable = false;
        self.journal_error = None;

        if let Some(parent) = self.journal_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                self.journal_error = Some(format!("Failed to create journal dir: {e}"));
                tracing::error!("{}", self.journal_error.as_deref().unwrap());
                return;
            }
        }

        match std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&self.journal_path)
        {
            Ok(_) => {
                self.journal_writable = true;
            }
            Err(e) => {
                self.journal_error = Some(format!(
                    "Failed to open journal file {}: {e}",
                    self.journal_path.display()
                ));
                tracing::error!("{}", self.journal_error.as_deref().unwrap());
            }
        }
    }

    fn apply_loaded_scheduler(&mut self, loaded: Scheduler) {
        self.scheduler.apply_persisted_state(loaded);
        self.scheduler.rebuild_user_jobs_index();
    }

    fn reinitialize_runtime_resources(&mut self) {
        match Nvml::init() {
            Ok(nvml) => {
                self.scheduler.update_gpu_slots(Self::get_gpus(&nvml));
                self.nvml = Some(nvml);
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize NVML during state load: {}. Running without GPU support.",
                    e
                );
                self.scheduler.update_gpu_slots(HashMap::new());
                self.nvml = None;
            }
        }

        let total_memory_mb = Self::get_total_system_memory_mb();
        self.scheduler.update_memory(total_memory_mb);
        self.scheduler.refresh_available_memory();
    }

    async fn append_journal_snapshot(&mut self) {
        if !self.journal_writable {
            tracing::error!(
                "Refusing to persist state: state.json is not writable and journal is not writable"
            );
            return;
        }

        if let Some(parent) = self.journal_path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                tracing::error!(
                    "Failed to create journal directory {}: {}",
                    parent.display(),
                    e
                );
                self.journal_writable = false;
                self.journal_error = Some(format!("Failed to create journal dir: {e}"));
                return;
            }
        }

        #[derive(serde::Serialize)]
        struct SchedulerSnapshot<'a> {
            version: u32,
            jobs: Vec<Job>,
            state_path: &'a std::path::PathBuf,
            next_job_id: u32,
            allowed_gpu_indices: Option<&'a Vec<u32>>,
            reservations: &'a Vec<gflow::core::reservation::GpuReservation>,
            next_reservation_id: u32,
        }

        #[derive(serde::Serialize)]
        struct JournalEntry<'a> {
            ts: u64,
            kind: &'static str,
            scheduler: SchedulerSnapshot<'a>,
        }

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let entry = JournalEntry {
            ts,
            kind: "snapshot",
            scheduler: SchedulerSnapshot {
                version: self.scheduler.version,
                jobs: self.scheduler.jobs_as_vec(),
                state_path: self.scheduler.state_path(),
                next_job_id: self.scheduler.next_job_id(),
                allowed_gpu_indices: self.scheduler.allowed_gpu_indices(),
                reservations: &self.scheduler.reservations,
                next_reservation_id: self.scheduler.next_reservation_id,
            },
        };

        let line = match serde_json::to_string(&entry) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to serialize journal entry: {}", e);
                return;
            }
        };

        let tmp_path = self.journal_path.with_extension("jsonl.tmp");
        match tokio::fs::File::create(&tmp_path).await {
            Ok(mut file) => {
                if let Err(e) =
                    tokio::io::AsyncWriteExt::write_all(&mut file, format!("{line}\n").as_bytes())
                        .await
                {
                    tracing::error!(
                        "Failed to write journal snapshot to {}: {}",
                        tmp_path.display(),
                        e
                    );
                    return;
                }

                if let Err(e) = file.sync_all().await {
                    tracing::warn!(
                        "Failed to fsync journal temp file {}: {}",
                        tmp_path.display(),
                        e
                    );
                }

                if let Err(e) = tokio::fs::rename(&tmp_path, &self.journal_path).await {
                    let _ = tokio::fs::remove_file(&self.journal_path).await;
                    if let Err(e2) = tokio::fs::rename(&tmp_path, &self.journal_path).await {
                        tracing::error!(
                            "Failed to move journal snapshot from {} to {}: {} (retry error: {})",
                            tmp_path.display(),
                            self.journal_path.display(),
                            e,
                            e2
                        );
                        self.journal_writable = false;
                        self.journal_error =
                            Some(format!("Failed to finalize journal snapshot: {e2}"));
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    "Failed to create journal temp file {}: {}",
                    tmp_path.display(),
                    e
                );
                self.journal_writable = false;
                self.journal_error = Some(format!("Failed to create journal temp file: {e}"));
            }
        };
    }
}

fn should_apply_journal(state_path: &std::path::Path, journal_path: &std::path::Path) -> bool {
    let Ok(j_meta) = std::fs::metadata(journal_path) else {
        return false;
    };
    if j_meta.len() == 0 {
        return false;
    }
    let Ok(j_mtime) = j_meta.modified() else {
        return true;
    };

    let Ok(s_meta) = std::fs::metadata(state_path) else {
        return true;
    };
    let Ok(s_mtime) = s_meta.modified() else {
        return true;
    };

    j_mtime >= s_mtime
}

fn load_last_journal_snapshot(journal_path: &std::path::Path) -> Option<(Scheduler, u64)> {
    #[derive(serde::Deserialize)]
    struct Entry {
        ts: u64,
        kind: String,
        scheduler: Scheduler,
    }

    let content = std::fs::read_to_string(journal_path).ok()?;
    let line = content.lines().next()?.trim();
    if line.is_empty() {
        return None;
    }
    let entry = serde_json::from_str::<Entry>(line).ok()?;
    if entry.kind != "snapshot" {
        return None;
    }
    Some((entry.scheduler, entry.ts))
}

fn backup_state_file(
    path: &std::path::Path,
    kind: &str,
) -> (Option<PathBuf>, Option<anyhow::Error>) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let backup_name = match path.file_name().and_then(|n| n.to_str()) {
        Some(name) => format!("{name}.{kind}.{ts}"),
        None => format!("state.{kind}.{ts}"),
    };
    let backup_path = path.with_file_name(backup_name);
    match std::fs::copy(path, &backup_path) {
        Ok(_) => (Some(backup_path), None),
        Err(e) => (None, Some(anyhow::anyhow!(e))),
    }
}
