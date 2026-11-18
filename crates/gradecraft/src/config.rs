//! Programmeinstellungen (unverschlüsselt, enthalten KEINE Schülerdaten –
//! nur Dateipfade und Darstellungsoptionen).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::theme::ThemeChoice;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub recent_files: Vec<PathBuf>,
    pub theme: ThemeChoice,
    pub zoom: f32,
    pub show_nav: bool,
    pub autosave: bool,
    pub first_name_first: bool,
    pub show_stats: bool,
    pub check_updates: bool,
    /// Noten verdecken, wenn GradeCraft nicht das aktive Fenster ist
    pub privacy_on_blur: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            recent_files: Vec::new(),
            theme: ThemeChoice::System,
            zoom: 1.0,
            show_nav: true,
            autosave: true,
            first_name_first: false,
            show_stats: true,
            check_updates: true,
            privacy_on_blur: true,
        }
    }
}

/// Portabler Modus: Liegt eine Datei `portable` (oder `portable.txt`) neben
/// der .exe, werden Einstellungen und Daten im Programmordner gespeichert –
/// z. B. auf einem USB-Stick.
pub fn portable_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.to_path_buf();
    (dir.join("portable").exists() || dir.join("portable.txt").exists()).then_some(dir)
}

fn project_dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("de", "GradeCraft", "GradeCraft")
}

/// Standardordner für Notendateien.
pub fn default_data_dir() -> PathBuf {
    if let Some(p) = portable_dir() {
        return p.join("Daten");
    }
    if let Some(u) = directories::UserDirs::new()
        && let Some(docs) = u.document_dir()
    {
        return docs.join("GradeCraft");
    }
    project_dirs()
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn config_path() -> Option<PathBuf> {
    if let Some(p) = portable_dir() {
        return Some(p.join("gradecraft-einstellungen.json"));
    }
    project_dirs().map(|d| d.config_dir().join("einstellungen.json"))
}

impl AppConfig {
    pub fn load() -> Self {
        config_path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(p) = config_path() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(b) = serde_json::to_vec_pretty(self) {
                let _ = std::fs::write(p, b);
            }
        }
    }

    pub fn add_recent(&mut self, path: &Path) {
        let path = path.to_path_buf();
        self.recent_files.retain(|p| p != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(8);
        self.save();
    }
}
