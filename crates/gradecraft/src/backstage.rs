//! „Datei“-Ansicht (Backstage) wie in Office.

use eframe::egui::{self, Color32, CornerRadius, RichText, Sense, vec2};
use egui_phosphor::regular as ic;
use gradecraft_core::store::FILE_EXTENSION;

use crate::config::AppConfig;
use crate::dialogs::{ConfirmAction, Dialog};
use crate::session::{BackstagePage, Session, SessionEvent};
use crate::theme::{self, Palette};
use crate::widgets::{card, primary_button, secondary_button, section_title};

impl Session {
    pub fn backstage(&mut self, ui: &mut egui::Ui, cfg: &mut AppConfig) {
        let p = Palette::get(ui.ctx());
        let page = self.backstage.unwrap_or(BackstagePage::Info);
        egui::Panel::left("backstage-menu")
            .exact_size(220.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::ACCENT)
                    .inner_margin(egui::Margin::symmetric(0, 12)),
            )
            .show(ui, |ui| {
                if menu_entry(ui, ic::ARROW_LEFT, "Zurück", false).clicked() {
                    self.backstage = None;
                }
                ui.add_space(10.0);
                for (pg, icon, label) in [
                    (BackstagePage::Info, ic::INFO, "Informationen"),
                    (BackstagePage::SaveAs, ic::FLOPPY_DISK, "Speichern unter"),
                    (
                        BackstagePage::Backup,
                        ic::CLOCK_COUNTER_CLOCKWISE,
                        "Sicherungen",
                    ),
                    (BackstagePage::Export, ic::EXPORT, "Exportieren"),
                    (BackstagePage::Years, ic::CALENDAR_BLANK, "Schuljahre"),
                    (BackstagePage::Templates, ic::BOOKMARK_SIMPLE, "Vorlagen"),
                    (BackstagePage::Settings, ic::GEAR, "Optionen"),
                ] {
                    if menu_entry(ui, icon, label, page == pg).clicked() {
                        self.backstage = Some(pg);
                    }
                }
                ui.add_space(16.0);
                if menu_entry(ui, ic::LOCK, "Sperren", false).clicked() {
                    self.event = SessionEvent::Lock;
                }
                if menu_entry(ui, ic::SIGN_OUT, "Datei schließen", false).clicked() {
                    self.event = SessionEvent::Close;
                }
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(p.sheet)
                    .inner_margin(egui::Margin::same(32)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_max_width(760.0);
                        match page {
                            BackstagePage::Info => self.bs_info(ui),
                            BackstagePage::SaveAs => self.bs_save_as(ui, cfg),
                            BackstagePage::Backup => self.bs_backup(ui),
                            BackstagePage::Export => self.bs_export(ui),
                            BackstagePage::Years => self.bs_years(ui),
                            BackstagePage::Templates => self.bs_templates(ui),
                            BackstagePage::Settings => self.bs_settings(ui, cfg),
                        }
                    });
            });
    }

    fn bs_info(&mut self, ui: &mut egui::Ui) {
        let p = Palette::get(ui.ctx());
        heading(ui, "Informationen");
        let path = self.vault.path().to_path_buf();
        card(ui, |ui| {
            egui::Grid::new("info").num_columns(2).spacing([20.0, 8.0]).show(ui, |ui| {
                ui.label(RichText::new("Datei").strong());
                ui.label(path.display().to_string());
                ui.end_row();
                ui.label(RichText::new("Größe").strong());
                ui.label(std::fs::metadata(&path).map(|m| format!("{:.1} KB", m.len() as f64 / 1024.0)).unwrap_or_default());
                ui.end_row();
                ui.label(RichText::new("Schutz").strong());
                ui.label(RichText::new(format!("{} AES-256-GCM, Argon2id-Passwortableitung", ic::SHIELD_CHECK)).color(p.good));
                ui.end_row();
                ui.label(RichText::new("Inhalt").strong());
                let students: usize = self.db.classes.iter().map(|c| c.students.len()).sum();
                let grades: usize = self.db.courses.iter().map(|c| c.grades.len()).sum();
                ui.label(format!(
                    "{} Schuljahre · {} Klassen · {} Fächer · {students} Schülereinträge · {grades} Noten",
                    self.db.school_years.len(),
                    self.db.classes.len(),
                    self.db.courses.len()
                ));
                ui.end_row();
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if secondary_button(ui, &format!("{} Ordner öffnen", ic::FOLDER_OPEN)).clicked()
                    && let Some(dir) = path.parent()
                {
                    open_in_explorer(dir);
                }
                if secondary_button(ui, &format!("{} Passwort ändern …", ic::KEY)).clicked() {
                    self.dialog = Some(Dialog::ChangePassword {
                        old: String::new(),
                        new1: String::new(),
                        new2: String::new(),
                        error: None,
                    });
                }
            });
        });
    }

    fn bs_save_as(&mut self, ui: &mut egui::Ui, cfg: &mut AppConfig) {
        let ctx = ui.ctx().clone();
        heading(ui, "Speichern unter");
        card(ui, |ui| {
            ui.label("Speichert die Notendatei (verschlüsselt, gleiches Passwort) an einem neuen Ort – z. B. auf einem dienstlichen USB-Stick. Anschließend wird mit der neuen Datei weitergearbeitet.");
            ui.add_space(8.0);
            if primary_button(ui, &format!("{}  Ort wählen …", ic::FOLDER_OPEN)).clicked() {
                let name = self
                    .vault
                    .path()
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or("Noten.gcdb".into());
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(name)
                    .add_filter("GradeCraft-Datei", &[FILE_EXTENSION])
                    .save_file()
                {
                    let path = if path.extension().is_none() {
                        path.with_extension(FILE_EXTENSION)
                    } else {
                        path
                    };
                    match self.vault.save_as(&path, &self.db) {
                        Ok(()) => {
                            self.dirty = false;
                            cfg.add_recent(&path);
                            self.notify(
                                &ctx,
                                &format!("Gespeichert unter {}", path.display()),
                                false,
                            );
                        }
                        Err(e) => self.notify(&ctx, &e.to_string(), true),
                    }
                }
            }
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            section_title(ui, "Verschlüsselte Kopie");
            ui.label("Legt eine Kopie an, ohne den aktuellen Speicherort zu ändern (z. B. zur Weitergabe an ein eigenes zweites Gerät).");
            ui.add_space(6.0);
            if secondary_button(ui, &format!("{} Kopie speichern …", ic::COPY_SIMPLE)).clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .set_file_name("Noten-Kopie.gcdb")
                    .add_filter("GradeCraft-Datei", &[FILE_EXTENSION])
                    .save_file()
            {
                match self.vault.export_copy(&path, &self.db) {
                    Ok(()) => self.notify(&ctx, "Kopie gespeichert", false),
                    Err(e) => self.notify(&ctx, &e.to_string(), true),
                }
            }
        });
    }

    fn bs_backup(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let p = Palette::get(&ctx);
        heading(ui, "Sicherungen");
        let dir = self.vault.backup_dir();
        card(ui, |ui| {
            ui.label(format!(
                "Beim Öffnen der Datei wird automatisch eine verschlüsselte Sicherung angelegt. Es werden die letzten {} Sicherungen aufbewahrt.",
                self.db.settings.backup_count
            ));
            ui.label(
                RichText::new(dir.display().to_string())
                    .color(p.text_weak)
                    .size(12.0),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if primary_button(
                    ui,
                    &format!("{}  Jetzt sichern", ic::CLOCK_COUNTER_CLOCKWISE),
                )
                .clicked()
                {
                    self.save(&ctx);
                    match self.vault.backup(self.db.settings.backup_count.max(1)) {
                        Ok(Some(_)) => self.notify(&ctx, "Sicherung erstellt", false),
                        Ok(None) => {}
                        Err(e) => self.notify(&ctx, &e.to_string(), true),
                    }
                }
                if secondary_button(ui, &format!("{} Ordner öffnen", ic::FOLDER_OPEN)).clicked() {
                    let _ = std::fs::create_dir_all(&dir);
                    open_in_explorer(&dir);
                }
            });
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            section_title(ui, "Vorhandene Sicherungen");
            let mut files: Vec<_> = std::fs::read_dir(&dir)
                .map(|r| {
                    r.filter_map(|e| e.ok())
                        .map(|e| e.path())
                        .filter(|p| p.extension().is_some_and(|e| e == FILE_EXTENSION))
                        .collect()
                })
                .unwrap_or_default();
            files.sort();
            files.reverse();
            if files.is_empty() {
                ui.label(RichText::new("Noch keine Sicherungen.").color(p.text_weak));
            }
            for f in files {
                ui.label(format!(
                    "{}  {}",
                    ic::FILE_LOCK,
                    f.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Wiederherstellen: Datei schließen und die gewünschte Sicherung über „Andere Datei öffnen“ laden (gleiches Passwort wie zum Zeitpunkt der Sicherung).").size(12.0).color(p.text_weak));
        });
    }

    fn bs_export(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let p = Palette::get(&ctx);
        heading(ui, "Exportieren");
        card(ui, |ui| {
            ui.label(RichText::new(format!("{} Exporte sind nicht verschlüsselt. Bitte nur auf dienstlich zugelassenen Geräten speichern.", ic::WARNING)).color(p.warn));
            ui.add_space(8.0);
            section_title(ui, "Notenlisten als CSV (Excel)");
            let courses: Vec<(u64, String)> = self
                .db
                .classes_of_year(self.year)
                .flat_map(|c| {
                    self.db
                        .courses_of_class(c.id)
                        .map(move |k| (k.id, format!("{} – {}", c.name, k.title())))
                })
                .collect();
            if courses.is_empty() {
                ui.label(RichText::new("Keine Fächer im gewählten Schuljahr.").color(p.text_weak));
            }
            for (id, label) in courses {
                ui.horizontal(|ui| {
                    ui.label(format!("{} {label}", ic::BOOK_OPEN));
                    if ui.button(format!("{} CSV", ic::FILE_CSV)).clicked() {
                        self.export_course_csv(&ctx, id);
                    }
                });
            }
            ui.add_space(8.0);
            ui.label(RichText::new("PDF-Notenlisten und Konferenzübersicht folgen in einer der nächsten Versionen.").size(12.0).color(p.text_weak));
        });
    }

    fn bs_years(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let p = Palette::get(&ctx);
        heading(ui, "Schuljahre");
        card(ui, |ui| {
            ui.label("Archivierte Schuljahre sind schreibgeschützt. Beim Anlegen eines neuen Schuljahres können Klassen samt Schülern übernommen werden (z. B. 9b → 10b).");
            ui.add_space(8.0);
            if primary_button(ui, &format!("{}  Neues Schuljahr …", ic::CALENDAR_PLUS)).clicked()
            {
                let next = next_year_name(
                    self.db
                        .year(self.year)
                        .map(|y| y.name.as_str())
                        .unwrap_or(""),
                );
                let carry = self
                    .db
                    .classes_of_year(self.year)
                    .map(|c| (c.id, true, bump_class_name(&c.name)))
                    .collect();
                self.dialog = Some(Dialog::NewYear {
                    name: next,
                    carry,
                    with_courses: true,
                });
            }
        });
        ui.add_space(12.0);
        let years: Vec<(u64, String, bool, usize)> = self
            .db
            .school_years
            .iter()
            .map(|y| {
                (
                    y.id,
                    y.name.clone(),
                    y.archived,
                    self.db.classes_of_year(y.id).count(),
                )
            })
            .collect();
        let single = years.len() == 1;
        for (id, name, archived, n) in years {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{} {name}", ic::CALENDAR_BLANK)).font(theme::bold(16.0)));
                    ui.label(RichText::new(format!("{n} Klassen")).color(p.text_weak));
                    if archived {
                        ui.label(RichText::new(format!("{} archiviert", ic::ARCHIVE)).color(p.warn));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(!single, egui::Button::new(format!("{} Löschen", ic::TRASH))).clicked() {
                            self.dialog = Some(Dialog::Confirm {
                                title: "Schuljahr löschen".into(),
                                text: format!("Schuljahr {name} mit {n} Klassen und allen Noten endgültig löschen?"),
                                action: ConfirmAction::DeleteYear(id),
                            });
                        }
                        let label = if archived { format!("{} Archivierung aufheben", ic::ARCHIVE) } else { format!("{} Archivieren", ic::ARCHIVE) };
                        if ui.button(label).clicked() {
                            self.checkpoint();
                            if let Some(y) = self.db.school_years.iter_mut().find(|y| y.id == id) {
                                y.archived = !archived;
                            }
                            self.touch(&ctx);
                        }
                        if ui.button(format!("{} Öffnen", ic::ARROW_RIGHT)).clicked() {
                            self.year = id;
                            self.view = crate::session::View::Welcome;
                            self.backstage = None;
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
    }

    fn bs_templates(&mut self, ui: &mut egui::Ui) {
        let p = Palette::get(ui.ctx());
        heading(ui, "Vorlagen");
        ui.label("Vorlagen speichern Leistungsbereiche, Spalten und Gewichtungen. Neue Vorlagen legst du im Register „Bewertung“ an.");
        ui.add_space(8.0);
        let templates: Vec<(u64, String, String)> = self
            .db
            .templates
            .iter()
            .map(|t| {
                let desc = t
                    .scheme
                    .areas
                    .iter()
                    .map(|a| {
                        format!(
                            "{} ×{} ({} Spalten)",
                            a.name,
                            crate::widgets::fmt_weight(a.weight),
                            a.columns.len()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" · ");
                (
                    t.id,
                    t.name.clone(),
                    format!("{} · {desc}", t.scale.label()),
                )
            })
            .collect();
        if templates.is_empty() {
            ui.label(RichText::new("Keine Vorlagen vorhanden.").color(p.text_weak));
        }
        for (id, name, desc) in templates {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(format!("{} {name}", ic::BOOKMARK_SIMPLE)).strong());
                        ui.label(RichText::new(desc).size(12.0).color(p.text_weak));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(format!("{} Löschen", ic::TRASH)).clicked() {
                            self.dialog = Some(Dialog::Confirm {
                                title: "Vorlage löschen".into(),
                                text: format!("Vorlage „{name}“ löschen? Bestehende Fächer bleiben unverändert."),
                                action: ConfirmAction::DeleteTemplate(id),
                            });
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
    }

    fn bs_settings(&mut self, ui: &mut egui::Ui, cfg: &mut AppConfig) {
        let ctx = ui.ctx().clone();
        let p = Palette::get(&ctx);
        heading(ui, "Optionen");
        let mut s = self.db.settings.clone();
        card(ui, |ui| {
            section_title(ui, "Sicherheit");
            ui.horizontal(|ui| {
                ui.label("Automatisch sperren nach");
                ui.add(
                    egui::DragValue::new(&mut s.auto_lock_minutes)
                        .range(0..=240)
                        .suffix(" min"),
                );
                ui.label(RichText::new("(0 = nie)").color(p.text_weak));
            });
            ui.add_space(6.0);
            section_title(ui, "Sicherungen");
            ui.horizontal(|ui| {
                ui.label("Anzahl aufbewahrter Sicherungen");
                ui.add(egui::DragValue::new(&mut s.backup_count).range(0..=100));
            });
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            section_title(ui, "Speicherorte");
            ui.label(format!(
                "Standardordner für neue Dateien: {}",
                crate::config::default_data_dir().display()
            ));
            match crate::config::portable_dir() {
                Some(d) => ui.label(RichText::new(format!("{} Portabler Modus aktiv ({})", ic::USB, d.display())).color(p.good)),
                None => ui.label(
                    RichText::new("Portabler Modus: Lege eine leere Datei namens „portable“ neben GradeCraft.exe, dann werden Einstellungen und Daten im Programmordner gespeichert (z. B. USB-Stick).")
                        .size(12.0)
                        .color(p.text_weak),
                ),
            };
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            section_title(ui, "Aktualisierungen");
            if ui
                .checkbox(
                    &mut cfg.check_updates,
                    "Beim Start automatisch nach neuen Versionen suchen",
                )
                .changed()
            {
                cfg.save();
            }
            ui.label(
                RichText::new(format!(
                    "Installierte Version: {}. Geprüft wird nur, ob auf GitHub ({}) eine neuere Version vorliegt – es werden keine Daten übertragen.",
                    crate::updater::CURRENT_VERSION,
                    crate::updater::UPDATE_REPO
                ))
                .size(12.0)
                .color(p.text_weak),
            );
            if ui
                .button(format!("{} Jetzt suchen", ic::ARROW_CIRCLE_UP))
                .clicked()
            {
                self.request_update_check = true;
            }
        });
        if s.auto_lock_minutes != self.db.settings.auto_lock_minutes
            || s.backup_count != self.db.settings.backup_count
        {
            self.db.settings = s;
            self.touch(&ctx);
        }
    }
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).font(theme::bold(30.0)));
    ui.add_space(16.0);
}

fn menu_entry(ui: &mut egui::Ui, icon: &str, label: &str, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    let bg = if selected {
        Color32::from_white_alpha(46)
    } else if resp.hovered() {
        Color32::from_white_alpha(24)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::ZERO, bg);
    ui.painter().text(
        rect.left_center() + vec2(26.0, 0.0),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(18.0),
        Color32::WHITE,
    );
    ui.painter().text(
        rect.left_center() + vec2(46.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        theme::bold(14.5),
        Color32::WHITE,
    );
    resp
}

pub fn open_in_explorer(dir: &std::path::Path) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(dir).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(dir).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
}

/// „2025/26“ → „2026/27“
pub fn next_year_name(name: &str) -> String {
    if let Some((a, _)) = name.split_once('/')
        && let Ok(y) = a.trim().parse::<i32>()
    {
        return format!("{}/{:02}", y + 1, (y + 2) % 100);
    }
    gradecraft_core::model::current_school_year_name()
}

/// „9b“ → „10b“, „IFK126“ bleibt.
pub fn bump_class_name(name: &str) -> String {
    let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || digits.len() > 2 {
        return name.to_string();
    }
    let n: u32 = digits.parse().unwrap_or(0);
    format!("{}{}", n + 1, &name[digits.len()..])
}
