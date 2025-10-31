use anyhow::{Context, Result};
use gflow::core::get_data_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const HISTORY_FILENAME: &str = "gbatch_history.json";
const MAX_ENTRIES: usize = 256;

#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryData {
    submissions: Vec<u32>,
}

#[derive(Debug)]
pub struct SubmissionHistory {
    path: PathBuf,
    data: HistoryData,
}

impl SubmissionHistory {
    pub fn load() -> Result<Self> {
        let data_dir = get_data_dir().context("Failed to locate gflow data directory")?;
        Self::load_from_dir(data_dir)
    }

    pub(crate) fn load_from_dir(dir: PathBuf) -> Result<Self> {
        if !dir.exists() {
            fs::create_dir_all(&dir)
                .with_context(|| format!("Failed to create data directory at {}", dir.display()))?;
        }
        let path = dir.join(HISTORY_FILENAME);
        Self::load_from_path(path)
    }

    fn load_from_path(path: PathBuf) -> Result<Self> {
        let data = if path.exists() {
            let contents = fs::read_to_string(&path).with_context(|| {
                format!("Failed to read submission history at {}", path.display())
            })?;
            if contents.trim().is_empty() {
                HistoryData::default()
            } else {
                serde_json::from_str::<HistoryData>(&contents).with_context(|| {
                    format!("Failed to parse submission history at {}", path.display())
                })?
            }
        } else {
            HistoryData::default()
        };

        Ok(Self { path, data })
    }

    pub fn len(&self) -> usize {
        self.data.submissions.len()
    }

    pub fn recent(&self, from_end: usize) -> Option<u32> {
        if from_end == 0 {
            return None;
        }
        let len = self.data.submissions.len();
        if from_end > len {
            None
        } else {
            self.data.submissions.get(len - from_end).copied()
        }
    }

    pub fn record(&mut self, submission_id: u32) -> Result<()> {
        self.data.submissions.push(submission_id);
        if self.data.submissions.len() > MAX_ENTRIES {
            let drain_count = self.data.submissions.len() - MAX_ENTRIES;
            self.data.submissions.drain(0..drain_count);
        }

        let serialized =
            serde_json::to_string(&self.data).context("Failed to serialize submission history")?;
        fs::write(&self.path, serialized).with_context(|| {
            format!(
                "Failed to write submission history to {}",
                self.path.display()
            )
        })?;
        Ok(())
    }
}
