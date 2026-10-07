//! Switching the IDE window between mini-ork projects.
//!
//! Known projects are the homes listed in `~/.config/mini-ork/projects.json`
//! (the registry `mini-ork serve` and Setup › Projects share;
//! `MINI_ORK_PROJECTS_FILE` overrides the path). Switching opens the project's
//! folder as a workspace of this window and activates it, so each project keeps
//! its own Threads board and tabs; a project opened once is remembered.

use std::path::{Path, PathBuf};

use gpui::{App, Entity, PathPromptOptions, TaskExt as _, Window};
use serde_json::{Value, json};
use workspace::{MultiWorkspace, OpenMode, OpenOptions, Workspace};

fn registry_file() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("MINI_ORK_PROJECTS_FILE").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(path));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/mini-ork/projects.json"))
}

fn read_registry(file: &Path) -> Value {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({ "projects": [] }))
}

/// Project folders (the parents of each `.mini-ork` home), registry order.
pub fn known() -> Vec<PathBuf> {
    let Some(file) = registry_file() else {
        return Vec::new();
    };
    let registry = read_registry(&file);
    let mut roots: Vec<PathBuf> = Vec::new();
    for home in registry.get("projects").and_then(Value::as_array).into_iter().flatten() {
        let Some(home) = home.as_str() else { continue };
        let home = PathBuf::from(home);
        let root = match home.file_name() {
            Some(name) if name == ".mini-ork" => home.parent().map(Path::to_path_buf).unwrap_or(home),
            _ => home,
        };
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots
}

/// Add `home` (a `.mini-ork` directory) to the registry, once.
pub fn remember_home(home: &Path) {
    let Some(file) = registry_file() else { return };
    let mut registry = read_registry(&file);
    let entry = home.to_string_lossy().to_string();
    let Some(list) = registry
        .as_object_mut()
        .map(|object| object.entry("projects").or_insert_with(|| json!([])))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    if list.iter().any(|known| known.as_str() == Some(entry.as_str())) {
        return;
    }
    list.push(Value::String(entry));
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    if let Ok(text) = serde_json::to_string_pretty(&registry) {
        std::fs::write(&file, text + "\n").ok();
    }
}

/// The folder of this workspace's first visible worktree.
pub fn current_root(workspace: &Entity<Workspace>, cx: &App) -> Option<PathBuf> {
    let project = workspace.read(cx).project().read(cx);
    let worktree = project.visible_worktrees(cx).next()?;
    Some(worktree.read(cx).abs_path().to_path_buf())
}

/// Switch this window to the project at `root`: open its folder as a
/// workspace of this window (or activate it when it is already open).
pub fn switch_to(workspace: &Entity<Workspace>, root: PathBuf, window: &mut Window, cx: &mut App) {
    if current_root(workspace, cx).as_deref() == Some(root.as_path()) {
        return;
    }
    let home = root.join(".mini-ork");
    if home.is_dir() {
        remember_home(&home);
    }
    let app_state = workspace.read(cx).app_state().clone();
    let options = OpenOptions {
        requesting_window: window.window_handle().downcast::<MultiWorkspace>(),
        open_mode: OpenMode::Activate,
        ..OpenOptions::default()
    };
    workspace::open_paths(&[root], app_state, options, cx).detach_and_log_err(cx);
}

/// Ask for a project folder, then switch this window to it.
pub fn pick(workspace: &Entity<Workspace>, window: &mut Window, cx: &mut App) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some("Open mini-ork project".into()),
    });
    let workspace = workspace.downgrade();
    window
        .spawn(cx, async move |cx| {
            let Ok(Ok(Some(paths))) = paths.await else { return };
            let Some(root) = paths.into_iter().next() else { return };
            cx.update(|window, cx| {
                if let Some(workspace) = workspace.upgrade() {
                    switch_to(&workspace, root, window, cx);
                }
            })
            .ok();
        })
        .detach();
}
