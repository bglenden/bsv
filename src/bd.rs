use anyhow::{Context, Result};
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
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

#[allow(dead_code)]
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
        .args(["ready", "--json", "--limit", "0"])
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

/// List all issues with full details including dependencies.
/// This calls `bd show` with all issue IDs to get complete data.
pub fn list_issues_with_details() -> Result<Vec<Issue>> {
    // First get the list of issue IDs
    let list_output = Command::new("bd")
        .args(["list", "--status=all", "--json"])
        .output()
        .context("Failed to run bd list")?;

    if !list_output.status.success() {
        let stderr = String::from_utf8_lossy(&list_output.stderr);
        anyhow::bail!("bd list failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let basic_issues: Vec<Issue> = serde_json::from_str(&stdout)
        .context("Failed to parse bd list output")?;

    if basic_issues.is_empty() {
        return Ok(vec![]);
    }

    // Get all issue IDs
    let ids: Vec<&str> = basic_issues.iter().map(|i| i.id.as_str()).collect();

    // Call bd show with all IDs to get full details including dependencies
    let mut args = vec!["show", "--json"];
    args.extend(ids);

    let show_output = Command::new("bd")
        .args(&args)
        .output()
        .context("Failed to run bd show")?;

    if !show_output.status.success() {
        // Fall back to basic list if show fails
        return Ok(basic_issues);
    }

    let show_stdout = String::from_utf8_lossy(&show_output.stdout);
    let detailed_issues: Vec<Issue> = serde_json::from_str(&show_stdout)
        .unwrap_or(basic_issues);

    Ok(detailed_issues)
}

/// Update an issue's title
pub fn update_issue_title(id: &str, title: &str) -> Result<()> {
    let output = Command::new("bd")
        .args(["update", id, "--title", title])
        .output()
        .context("Failed to run bd update")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("bd update failed: {}", stderr);
    }

    Ok(())
}

/// Update an issue's description
pub fn update_issue_description(id: &str, description: &str) -> Result<()> {
    let output = Command::new("bd")
        .args(["update", id, "--description", description])
        .output()
        .context("Failed to run bd update")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("bd update failed: {}", stderr);
    }

    Ok(())
}
