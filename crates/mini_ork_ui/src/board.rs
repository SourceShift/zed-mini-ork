//! The `mini-ork board` contract: what the panels read and the commands they run.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, anyhow};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Board {
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub runs: Vec<Run>,
    #[serde(default)]
    pub counts: Counts,
    #[serde(default)]
    pub learnings: Vec<Learning>,
    #[serde(default)]
    pub automations: Vec<Automation>,
    #[serde(default)]
    pub scheduler: Scheduler,
    #[serde(default)]
    pub recipes: Vec<Recipe>,
    #[serde(default)]
    pub workspaces: Vec<OpenWorkspace>,
    #[serde(default)]
    pub errors: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Counts {
    #[serde(default)]
    pub working: u64,
    #[serde(default)]
    pub needs_you: u64,
    #[serde(default)]
    pub done: u64,
    #[serde(default)]
    pub failed: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Run {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub recipe: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub step: String,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub cost_usd: f64,
    #[serde(default)]
    pub added: u64,
    #[serde(default)]
    pub removed: u64,
    #[serde(default)]
    pub has_workspace: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Learning {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub suggestion: String,
    #[serde(default)]
    pub confidence: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Automation {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub recipe: String,
    #[serde(default)]
    pub when: String,
    #[serde(default)]
    pub next: String,
    #[serde(default)]
    pub last_run: String,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Scheduler {
    #[serde(default)]
    pub installed: bool,
    #[serde(default)]
    pub last_tick: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Recipe {
    pub id: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub grade_letter: Option<String>,
    #[serde(default)]
    pub runs: Option<Value>,
    #[serde(default)]
    pub success_pct: Option<Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OpenWorkspace {
    pub run_id: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub added: u64,
    #[serde(default)]
    pub removed: u64,
    #[serde(default)]
    pub files: Vec<String>,
}

/// The `mini-ork` launcher: `$MINI_ORK_BIN`, else the managed launcher
/// `~/.local/bin/mini-ork` (written by `mini-ork install`; it pins mini-ork's
/// own Python), else the first `mini-ork` on `PATH`. The managed one comes
/// before `PATH` because a checkout's `bin/mini-ork` on `PATH` runs under
/// whatever `python3` is first, which may lack mini-ork's dependencies.
pub fn mini_ork_bin() -> PathBuf {
    if let Some(bin) = std::env::var_os("MINI_ORK_BIN") {
        return PathBuf::from(bin);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = Path::new(&home).join(".local/bin/mini-ork");
        if candidate.is_file() {
            return candidate;
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("mini-ork");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from("mini-ork")
}

/// The project's `.mini-ork` home: `<root>/.mini-ork`, or — for a linked git
/// worktree, which has none of its own — the main checkout's.
pub async fn resolve_home(root: &Path) -> Option<PathBuf> {
    let direct = root.join(".mini-ork");
    if direct.is_dir() {
        return Some(direct);
    }
    let output = util::command::new_command("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(root)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let common = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let home = common.parent()?.join(".mini-ork");
    home.is_dir().then_some(home)
}

async fn run_json(bin: &Path, args: &[String], home: &Path) -> Result<Value> {
    let output = util::command::new_command(bin)
        .args(args)
        .arg("--home")
        .arg(home)
        // An inherited flag would stop the launcher from switching to
        // mini-ork's own Python.
        .env_remove("MINI_ORK_VENV_ACTIVE")
        .output()
        .await
        .with_context(|| format!("running {}", bin.display()))?;
    let value: Value = serde_json::from_slice(&output.stdout).map_err(|_| {
        anyhow!(
            "mini-ork {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )
    })?;
    Ok(value)
}

pub async fn fetch_board(bin: PathBuf, home: PathBuf) -> Result<Board> {
    let value = run_json(&bin, &["board".into(), "--json".into()], &home).await?;
    serde_json::from_value(value).context("reading mini-ork board")
}

pub async fn fetch_run_card(bin: PathBuf, home: PathBuf, run_id: String) -> Result<String> {
    let value = run_json(&bin, &["board".into(), "run".into(), run_id], &home).await?;
    if let Some(markdown) = value.get("markdown").and_then(Value::as_str) {
        return Ok(markdown.to_string());
    }
    Err(anyhow!(
        "{}",
        value.get("error").and_then(Value::as_str).unwrap_or("no run card")
    ))
}

/// Run a mini-ork action (`board merge <id>`, `automations run <id>`, …) and
/// return its one-line outcome for the panel.
pub async fn run_action(bin: PathBuf, home: PathBuf, args: Vec<String>) -> Result<String> {
    let value = run_json(&bin, &args, &home).await?;
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(anyhow!(
            "{}",
            value.get("error").and_then(Value::as_str).unwrap_or("failed")
        ));
    }
    Ok(match value.get("mode").and_then(Value::as_str) {
        Some(mode) => format!("Done ({mode})."),
        None => "Done.".to_string(),
    })
}

pub fn value_text(value: &Option<Value>) -> String {
    match value {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(s)) => s.clone(),
        _ => "—".to_string(),
    }
}
