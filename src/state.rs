use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct State {
    #[serde(default)]
    pub seen: HashMap<String, DateTime<Utc>>,
    #[serde(default)]
    pub issue_timestamps: HashMap<String, DateTime<Utc>>,
    #[serde(default)]
    pub last_check: Option<DateTime<Utc>>,
}

pub fn state_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let dir = PathBuf::from(home).join(".local/share/work-driver");
    fs::create_dir_all(&dir).context("Failed to create state directory")?;
    Ok(dir.join("state.json"))
}

pub fn load_state() -> Result<State> {
    let path = state_path()?;
    if !path.exists() {
        return Ok(State::default());
    }
    let content = fs::read_to_string(&path).context("Failed to read state file")?;
    serde_json::from_str(&content).context("Failed to parse state file")
}

pub fn save_state(state: &State) -> Result<()> {
    let path = state_path()?;
    let tmp_path = path.with_extension("json.tmp");
    let content = serde_json::to_string_pretty(state).context("Failed to serialize state")?;
    fs::write(&tmp_path, content).context("Failed to write temp state file")?;
    fs::rename(&tmp_path, &path).context("Failed to rename temp state file")?;
    Ok(())
}
