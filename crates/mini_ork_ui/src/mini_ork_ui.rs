//! The mini-ork IDE shell, drawn to `mini-ork IDE.dc.html`.
//!
//! Everything mini-ork-specific in the IDE lives in this crate. Zed's own
//! sources are touched only by the small hook commit that registers the crate
//! (workspace `Cargo.toml`, `crates/zed/Cargo.toml`), calls [`init`] and puts
//! [`MiniOrkSidebar`] in the workspace sidebar slot — so a Zed upgrade is a
//! rebase of that hook commit, and API drift stays inside this crate.
//!
//! - `sidebar`: activity bar + Threads board (Zed's pluggable sidebar slot)
//! - `title_bar`: replaces Zed's title bar item
//! - `status`: status bar items
//! - `page`: every activity page and run tab, rendered from
//!   `mini-ork board page <key>` JSON
//! - `store`: one `mini-ork board` poller per workspace + page actions
//!
//! mini-ork stays the source of truth: the shell is a view over its CLI.

mod board;
mod graph;
mod loading;
mod page;
mod palette;
mod sidebar;
mod status;
mod store;
mod title_bar;

pub use sidebar::MiniOrkSidebar;

use gpui::{App, BorrowAppContext as _};
use settings::{MergeFromTrait as _, SettingsContent, SettingsStore};

/// The IDE's own defaults, layered under the user's settings (which stay
/// shared with stock Zed and are never written): the design's status bar has
/// no dock buttons, and the activity bar replaces them.
const IDE_DEFAULTS: &str = r#"{
  "search": { "button": false },
  "project_panel": { "button": false },
  "outline_panel": { "button": false },
  "collaboration_panel": { "button": false },
  "git_panel": { "button": false },
  "agent": { "button": false, "dock": "left" },
  "diagnostics": { "button": false },
  "terminal": { "button": false },
  "global_lsp_settings": { "button": false },
  "debugger": { "button": false }
}"#;

pub fn init(cx: &mut App) {
    store::init(cx);
    title_bar::init(cx);
    status::init(cx);
    apply_ide_defaults(cx);
}

fn apply_ide_defaults(cx: &mut App) {
    let overlay = match serde_json::from_str::<SettingsContent>(IDE_DEFAULTS) {
        Ok(overlay) => overlay,
        Err(error) => {
            log::error!("mini-ork IDE defaults: {error}");
            return;
        }
    };
    if cx.has_global::<SettingsStore>() {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_default_settings(cx, |defaults| defaults.merge_from(&overlay));
        });
    }
}
