//! Fensterrahmen: Schnellzugriff, Registerkarten, Ribbon, Navigation, Statusleiste.

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, RichText, Sense, Stroke, vec2,
};
use egui_phosphor::regular as ic;
use gradecraft_core::{Period, ScaleKind};

use crate::config::AppConfig;
use crate::dialogs::Dialog;
use crate::session::{BackstagePage, CourseTab, RibbonTab, Session, SessionEvent, View};
use crate::theme::{self, Palette, ThemeChoice};
use crate::widgets::{big_button, big_toggle, group, small_button, stack};

impl Session {
    /// Obere Leiste: Speichern, Rückgängig, Dateiname, Suche.
    pub fn title_bar(&mut self, ui: &mut egui::Ui, cfg: &mut AppConfig) {
        let p = Palette::get(ui.ctx());
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            ui.set_height(34.0);
            ui.add_space(6.0);
            ui.label(
                RichText::new(ic::GRADUATION_CAP)
                    .size(20.0)
                    .color(p.accent_text),
            );
            ui.label(
                RichText::new("GradeCraft")
                    .font(theme::bold(14.0))
                    .color(p.accent_text),
            );
            ui.add_space(12.0);
            ui.label(
                RichText::new("Automatisch speichern")
                    .size(12.5)
                    .color(p.text_weak),
            );
            if toggle(ui, &mut cfg.autosave).changed() {
                cfg.save();
            }
            ui.add_space(6.0);
            if icon_button(ui, ic::FLOPPY_DISK, "Speichern (Strg+S)", true).clicked()
                && self.save(&ctx)
            {
                self.notify(&ctx, "Gespeichert", false);
            }
            if icon_button(
                ui,
                ic::ARROW_U_UP_LEFT,
                "Rückgängig (Strg+Z)",
                !self.undo.is_empty(),
            )
            .clicked()
            {
                self.undo(&ctx);
            }
            if icon_button(
                ui,
                ic::ARROW_U_UP_RIGHT,
                "Wiederholen (Strg+Y)",
                !self.redo.is_empty(),
            )
            .clicked()
            {
                self.redo(&ctx);
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(8.0);
                let lock = egui::Button::new(
                    RichText::new(format!("{}  Sperren", ic::LOCK))
                        .color(Color32::WHITE)
                        .strong(),
                )
                .fill(theme::ACCENT)
                .corner_radius(CornerRadius::same(12));
                if ui
                    .add(lock)
                    .on_hover_text("Datei sperren und Passwort verlangen (Strg+L)")
                    .clicked()
                {
                    self.event = SessionEvent::Lock;
                }
                ui.add_space(8.0);
                self.search_box(ui, cfg);
                // Titel mittig im verbleibenden Platz
                let name = self
                    .vault
                    .path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Notendatei")
                    .to_string();
                let state = if self.dirty {
                    "Nicht gespeichert"
                } else {
                    "Gespeichert"
                };
                ui.with_layout(
                    Layout::centered_and_justified(egui::Direction::LeftToRight),
                    |ui| {
                        ui.label(
                            RichText::new(format!("{name} — {state}")).font(theme::bold(13.5)),
                        );
                    },
                );
            });
        });
    }

    fn search_box(&mut self, ui: &mut egui::Ui, cfg: &AppConfig) {
        let p = Palette::get(ui.ctx());
        let resp = ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text(format!(
                    "{}  Schüler, Klassen, Fächer suchen",
                    ic::MAGNIFYING_GLASS
                ))
                .desired_width(280.0),
        );
        let query = self.search.trim().to_lowercase();
        // Popup bleibt offen, solange das Suchfeld fokussiert ist oder die Maus darüber ist
        let popup_rect_id = egui::Id::new("search-popup-rect");
        let last_rect: Option<egui::Rect> = ui.memory(|m| m.data.get_temp(popup_rect_id));
        let over_popup = last_rect
            .is_some_and(|r| ui.input(|i| i.pointer.interact_pos().is_some_and(|p| r.contains(p))));
        if query.is_empty() || !(resp.has_focus() || over_popup) {
            ui.memory_mut(|m| m.data.remove::<egui::Rect>(popup_rect_id));
            return;
        }
        // Treffer sammeln
        let mut hits: Vec<(String, String, View)> = Vec::new();
        for class in &self.db.classes {
            let year = self
                .db
                .year(class.year_id)
                .map(|y| y.name.clone())
                .unwrap_or_default();
            if class.name.to_lowercase().contains(&query) {
                hits.push((
                    format!("{}  Klasse {}", ic::CHALKBOARD_TEACHER, class.name),
                    year.clone(),
                    View::Class(class.id),
                ));
            }
            for course in self.db.courses_of_class(class.id) {
                if course.subject.to_lowercase().contains(&query)
                    || course.short.to_lowercase() == query
                {
                    hits.push((
                        format!("{}  {} – {}", ic::BOOK_OPEN, class.name, course.title()),
                        year.clone(),
                        View::Course(course.id),
                    ));
                }
            }
            for s in &class.students {
                let name = self.student_name(s, cfg.first_name_first);
                if name.to_lowercase().contains(&query)
                    || s.first_name.to_lowercase().contains(&query)
                {
                    hits.push((
                        format!("{}  {}", ic::STUDENT, name),
                        format!("{} · {}", class.name, year),
                        View::Class(class.id),
                    ));
                }
            }
        }
        hits.truncate(12);
        let mut go = None;
        let area = egui::Area::new(egui::Id::new("search-popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(resp.rect.left_bottom() + vec2(0.0, 4.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(resp.rect.width());
                    if hits.is_empty() {
                        ui.label(RichText::new("Keine Treffer").color(p.text_weak));
                    }
                    for (label, sub, view) in &hits {
                        let r = ui.add(
                            egui::Button::new(RichText::new(label))
                                .frame(false)
                                .min_size(vec2(resp.rect.width() - 12.0, 0.0)),
                        );
                        ui.label(RichText::new(sub).size(11.0).color(p.text_weak));
                        if r.clicked() {
                            go = Some(*view);
                        }
                    }
                });
            });
        ui.memory_mut(|m| m.data.insert_temp(popup_rect_id, area.response.rect));
        if let Some(v) = go {
            self.open_view(v);
            self.search.clear();
        }
    }

    /// Registerkarten-Leiste (Datei, Start, …).
    pub fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let p = Palette::get(ui.ctx());
        ui.horizontal(|ui| {
            ui.set_height(30.0);
            ui.add_space(8.0);
            // „Datei“ öffnet die Backstage-Ansicht
            if tab_label(ui, "Datei", self.backstage.is_some(), true).clicked() {
                self.backstage = if self.backstage.is_some() {
                    None
                } else {
                    Some(BackstagePage::Info)
                };
            }
            for tab in RibbonTab::ALL {
                if tab_label(
                    ui,
                    tab.label(),
                    self.backstage.is_none() && self.tab == tab,
                    false,
                )
                .clicked()
                {
                    self.tab = tab;
                    self.backstage = None;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(10.0);
                if self.readonly() {
                    ui.label(
                        RichText::new(format!("{} Archiviert – schreibgeschützt", ic::ARCHIVE))
                            .color(p.warn),
                    );
                }
            });
        });
    }

    /// Ribbon mit Gruppen je nach Registerkarte.
    pub fn ribbon(&mut self, ui: &mut egui::Ui, cfg: &mut AppConfig) {
        let ctx = ui.ctx().clone();
        let ro = self.readonly() || self.year_archived();
        let class = self.current_class();
        let course = self.current_course();
        egui::ScrollArea::horizontal().id_salt("ribbon-scroll").show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.add_space(4.0);
                match self.tab {
                    RibbonTab::Start => {
                        group(ui, "Neu", |ui| {
                            if big_button(ui, ic::CHALKBOARD_TEACHER, "Klasse", !self.year_archived()).clicked() {
                                self.dialog = Some(Dialog::NewClass { name: String::new() });
                            }
                            if big_button(ui, ic::BOOK_OPEN, "Fach", class.is_some() && !ro).clicked()
                                && let Some(c) = class {
                                    self.dialog = Some(Dialog::new_course(&self.db, c));
                                }
                            if big_button(ui, ic::USER_PLUS, "Schüler", class.is_some() && !ro).clicked()
                                && let Some(c) = class {
                                    self.add_student(&ctx, c);
                                }
                        });
                        group(ui, "Halbjahr", |ui| {
                            let on = course.is_some();
                            for (tab, icon, label) in [
                                (CourseTab::H1, ic::NUMBER_CIRCLE_ONE, "1. Halbjahr"),
                                (CourseTab::H2, ic::NUMBER_CIRCLE_TWO, "2. Halbjahr"),
                                (CourseTab::Schema, ic::SCALES, "Bewertungs-\nschema"),
                            ] {
                                if on {
                                    if big_toggle(ui, icon, label, self.course_tab == tab).clicked() {
                                        self.course_tab = tab;
                                        self.grid = Default::default();
                                    }
                                } else {
                                    big_button(ui, icon, label, false);
                                }
                            }
                        });
                        group(ui, "Zeugnisnoten", |ui| {
                            let period = if self.course_tab == CourseTab::H2 { Period::Schuljahr } else { Period::Halbjahr };
                            let on = course.is_some() && self.course_tab != CourseTab::Schema && !ro;
                            if big_button(ui, ic::SEAL_CHECK, "Vorschläge\nbestätigen", on).on_hover_text(format!("Alle offenen Vorschläge für die {} übernehmen", period.label())).clicked()
                                && let Some(c) = course {
                                    self.confirm_all(&ctx, c, period);
                                }
                            stack(ui, |ui| {
                                let sel_student = self.selected_student();
                                if small_button(ui, ic::HAND_HEART, "Pädagogische Note …", on && sel_student.is_some()).on_hover_text("Zuerst eine Zeile in der Tabelle auswählen").clicked()
                                    && let (Some(c), Some(s)) = (course, sel_student) {
                                        self.dialog = Some(Dialog::final_grade(&self.db, c, s, period));
                                    }
                                if small_button(ui, ic::FILE_CSV, "Notenliste exportieren", course.is_some()).clicked()
                                    && let Some(c) = course {
                                        self.export_course_csv(&ctx, c);
                                    }
                            });
                        });
                        group(ui, "Datei", |ui| {
                            if big_button(ui, ic::FLOPPY_DISK, "Speichern", true).clicked() && self.save(&ctx) {
                                self.notify(&ctx, "Gespeichert", false);
                            }
                            if big_button(ui, ic::LOCK, "Sperren", true).clicked() {
                                self.event = SessionEvent::Lock;
                            }
                        });
                    }
                    RibbonTab::Klasse => {
                        group(ui, "Klasse", |ui| {
                            if big_button(ui, ic::CHALKBOARD_TEACHER, "Neue\nKlasse", !self.year_archived()).clicked() {
                                self.dialog = Some(Dialog::NewClass { name: String::new() });
                            }
                            stack(ui, |ui| {
                                if small_button(ui, ic::PENCIL_SIMPLE, "Umbenennen", class.is_some() && !ro).clicked()
                                    && let Some(c) = class.and_then(|c| self.db.class(c)) {
                                        self.dialog = Some(Dialog::RenameClass { id: c.id, name: c.name.clone() });
                                    }
                                if small_button(ui, ic::TRASH, "Klasse löschen", class.is_some() && !ro).clicked()
                                    && let Some(c) = class {
                                        self.dialog = Some(Dialog::confirm_delete_class(&self.db, c));
                                    }
                            });
                        });
                        group(ui, "Schüler", |ui| {
                            if big_button(ui, ic::USER_PLUS, "Schüler\nhinzufügen", class.is_some() && !ro).clicked()
                                && let Some(c) = class {
                                    self.add_student(&ctx, c);
                                }
                            if big_button(ui, ic::CLIPBOARD_TEXT, "Liste\nimportieren", class.is_some() && !ro).on_hover_text("Aus CSV-Datei oder Zwischenablage (z. B. aus SchILD, Excel)").clicked()
                                && let Some(c) = class {
                                    self.dialog = Some(Dialog::Import { class: c, text: String::new() });
                                }
                            stack(ui, |ui| {
                                if small_button(ui, ic::SORT_ASCENDING, "Alphabetisch sortieren", class.is_some() && !ro).clicked()
                                    && let Some(c) = class {
                                        self.sort_students(&ctx, c);
                                    }
                            });
                        });
                        group(ui, "Fächer", |ui| {
                            if big_button(ui, ic::BOOK_OPEN, "Neues\nFach", class.is_some() && !ro).clicked()
                                && let Some(c) = class {
                                    self.dialog = Some(Dialog::new_course(&self.db, c));
                                }
                            stack(ui, |ui| {
                                if small_button(ui, ic::PENCIL_SIMPLE, "Fach bearbeiten", course.is_some() && !ro).clicked()
                                    && let Some(c) = course.and_then(|c| self.db.course(c)) {
                                        self.dialog = Some(Dialog::EditCourse { id: c.id, subject: c.subject.clone(), short: c.short.clone() });
                                    }
                                if small_button(ui, ic::TRASH, "Fach löschen", course.is_some() && !ro).clicked()
                                    && let Some(c) = course {
                                        self.dialog = Some(Dialog::confirm_delete_course(&self.db, c));
                                    }
                            });
                        });
                    }
                    RibbonTab::Bewertung => {
                        let on = course.is_some() && !ro;
                        group(ui, "Leistungsbereiche", |ui| {
                            if big_button(ui, ic::SCALES, "Schema\nbearbeiten", course.is_some()).clicked() {
                                self.course_tab = CourseTab::Schema;
                            }
                            if big_button(ui, ic::STACK, "Bereich\nhinzufügen", on).clicked()
                                && let Some(c) = course {
                                    self.add_area(&ctx, c);
                                    self.course_tab = CourseTab::Schema;
                                }
                        });
                        group(ui, "Leistung", |ui| {
                            if big_button(ui, ic::LIST_PLUS, "Spalte\n1. Halbjahr", on).clicked()
                                && let Some(c) = course {
                                    self.add_column_dialog(c, gradecraft_core::Half::H1);
                                }
                            if big_button(ui, ic::LIST_PLUS, "Spalte\n2. Halbjahr", on).clicked()
                                && let Some(c) = course {
                                    self.add_column_dialog(c, gradecraft_core::Half::H2);
                                }
                        });
                        group(ui, "Vorlagen", |ui| {
                            if big_button(ui, ic::BOOKMARK_SIMPLE, "Als Vorlage\nspeichern", course.is_some()).clicked()
                                && let Some(c) = course.and_then(|c| self.db.course(c)) {
                                    self.dialog = Some(Dialog::SaveTemplate { course: c.id, name: format!("{} – {}", c.subject, self.db.class(c.class_id).map(|k| k.name.as_str()).unwrap_or("")) });
                                }
                            if big_button(ui, ic::COPY_SIMPLE, "Vorlage\nanwenden", on && !self.db.templates.is_empty()).clicked()
                                && let Some(c) = course {
                                    self.dialog = Some(Dialog::ApplyTemplate { course: c, template: self.db.templates.first().map(|t| t.id) });
                                }
                        });
                        group(ui, "Notenskala", |ui| {
                            let scale = course.and_then(|c| self.db.course(c)).map(|c| c.scale);
                            let has_grades = course.and_then(|c| self.db.course(c)).is_some_and(|c| !c.grades.is_empty() || !c.finals.is_empty());
                            for s in ScaleKind::ALL {
                                let icon = if s == ScaleKind::Noten { ic::NUMBER_SQUARE_SIX } else { ic::HASH };
                                let label = if s == ScaleKind::Noten { "Noten\n1–6" } else { "Punkte\n0–15" };
                                let r = if on && !has_grades {
                                    big_toggle(ui, icon, label, scale == Some(s))
                                } else {
                                    let r = big_button(ui, icon, label, false);
                                    if has_grades {
                                        r.on_disabled_hover_text("Die Skala kann nur geändert werden, solange noch keine Noten eingetragen sind.")
                                    } else {
                                        r
                                    }
                                };
                                if r.clicked()
                                    && let Some(c) = course {
                                        self.checkpoint();
                                        if let Some(c) = self.db.course_mut(c) {
                                            c.scale = s;
                                        }
                                        self.touch(&ctx);
                                    }
                            }
                        });
                    }
                    RibbonTab::Berichte => {
                        use crate::reports::PdfKind;
                        let sel_student = self.selected_student();
                        let sel_column = self.selected_column();
                        group(ui, "Fach", |ui| {
                            if big_button(ui, ic::FILE_PDF, "Notenliste", course.is_some())
                                .on_hover_text("Komplette Notentabelle beider Halbjahre mit Zeugnisnoten")
                                .clicked()
                                && let Some(c) = course {
                                    self.export_pdf(&ctx, PdfKind::CourseList(c));
                                }
                            if big_button(ui, ic::CHART_BAR, "Noten-\nspiegel", course.is_some())
                                .on_hover_text("Notenspiegel aller Leistungen mit Noten")
                                .clicked()
                                && let Some(c) = course {
                                    self.export_pdf(&ctx, PdfKind::Distribution(c, None));
                                }
                            stack(ui, |ui| {
                                if small_button(ui, ic::CHART_BAR, "Nur ausgewählte Leistung", course.is_some() && sel_column.is_some())
                                    .on_hover_text("Zuerst eine Zelle der gewünschten Leistung auswählen")
                                    .clicked()
                                    && let Some(c) = course {
                                        self.export_pdf(&ctx, PdfKind::Distribution(c, sel_column));
                                    }
                                if small_button(ui, ic::FILE_CSV, "Notenliste als CSV", course.is_some()).clicked()
                                    && let Some(c) = course {
                                        self.export_course_csv(&ctx, c);
                                    }
                            });
                        });
                        group(ui, "Klasse", |ui| {
                            if big_button(ui, ic::USERS_THREE, "Konferenz\nHalbjahr", class.is_some()).clicked()
                                && let Some(c) = class {
                                    self.export_pdf(&ctx, PdfKind::Conference(c, Period::Halbjahr));
                                }
                            if big_button(ui, ic::USERS_THREE, "Konferenz\nSchuljahr", class.is_some()).clicked()
                                && let Some(c) = class {
                                    self.export_pdf(&ctx, PdfKind::Conference(c, Period::Schuljahr));
                                }
                        });
                        group(ui, "Schülerbogen", |ui| {
                            if big_button(ui, ic::IDENTIFICATION_CARD, "Ausgewählter\nSchüler", class.is_some() && sel_student.is_some())
                                .on_hover_text("Zuerst eine Zeile in der Notentabelle auswählen")
                                .clicked()
                                && let Some(c) = class {
                                    self.export_pdf(&ctx, PdfKind::Sheets { class: c, student: sel_student, course: None });
                                }
                            if big_button(ui, ic::FILES, "Ganze\nKlasse", class.is_some())
                                .on_hover_text("Ein Schülerbogen pro Schüler, alle Fächer")
                                .clicked()
                                && let Some(c) = class {
                                    self.export_pdf(&ctx, PdfKind::Sheets { class: c, student: None, course: None });
                                }
                        });
                        group(ui, "Einstellungen", |ui| {
                            if big_button(ui, ic::GEAR, "Kopfzeile\n& Logo", true).clicked() {
                                self.backstage = Some(BackstagePage::Settings);
                            }
                        });
                    }
                    RibbonTab::Ansicht => {
                        group(ui, "Design", |ui| {
                            for (choice, icon) in [(ThemeChoice::Hell, ic::SUN), (ThemeChoice::Dunkel, ic::MOON), (ThemeChoice::System, ic::CIRCLE_HALF)] {
                                if big_toggle(ui, icon, choice.label(), cfg.theme == choice).clicked() {
                                    cfg.theme = choice;
                                    theme::apply(&ctx, choice);
                                    cfg.save();
                                }
                            }
                        });
                        group(ui, "Zoom", |ui| {
                            if big_button(ui, ic::MAGNIFYING_GLASS_PLUS, "Vergrößern", cfg.zoom < 2.0).clicked() {
                                cfg.zoom = (cfg.zoom + 0.1).min(2.0);
                                cfg.save();
                            }
                            if big_button(ui, ic::MAGNIFYING_GLASS_MINUS, "Verkleinern", cfg.zoom > 0.7).clicked() {
                                cfg.zoom = (cfg.zoom - 0.1).max(0.7);
                                cfg.save();
                            }
                            if big_button(ui, ic::ARROWS_OUT_SIMPLE, "100 %", true).clicked() {
                                cfg.zoom = 1.0;
                                cfg.save();
                            }
                        });
                        group(ui, "Anzeigen", |ui| {
                            if big_toggle(ui, ic::SIDEBAR, "Navigation", cfg.show_nav).clicked() {
                                cfg.show_nav = !cfg.show_nav;
                                cfg.save();
                            }
                            if big_toggle(ui, ic::CHART_BAR, "Klassen-\nstatistik", cfg.show_stats).clicked() {
                                cfg.show_stats = !cfg.show_stats;
                                cfg.save();
                            }
                            if big_toggle(ui, ic::TEXT_AA, "Vorname\nzuerst", cfg.first_name_first).clicked() {
                                cfg.first_name_first = !cfg.first_name_first;
                                cfg.save();
                            }
                        });
                    }
                    RibbonTab::Hilfe => {
                        group(ui, "Hilfe", |ui| {
                            if big_button(ui, ic::KEYBOARD, "Noten-\neingabe", true).clicked() {
                                self.dialog = Some(Dialog::InputHelp);
                            }
                            if big_button(ui, ic::SHIELD_CHECK, "Datenschutz", true).clicked() {
                                self.dialog = Some(Dialog::Privacy);
                            }
                            if big_button(ui, ic::INFO, "Über\nGradeCraft", true).clicked() {
                                self.dialog = Some(Dialog::About);
                            }
                        });
                        group(ui, "Aktualisierung", |ui| {
                            if big_button(ui, ic::ARROW_CIRCLE_UP, "Nach Updates\nsuchen", true).clicked() {
                                self.request_update_check = true;
                            }
                        });
                    }
                }
            });
        });
    }

    /// Navigationsbereich links: Schuljahr, Klassen, Fächer.
    pub fn navigation(&mut self, ui: &mut egui::Ui) {
        let p = Palette::get(ui.ctx());
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Navigation").font(theme::bold(17.0)));
        });
        ui.add_space(6.0);
        let year_name = self
            .db
            .year(self.year)
            .map(|y| y.name.clone())
            .unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(RichText::new(ic::CALENDAR_BLANK).color(p.accent_text));
            egui::ComboBox::from_id_salt("year")
                .selected_text(format!("Schuljahr {year_name}"))
                .width(ui.available_width() - 4.0)
                .show_ui(ui, |ui| {
                    for y in &self.db.school_years {
                        let label = if y.archived {
                            format!("{} (archiviert)", y.name)
                        } else {
                            y.name.clone()
                        };
                        if ui.selectable_label(self.year == y.id, label).clicked() {
                            self.year = y.id;
                            self.view = View::Welcome;
                        }
                    }
                    ui.separator();
                    if ui
                        .button(format!("{} Schuljahre verwalten …", ic::GEAR))
                        .clicked()
                    {
                        self.backstage = Some(BackstagePage::Years);
                    }
                });
        });
        ui.add_space(8.0);
        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let classes: Vec<(u64, String)> = self
                    .db
                    .classes_of_year(self.year)
                    .map(|c| (c.id, c.name.clone()))
                    .collect();
                if classes.is_empty() {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Noch keine Klassen in diesem Schuljahr.").color(p.text_weak),
                    );
                }
                for (cid, cname) in classes {
                    let selected = self.view == View::Class(cid);
                    let n = self.db.class(cid).map(|c| c.students.len()).unwrap_or(0);
                    let r = nav_item(
                        ui,
                        ic::CHALKBOARD_TEACHER,
                        &cname,
                        &format!("{n}"),
                        selected,
                        0.0,
                        true,
                    );
                    if r.clicked() {
                        self.open_view(View::Class(cid));
                    }
                    let ro = self.is_readonly_class(cid);
                    r.context_menu(|ui| {
                        if ui
                            .add_enabled(
                                !ro,
                                egui::Button::new(format!("{} Fach hinzufügen", ic::BOOK_OPEN)),
                            )
                            .clicked()
                        {
                            self.dialog = Some(Dialog::new_course(&self.db, cid));
                        }
                        if ui
                            .add_enabled(
                                !ro,
                                egui::Button::new(format!("{} Umbenennen", ic::PENCIL_SIMPLE)),
                            )
                            .clicked()
                        {
                            self.dialog = Some(Dialog::RenameClass {
                                id: cid,
                                name: cname.clone(),
                            });
                        }
                        if ui
                            .add_enabled(!ro, egui::Button::new(format!("{} Löschen", ic::TRASH)))
                            .clicked()
                        {
                            self.dialog = Some(Dialog::confirm_delete_class(&self.db, cid));
                        }
                    });
                    let courses: Vec<(u64, String)> = self
                        .db
                        .courses_of_class(cid)
                        .map(|c| (c.id, c.title()))
                        .collect();
                    for (id, title) in courses {
                        let r = nav_item(
                            ui,
                            ic::BOOK_OPEN,
                            &title,
                            "",
                            self.view == View::Course(id),
                            18.0,
                            false,
                        );
                        if r.clicked() {
                            self.open_view(View::Course(id));
                        }
                        r.context_menu(|ui| {
                            if ui
                                .add_enabled(
                                    !ro,
                                    egui::Button::new(format!("{} Bearbeiten", ic::PENCIL_SIMPLE)),
                                )
                                .clicked()
                                && let Some(c) = self.db.course(id)
                            {
                                self.dialog = Some(Dialog::EditCourse {
                                    id,
                                    subject: c.subject.clone(),
                                    short: c.short.clone(),
                                });
                            }
                            if ui
                                .add_enabled(
                                    !ro,
                                    egui::Button::new(format!("{} Löschen", ic::TRASH)),
                                )
                                .clicked()
                            {
                                self.dialog = Some(Dialog::confirm_delete_course(&self.db, id));
                            }
                        });
                    }
                    ui.add_space(2.0);
                }
                ui.add_space(8.0);
                if !self.year_archived()
                    && ui
                        .add(
                            egui::Button::new(
                                RichText::new(format!("{}  Neue Klasse", ic::PLUS))
                                    .color(p.accent_text),
                            )
                            .frame(false),
                        )
                        .clicked()
                {
                    self.dialog = Some(Dialog::NewClass {
                        name: String::new(),
                    });
                }
            });
    }

    /// Statusleiste unten.
    pub fn status_bar(&mut self, ui: &mut egui::Ui, cfg: &mut AppConfig) {
        let p = Palette::get(ui.ctx());
        let now = ui.input(|i| i.time);
        ui.horizontal(|ui| {
            ui.set_height(24.0);
            ui.add_space(8.0);
            let small = |t: String| RichText::new(t).size(12.0).color(p.text_weak);
            match self.view {
                View::Class(c) | View::Course(c) if self.current_class().is_some() => {
                    let class = self.current_class().and_then(|c| self.db.class(c));
                    if let Some(class) = class {
                        ui.label(small(format!("Klasse {}", class.name)));
                        ui.label(small(format!("{} Schüler", class.students.len())));
                    }
                    if let View::Course(_) = self.view
                        && let Some(course) = self.db.course(c)
                    {
                        ui.label(small(format!(
                            "{} · {}",
                            course.title(),
                            course.scale.label()
                        )));
                    }
                }
                _ => {
                    ui.label(small(format!(
                        "{} Klassen",
                        self.db.classes_of_year(self.year).count()
                    )));
                }
            }
            ui.label(small(format!(
                "{} Verschlüsselt (AES-256)",
                ic::SHIELD_CHECK
            )));
            if let Some((text, t, err)) = &self.status
                && now - t < 5.0
            {
                ui.label(RichText::new(text).size(12.0).color(if *err {
                    p.bad
                } else {
                    p.accent_text
                }));
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(500));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(10.0);
                ui.label(small(format!("{:.0} %", cfg.zoom * 100.0)));
                if ui.small_button("+").clicked() {
                    cfg.zoom = (cfg.zoom + 0.1).min(2.0);
                    cfg.save();
                }
                let mut z = cfg.zoom;
                if ui
                    .add(egui::Slider::new(&mut z, 0.7..=2.0).show_value(false))
                    .drag_stopped()
                {
                    cfg.zoom = z;
                    cfg.save();
                } else {
                    cfg.zoom = z;
                }
                if ui.small_button("−").clicked() {
                    cfg.zoom = (cfg.zoom - 0.1).max(0.7);
                    cfg.save();
                }
            });
        });
    }

    /// ID der Leistung (Spalte) der ausgewählten Tabellenzelle.
    pub fn selected_column(&self) -> Option<u64> {
        let (_, ecol) = self.grid.sel?;
        let course = self.db.course(self.current_course()?)?;
        let half = match self.course_tab {
            CourseTab::H1 => gradecraft_core::Half::H1,
            CourseTab::H2 => gradecraft_core::Half::H2,
            CourseTab::Schema => return None,
        };
        course
            .scheme
            .areas
            .iter()
            .flat_map(|a| a.columns.iter().filter(|c| c.half == half))
            .nth(ecol)
            .map(|c| c.id)
    }

    /// ID des Schülers in der ausgewählten Tabellenzeile.
    pub fn selected_student(&self) -> Option<u64> {
        let (row, _) = self.grid.sel?;
        let course = self.db.course(self.current_course()?)?;
        self.db
            .class(course.class_id)?
            .students
            .get(row)
            .map(|s| s.id)
    }
}

/// Kleine Icon-Schaltfläche im Schnellzugriff.
pub fn icon_button(ui: &mut egui::Ui, icon: &str, tip: &str, enabled: bool) -> egui::Response {
    let p = Palette::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(
        vec2(26.0, 26.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if enabled && resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), p.accent_soft);
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(17.0),
        if enabled {
            p.text
        } else {
            p.text_weak.gamma_multiply(0.5)
        },
    );
    resp.on_hover_text(tip)
}

/// Ein-/Aus-Schalter.
pub fn toggle(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let size = vec2(34.0, 18.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let p = Palette::get(ui.ctx());
    let t = ui.ctx().animate_bool(resp.id, *on);
    let bg = if *on { theme::ACCENT } else { p.sheet };
    ui.painter().rect(
        rect,
        CornerRadius::same(9),
        bg,
        Stroke::new(1.0, if *on { theme::ACCENT } else { p.text_weak }),
        egui::StrokeKind::Inside,
    );
    let x = egui::lerp(rect.left() + 9.0..=rect.right() - 9.0, t);
    ui.painter().circle_filled(
        egui::pos2(x, rect.center().y),
        5.5,
        if *on { Color32::WHITE } else { p.text_weak },
    );
    resp
}

fn tab_label(ui: &mut egui::Ui, text: &str, selected: bool, file: bool) -> egui::Response {
    let p = Palette::get(ui.ctx());
    let font = if selected {
        theme::bold(14.0)
    } else {
        FontId::proportional(14.0)
    };
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, p.text);
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 22.0, 30.0), Sense::click());
    if resp.hovered() && !selected {
        ui.painter()
            .rect_filled(rect.shrink(2.0), CornerRadius::same(4), p.accent_soft);
    }
    let color = if selected || file {
        p.accent_text
    } else {
        p.text
    };
    ui.painter().galley_with_override_text_color(
        rect.center() - galley.size() / 2.0,
        galley,
        color,
    );
    if selected {
        ui.painter().rect_filled(
            egui::Rect::from_center_size(
                egui::pos2(rect.center().x, rect.bottom() - 2.0),
                vec2(rect.width() - 18.0, 3.0),
            ),
            CornerRadius::same(1),
            p.accent_text,
        );
    }
    resp
}

fn nav_item(
    ui: &mut egui::Ui,
    icon: &str,
    text: &str,
    badge: &str,
    selected: bool,
    indent: f32,
    strong: bool,
) -> egui::Response {
    let p = Palette::get(ui.ctx());
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 28.0), Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), p.accent_soft);
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.min + vec2(0.0, 6.0), vec2(3.0, rect.height() - 12.0)),
            CornerRadius::same(1),
            p.accent_text,
        );
    } else if resp.hovered() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(4),
            p.accent_soft.gamma_multiply(0.6),
        );
    }
    let x = rect.left() + 10.0 + indent;
    ui.painter().text(
        egui::pos2(x + 8.0, rect.center().y),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(16.0),
        p.accent_text,
    );
    let font = if strong {
        theme::bold(14.0)
    } else {
        FontId::proportional(14.0)
    };
    ui.painter().text(
        egui::pos2(x + 22.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        font,
        p.text,
    );
    if !badge.is_empty() {
        ui.painter().text(
            rect.right_center() - vec2(10.0, 0.0),
            Align2::RIGHT_CENTER,
            badge,
            FontId::proportional(12.0),
            p.text_weak,
        );
    }
    resp
}
