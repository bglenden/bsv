use anyhow::{Context, Result};
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Clone, Deserialize)]
pub struct Issue {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub status: String,
    pub priority: i32,
    pub issue_type: String,
    pub created_at: String,
    #[serde(default)]
    pub created_by: Option<String>,
    pub updated_at: String,
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub dependencies: Option<Vec<Dependency>>,
    #[serde(default)]
    pub dependents: Option<Vec<Dependency>>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub design: Option<String>,
    #[serde(default)]
    pub acceptance_criteria: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dependency {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub dependency_type: Option<String>,
}

pub fn list_issues() -> Result<Vec<Issue>> {
    // Use --status=all to include closed issues
    let output = Command::new("bd")
        .args(["list", "--status=all", "--json"])
        .output()
        .context("Failed to run bd list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("bd list failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let issues: Vec<Issue> = serde_json::from_str(&stdout)
        .context("Failed to parse bd list output")?;

    Ok(issues)
}

pub fn get_ready_ids() -> Result<std::collections::HashSet<String>> {
    let output = Command::new("bd")
        .args(["ready", "--json"])
        .output()
        .context("Failed to run bd ready")?;

    if !output.status.success() {
        // If bd ready fails, return empty set (treat all as not ready)
        return Ok(std::collections::HashSet::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let issues: Vec<Issue> = serde_json::from_str(&stdout).unwrap_or_default();

    Ok(issues.into_iter().map(|i| i.id).collect())
}

pub fn get_issue_details(id: &str) -> Result<Option<Issue>> {
    let output = Command::new("bd")
        .args(["show", id, "--json"])
        .output()
        .context("Failed to run bd show")?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let issues: Vec<Issue> = serde_json::from_str(&stdout).unwrap_or_default();

    Ok(issues.into_iter().next())
}
