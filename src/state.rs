use crate::HierarchyMode;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AppState {
    pub projects: HashMap<String, ProjectState>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ProjectState {
    pub expanded: HashSet<String>,
    #[serde(default)]
    pub dep_expanded: HashSet<String>,
    #[serde(default)]
    pub hierarchy_mode: Option<HierarchyMode>,
    #[serde(default)]
    pub panel_ratio: Option<f32>,
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

#[allow(dead_code)]
pub fn load_expanded() -> HashSet<String> {
    let state = load_state();
    let key = get_project_key();
    state.projects.get(&key)
        .map(|p| p.expanded.clone())
        .unwrap_or_default()
}

/// Load all tree-related state: expanded (ID mode), dep_expanded (Dep mode), and hierarchy_mode
pub fn load_tree_state() -> (HashSet<String>, HashSet<String>, HierarchyMode) {
    let state = load_state();
    let key = get_project_key();
    if let Some(project) = state.projects.get(&key) {
        (
            project.expanded.clone(),
            project.dep_expanded.clone(),
            project.hierarchy_mode.unwrap_or_default(),
        )
    } else {
        (HashSet::new(), HashSet::new(), HierarchyMode::default())
    }
}

pub fn save_expanded(expanded: &HashSet<String>) -> Result<()> {
    let mut state = load_state();
    let key = get_project_key();
    let existing = state.projects.get(&key).cloned().unwrap_or_default();
    state.projects.insert(key, ProjectState {
        expanded: expanded.clone(),
        dep_expanded: existing.dep_expanded,
        hierarchy_mode: existing.hierarchy_mode,
        panel_ratio: existing.panel_ratio,
    });
    save_state(&state)
}

/// Save the full tree state
pub fn save_tree_state(
    expanded: &HashSet<String>,
    dep_expanded: &HashSet<String>,
    hierarchy_mode: HierarchyMode,
) -> Result<()> {
    let mut state = load_state();
    let key = get_project_key();
    let existing = state.projects.get(&key).cloned().unwrap_or_default();
    state.projects.insert(key, ProjectState {
        expanded: expanded.clone(),
        dep_expanded: dep_expanded.clone(),
        hierarchy_mode: Some(hierarchy_mode),
        panel_ratio: existing.panel_ratio,
    });
    save_state(&state)
}

const DEFAULT_PANEL_RATIO: f32 = 0.4;

/// Load panel ratio (defaults to 0.4 = 40% left panel)
pub fn load_panel_ratio() -> f32 {
    let state = load_state();
    let key = get_project_key();
    state.projects.get(&key)
        .and_then(|p| p.panel_ratio)
        .unwrap_or(DEFAULT_PANEL_RATIO)
}

/// Save panel ratio
pub fn save_panel_ratio(ratio: f32) -> Result<()> {
    let mut state = load_state();
    let key = get_project_key();
    let existing = state.projects.get(&key).cloned().unwrap_or_default();
    state.projects.insert(key, ProjectState {
        expanded: existing.expanded,
        dep_expanded: existing.dep_expanded,
        hierarchy_mode: existing.hierarchy_mode,
        panel_ratio: Some(ratio),
    });
    save_state(&state)
}
