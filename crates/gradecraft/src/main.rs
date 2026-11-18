// Unter Windows kein Konsolenfenster öffnen (nur im Release-Build).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod backstage;
mod chrome;
mod config;
mod dialogs;
mod grid;
mod oral;
mod reports;
mod session;
mod theme;
mod updater;
mod views;
mod widgets;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GradeCraft")
            .with_app_id("gradecraft")
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([900.0, 560.0])
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "GradeCraft",
        options,
        Box::new(|cc| Ok(Box::new(app::GradeCraftApp::new(cc)))),
    )
}

/// Programm-Icon (lila Kachel mit Doktorhut) für Fenster und Taskleiste.
fn app_icon() -> egui::IconData {
    let png = include_bytes!("../assets/icon-256.png");
    eframe::icon_data::from_png_bytes(png).unwrap_or_default()
}

#[cfg(test)]
mod ui_tests;
