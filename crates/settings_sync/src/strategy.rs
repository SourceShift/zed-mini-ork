use std::sync::Arc;

use anyhow::{Context as _, Result};
use serde_json::Value;
use settings_json::parse_json_with_comments;

use crate::classifier::DocumentClassifier;
use crate::merge::{ExclusionSet, PathMap, apply_target_to_text, flatten_doc, unflatten};
use crate::settings_schema_epoch;

pub trait SyncStrategy: Send + Sync {
    fn flatten(&self, doc: &Value) -> PathMap;
    fn unflatten(&self, paths: &PathMap) -> Value;
    fn apply(&self, text: &str, current: &PathMap, target: &PathMap) -> Result<String>;
    fn migrate(&self, text: &str) -> Result<Option<String>>;
    fn built_in_exclusions(&self) -> ExclusionSet;
    fn schema_epoch(&self) -> u64;

    fn migrate_doc(&self, doc: &Value) -> Result<Value> {
        let text = serde_json::to_string_pretty(doc).context("serializing a synced document")?;
        match self.migrate(&text)? {
            Some(migrated) => parse_json_with_comments::<Value>(&migrated)
                .context("parsing a migrated synced document"),
            None => Ok(doc.clone()),
        }
    }
}

pub struct SettingsJsonStrategy {
    classifier: DocumentClassifier,
}

impl SettingsJsonStrategy {
    pub fn user_settings() -> Arc<dyn SyncStrategy> {
        Arc::new(Self {
            classifier: DocumentClassifier::for_user_settings(),
        })
    }
}

impl SyncStrategy for SettingsJsonStrategy {
    fn flatten(&self, doc: &Value) -> PathMap {
        flatten_doc(&self.classifier, doc)
    }

    fn unflatten(&self, paths: &PathMap) -> Value {
        unflatten(paths)
    }

    fn apply(&self, text: &str, current: &PathMap, target: &PathMap) -> Result<String> {
        apply_target_to_text(&self.classifier, text, current, target)
    }

    fn migrate(&self, text: &str) -> Result<Option<String>> {
        migrator::migrate_settings(text)
    }

    fn built_in_exclusions(&self) -> ExclusionSet {
        ExclusionSet::built_in()
    }

    fn schema_epoch(&self) -> u64 {
        settings_schema_epoch()
    }
}
