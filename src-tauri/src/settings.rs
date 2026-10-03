use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

/// User preferences, persisted as JSON in the app config dir. OS-owned state
/// (e.g. the login item, owned by System Settings) does not live here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Show each app's memory usage under its name in the panel.
    pub show_memory_usage: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_memory_usage: true,
        }
    }
}

impl Settings {
    fn path(app: &AppHandle) -> Option<PathBuf> {
        app.path()
            .app_config_dir()
            .ok()
            .map(|dir| dir.join("settings.json"))
    }

    fn load(app: &AppHandle) -> Self {
        let Some(path) = Self::path(app) else {
            return Self::default();
        };
        match fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn save(&self, app: &AppHandle) -> Result<(), String> {
        let Some(path) = Self::path(app) else {
            return Err("no app config dir".into());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, json).map_err(|e| e.to_string())
    }
}

pub struct SettingsState(Mutex<Settings>);

// The command fns must be `pub`: `generate_handler!` (in lib.rs) needs the
// macros `#[tauri::command]` generates, and their visibility follows the fn's.
#[tauri::command]
pub fn get_settings(state: State<'_, SettingsState>) -> Settings {
    state.0.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_show_memory_usage(
    app: AppHandle,
    state: State<'_, SettingsState>,
    show: bool,
) -> Settings {
    let settings = {
        let mut guard = state.0.lock().unwrap();
        guard.show_memory_usage = show;
        guard.clone()
    };
    if let Err(err) = settings.save(&app) {
        log::warn!("failed to save settings: {err}");
    }
    settings
}

/// Load (or seed defaults for) the persisted settings and manage them as app
/// state. Must run before the settings window can use the IPC commands.
pub fn init(app: &AppHandle) {
    app.manage(SettingsState(Mutex::new(Settings::load(app))));
}
