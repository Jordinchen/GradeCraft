//! Update-Prüfung über GitHub Releases.
//!
//! Es wird ausschließlich eine anonyme Anfrage an die GitHub-API gestellt
//! (keine Schüler- oder Nutzungsdaten). Unter Windows kann das neue
//! Setup direkt heruntergeladen und gestartet werden; es aktualisiert die
//! vorhandene Installation (gleiche AppId im Inno-Setup-Skript).

use std::sync::mpsc::{Receiver, channel};

use serde::Deserialize;

/// GitHub-Repository, dessen Releases geprüft werden (muss öffentlich sein
/// oder ein separates öffentliches Release-Repository).
pub const UPDATE_REPO: &str = "jordinchen/gradecraft";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
    pub page_url: String,
    pub setup_url: Option<String>,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

pub enum UpdateState {
    Idle,
    Checking(Receiver<Result<Option<UpdateInfo>, String>>),
    Available(UpdateInfo),
    UpToDate,
    Failed(String),
    Downloading(Receiver<Result<std::path::PathBuf, String>>),
}

/// Vergleicht Versionen der Form „1.2.3“ (optional mit führendem „v“).
pub fn is_newer(remote: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.trim_start_matches(['v', 'V'])
            .split(['.', '-'])
            .take(3)
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    parse(remote) > parse(current)
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .build()
        .into()
}

fn fetch_latest() -> Result<Option<UpdateInfo>, String> {
    let url = format!("https://api.github.com/repos/{UPDATE_REPO}/releases/latest");
    let release: Release = agent()
        .get(&url)
        .header("User-Agent", "GradeCraft-Updater")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Update-Server nicht erreichbar ({e})"))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("Ungültige Antwort ({e})"))?;
    if release.draft || release.prerelease || !is_newer(&release.tag_name, CURRENT_VERSION) {
        return Ok(None);
    }
    let setup_url = release
        .assets
        .iter()
        .find(|a| a.name.to_lowercase().ends_with("-setup.exe"))
        .map(|a| a.browser_download_url.clone());
    Ok(Some(UpdateInfo {
        version: release.tag_name.trim_start_matches('v').to_string(),
        notes: release.body,
        page_url: release.html_url,
        setup_url,
    }))
}

pub fn check_async(ctx: eframe::egui::Context) -> UpdateState {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(fetch_latest());
        ctx.request_repaint();
    });
    UpdateState::Checking(rx)
}

pub fn download_async(ctx: eframe::egui::Context, url: String) -> UpdateState {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let result = (|| {
            let mut resp = agent()
                .get(&url)
                .header("User-Agent", "GradeCraft-Updater")
                .call()
                .map_err(|e| format!("Download fehlgeschlagen ({e})"))?;
            let bytes = resp
                .body_mut()
                .with_config()
                .limit(200 * 1024 * 1024)
                .read_to_vec()
                .map_err(|e| format!("Download fehlgeschlagen ({e})"))?;
            let path = std::env::temp_dir().join("GradeCraft-Update-Setup.exe");
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
            Ok(path)
        })();
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    UpdateState::Downloading(rx)
}

/// Startet das heruntergeladene Setup. Die App muss sich danach beenden.
pub fn run_installer(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new(path)
            .args([
                "/SILENT",
                "/SP-",
                "/CLOSEAPPLICATIONS",
                "/RESTARTAPPLICATIONS",
            ])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Automatische Installation ist nur unter Windows verfügbar.".into())
    }
}

impl UpdateState {
    /// Fragt laufende Hintergrundaufgaben ab.
    pub fn poll(&mut self) -> Option<std::path::PathBuf> {
        match self {
            UpdateState::Checking(rx) => {
                if let Ok(r) = rx.try_recv() {
                    *self = match r {
                        Ok(Some(info)) => UpdateState::Available(info),
                        Ok(None) => UpdateState::UpToDate,
                        Err(e) => UpdateState::Failed(e),
                    };
                }
            }
            UpdateState::Downloading(rx) => {
                if let Ok(r) = rx.try_recv() {
                    match r {
                        Ok(path) => {
                            *self = UpdateState::Idle;
                            return Some(path);
                        }
                        Err(e) => *self = UpdateState::Failed(e),
                    }
                }
            }
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn versionen() {
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(is_newer("0.1.10", "0.1.9"));
    }
}
