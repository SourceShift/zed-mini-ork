use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use settings_macros::{MergeFrom, with_fallible_options};

#[with_fallible_options]
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize, JsonSchema, MergeFrom)]
pub struct MiniOrkSettingsContent {
    /// Which segments the mini-ork status bar item draws.
    pub status_bar: Option<MiniOrkStatusBarContent>,
}

/// One switch per mini-ork status bar segment. `null` (or a missing key) leaves
/// that segment at its default, which is shown.
#[with_fallible_options]
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize, JsonSchema, MergeFrom)]
pub struct MiniOrkStatusBarContent {
    /// Show the count of runs and gates waiting on you.
    ///
    /// Default: true
    pub needs_you: Option<bool>,
    /// Show the count of runs in flight.
    ///
    /// Default: true
    pub working: Option<bool>,
    /// Show the count of failed runs.
    ///
    /// Default: true
    pub failed: Option<bool>,
    /// Show today's spend against the daily cap.
    ///
    /// Default: true
    pub spend: Option<bool>,
    /// Show whether the project scheduler is on.
    ///
    /// Default: true
    pub scheduler: Option<bool>,
    /// Show the alert naming lanes that are failing half their calls.
    ///
    /// Default: true
    pub lanes: Option<bool>,
    /// Show the mini-ork version.
    ///
    /// Default: true
    pub version: Option<bool>,
    /// Show the active file's ContextNest session count.
    ///
    /// Default: true
    pub file_sessions: Option<bool>,
}
