//! mini-ork panels for the mini-ork IDE.
//!
//! Everything mini-ork-specific in the IDE lives in this crate. Zed's own
//! sources are touched only by the small hook commit that registers the crate
//! (workspace `Cargo.toml`, `crates/zed/Cargo.toml`), calls [`init`] and loads
//! [`MiniOrkPanel`] in `initialize_panels` — so a Zed upgrade is a rebase of
//! that hook commit, and API drift stays inside this crate.
//!
//! The panels are a view: the data and every action come from the
//! `mini-ork board` CLI (see `board.rs`), so mini-ork stays the source of truth.

mod board;
mod panel;

pub use panel::MiniOrkPanel;

pub fn init(cx: &mut gpui::App) {
    panel::init(cx);
}
