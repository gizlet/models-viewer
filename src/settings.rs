//! What the app remembers between runs, as JSON in `GIZLET_SAVE_DIR`
//! (`./save` when it is not set): the render style, auto-rotate, and the tab
//! and model last looked at.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub style: String,
    pub auto_rotate: bool,
    pub category: Option<String>,
    pub model: Option<String>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { style: "studio".into(), auto_rotate: true, category: None, model: None }
    }
}

pub struct Store {
    path: Option<PathBuf>,
}

impl Store {
    /// `enabled` is false for captures and tests, which leave saves alone.
    pub fn from_env(enabled: bool) -> Store {
        let dir = std::env::var_os("GIZLET_SAVE_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("save"));
        Store { path: enabled.then(|| dir.join("settings.json")) }
    }

    pub fn load(&self) -> Settings {
        let Some(path) = &self.path else { return Settings::default() };
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, s: &Settings) {
        let Some(path) = &self.path else { return };
        let write = || -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = path.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec_pretty(s).unwrap_or_default())?;
            std::fs::rename(&tmp, path)
        };
        if let Err(e) = write() {
            eprintln!("modelsviewer: cannot save settings to {}: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_tolerate_missing_fields() {
        let dir = std::env::temp_dir().join(format!("modelsviewer-test-{}", std::process::id()));
        let store = Store { path: Some(dir.join("settings.json")) };
        assert_eq!(store.load(), Settings::default());
        let s = Settings {
            style: "clay".into(),
            auto_rotate: false,
            category: Some("food".into()),
            model: Some("khronos-fox".into()),
        };
        store.save(&s);
        assert_eq!(store.load(), s);
        std::fs::write(dir.join("settings.json"), r#"{"style":"wireframe"}"#).unwrap();
        let partial = store.load();
        assert_eq!(partial.style, "wireframe");
        assert!(partial.auto_rotate);
        let _ = std::fs::remove_dir_all(dir);
    }
}
