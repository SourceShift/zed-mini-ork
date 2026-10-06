//! The `mini-ork board` contract: what the panels read and the commands they run.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, anyhow};
use serde::Deserialize;
use serde_json::Value;

/// What `mini-ork board --json` returns that the shell reads; the rest of
/// the document (learnings, automations, …) is for pages and is ignored here.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Board {
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub header: Header,
    #[serde(default)]
    pub runs: Vec<Run>,
    #[serde(default)]
    pub counts: Counts,
}

/// The title bar and status bar numbers (`board --json` → `header`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Header {
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub worktrees: u64,
    #[serde(default)]
    pub needs_you: u64,
    #[serde(default)]
    pub working: u64,
    #[serde(default)]
    pub failed: u64,
    /// `None` when the project has no state.db yet.
    #[serde(default)]
    pub today_usd: Option<f64>,
    #[serde(default)]
    pub cap_usd: f64,
    #[serde(default)]
    pub scheduler_on: bool,
    #[serde(default)]
    pub contextnest: bool,
    #[serde(default)]
    pub nodes: Nodes,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Nodes {
    #[serde(default)]
    pub configured: u64,
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
    pub ended_at: Option<i64>,
    #[serde(default)]
    pub cost_usd: f64,
    #[serde(default)]
    pub added: u64,
    #[serde(default)]
    pub removed: u64,
}

impl Run {
    pub fn display_title(&self) -> String {
        if self.title.trim().is_empty() {
            self.id.clone()
        } else {
            self.title.trim().to_string()
        }
    }
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

/// One node's detail view (`mini-ork board node <run> <node> --view V`):
/// stream, output, prompt, telemetry or learning. `offset` asks the stream
/// view for the entries after the ones already shown.
pub async fn fetch_node(
    bin: PathBuf,
    home: PathBuf,
    run: String,
    node: String,
    view: String,
    offset: Option<u64>,
) -> Result<Value> {
    let mut argv = vec!["board".to_string(), "node".into(), run, node, "--view".into(), view];
    if let Some(offset) = offset {
        argv.push("--offset".into());
        argv.push(offset.to_string());
    }
    let value = run_json(&bin, &argv, &home).await?;
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(anyhow!(
            "{}",
            value.get("error").and_then(Value::as_str).unwrap_or("the node could not be read")
        ));
    }
    Ok(value)
}

/// One IDE page (`mini-ork board page <key>`), as JSON for the page renderer.
pub async fn fetch_page(
    bin: PathBuf,
    home: PathBuf,
    key: String,
    tab: Option<String>,
    args: BTreeMap<String, String>,
) -> Result<Value> {
    let mut argv = vec!["board".to_string(), "page".to_string(), key];
    if let Some(tab) = tab {
        argv.push("--tab".into());
        argv.push(tab);
    }
    for (k, v) in args {
        argv.push("--arg".into());
        argv.push(format!("{k}={v}"));
    }
    let value = run_json(&bin, &argv, &home).await?;
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(anyhow!(
            "{}",
            value.get("error").and_then(Value::as_str).unwrap_or("the page failed")
        ));
    }
    Ok(value)
}

/// The shell's board read: `board --json --shell` (header, runs and counts
/// only — cheap enough to poll), or the full `board --json` from a mini-ork
/// that predates `--shell`.
pub async fn fetch_board(bin: PathBuf, home: PathBuf) -> Result<Board> {
    let value = match run_json(&bin, &["board".into(), "--json".into(), "--shell".into()], &home).await {
        Ok(value) => value,
        Err(_) => run_json(&bin, &["board".into(), "--json".into()], &home).await?,
    };
    serde_json::from_value(value).context("reading mini-ork board")
}

/// Run a mini-ork action (`board merge <id>`, `automations run <id>`, …) and
/// return its one-line outcome. Commands that answer in JSON report through
/// `ok`/`error`; the rest succeed on exit 0 with their last line of output.
/// `with_home: false` is for subcommands without a `--home` flag: they find
/// the project through `MINI_ORK_HOME` and the working directory instead.
pub async fn run_action(bin: PathBuf, home: PathBuf, args: Vec<String>, with_home: bool) -> Result<String> {
    let mut command = util::command::new_command(&bin);
    command
        .args(&args)
        .env("MINI_ORK_HOME", &home)
        .env_remove("MINI_ORK_VENV_ACTIVE");
    if with_home {
        command.arg("--home").arg(&home);
    }
    if let Some(project) = home.parent() {
        command.current_dir(project);
    }
    let output = command
        .output()
        .await
        .with_context(|| format!("running {}", bin.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Ok(value) = serde_json::from_str::<Value>(stdout.trim()) {
        if value.get("ok").and_then(Value::as_bool) == Some(false) {
            return Err(anyhow!(
                "{}",
                value.get("error").and_then(Value::as_str).unwrap_or("failed")
            ));
        }
        return Ok(match value.get("mode").and_then(Value::as_str) {
            Some(mode) => format!("Done ({mode})."),
            None => "Done.".to_string(),
        });
    }
    let last = |text: &str| {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .last()
            .unwrap_or_default()
            .to_string()
    };
    if output.status.success() {
        let line = last(&stdout);
        Ok(if line.is_empty() { "Done.".to_string() } else { line })
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!("{}", last(&stderr).max(last(&stdout))))
    }
}
