use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AppState {
    pub projects: HashMap<String, ProjectState>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProjectState {
    pub expanded: HashSet<String>,
}

fn state_file_path() -> Option<PathBuf> {
    dirs::home_dir().map(|p| p.join(".config").join("bsv").join("state.json"))
}

pub fn load_state() -> AppState {
    state_file_path()
        .and_then(|path| fs::read_to_string(&path).ok())
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

pub fn save_state(state: &AppState) -> Result<()> {
    if let Some(path) = state_file_path() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(state)?;
        fs::write(&path, json)?;
    }
    Ok(())
}

pub fn get_project_key() -> String {
    // Use beads database path as key (from bd info --json)
    // This ensures same expand state regardless of which subdirectory you run from
    use std::process::Command;

    if let Ok(output) = Command::new("bd").args(["info", "--json"]).output() {
        if output.status.success() {
            if let Ok(stdout) = String::from_utf8(output.stdout) {
                if let Ok(info) = serde_json::from_str::<serde_json::Value>(&stdout) {
                    if let Some(db_path) = info.get("database_path").and_then(|v| v.as_str()) {
                        return db_path.to_string();
                    }
                }
            }
        }
    }

    // Fallback to current directory if bd info fails
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "default".to_string())
}

pub fn load_expanded() -> HashSet<String> {
    let state = load_state();
    let key = get_project_key();
    state.projects.get(&key)
        .map(|p| p.expanded.clone())
        .unwrap_or_default()
}

pub fn save_expanded(expanded: &HashSet<String>) -> Result<()> {
    let mut state = load_state();
    let key = get_project_key();
    state.projects.insert(key, ProjectState {
        expanded: expanded.clone(),
    });
    save_state(&state)
}
