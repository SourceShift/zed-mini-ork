// `dev-inspector` exists because `debug_assertions` is off in every optimized
// profile, including `release-fast` — the one the shipped IDE is built with.
// Gating on the feature instead of the assertion lets an optimized build carry
// the inspector without turning on the ~100 other debug assertions in gpui.
#[cfg(any(debug_assertions, feature = "dev-inspector"))]
mod div_inspector;
#[cfg(any(debug_assertions, feature = "dev-inspector"))]
mod inspector;

#[cfg(any(debug_assertions, feature = "dev-inspector"))]
pub use inspector::init;

#[cfg(not(any(debug_assertions, feature = "dev-inspector")))]
pub fn init(_app_state: std::sync::Arc<workspace::AppState>, cx: &mut gpui::App) {
    use std::any::TypeId;
    use workspace::notifications::NotifyResultExt as _;

    cx.on_action(|_: &zed_actions::dev::ToggleInspector, cx| {
        Err::<(), anyhow::Error>(anyhow::anyhow!(
            "dev::ToggleInspector is only available in debug builds"
        ))
        .notify_app_err(cx);
    });

    command_palette_hooks::CommandPaletteFilter::update_global(cx, |filter, _cx| {
        filter.hide_action_types(&[TypeId::of::<zed_actions::dev::ToggleInspector>()]);
    });
}
