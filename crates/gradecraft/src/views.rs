//! Startansicht, Klassenansicht und Bewertungsschema-Editor.

use eframe::egui::{self, RichText};
use egui_phosphor::regular as ic;
use gradecraft_core::{Area, Gender, Half, Id, Student, YearMode};

use crate::config::AppConfig;
use crate::dialogs::Dialog;
use crate::session::{Session, View};
use crate::theme::{self, Palette};
use crate::widgets::{card, fmt_weight, primary_button, section_title, weight_edit};

impl Session {
    pub fn welcome_view(&mut self, ui: &mut egui::Ui) {
        let p = Palette::get(ui.ctx());
        let year = self
            .db
            .year(self.year)
            .map(|y| y.name.clone())
            .unwrap_or_default();
        ui.label(RichText::new(format!("Schuljahr {year}")).font(theme::bold(24.0)));
        ui.label(
            RichText::new("Wähle links eine Klasse oder ein Fach – oder lege eine neue Klasse an.")
                .color(p.text_weak),
        );
        ui.add_space(14.0);
        let classes: Vec<_> = self
            .db
            .classes_of_year(self.year)
            .map(|c| (c.id, c.name.clone(), c.students.len()))
            .collect();
        ui.horizontal_wrapped(|ui| {
            for (id, name, n) in classes {
                let courses: Vec<String> =
                    self.db.courses_of_class(id).map(|c| c.title()).collect();
                let resp = egui::Frame::new()
                    .fill(p.sheet)
                    .stroke(egui::Stroke::new(1.0, p.border))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(14))
                    .show(ui, |ui| {
                        ui.set_width(220.0);
                        ui.label(
                            RichText::new(format!("{}  {}", ic::CHALKBOARD_TEACHER, name))
                                .font(theme::bold(18.0))
                                .color(p.accent_text),
                        );
                        ui.label(format!("{n} Schüler"));
                        ui.label(
                            RichText::new(if courses.is_empty() {
                                "Noch keine Fächer".into()
                            } else {
                                courses.join(", ")
                            })
                            .color(p.text_weak),
                        );
                    })
                    .response
                    .interact(egui::Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.clicked() {
                    self.open_view(View::Class(id));
                }
            }
        });
        ui.add_space(14.0);
        if !self.year_archived()
            && primary_button(ui, &format!("{}  Neue Klasse anlegen", ic::PLUS)).clicked()
        {
            self.dialog = Some(Dialog::NewClass {
                name: String::new(),
            });
        }
    }

    pub fn class_view(&mut self, ui: &mut egui::Ui, cfg: &AppConfig) {
        let Some(class_id) = self.current_class() else {
            return;
        };
        let Some(class) = self.db.class(class_id) else {
            return;
        };
        let ctx = ui.ctx().clone();
        let p = Palette::get(&ctx);
        let ro = self.readonly();
        let year = self
            .db
            .year(class.year_id)
            .map(|y| y.name.clone())
            .unwrap_or_default();
        let name = class.name.clone();

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(ic::CHALKBOARD_TEACHER)
                    .size(26.0)
                    .color(p.accent_text),
            );
            ui.vertical(|ui| {
                ui.label(RichText::new(format!("Klasse {name}")).font(theme::bold(22.0)));
                ui.label(
                    RichText::new(format!(
                        "Schuljahr {year} · {} Schüler",
                        class.students.len()
                    ))
                    .color(p.text_weak),
                );
            });
        });
        ui.add_space(10.0);

        // Fächer
        section_title(ui, "Fächer");
        ui.add_space(4.0);
        let courses: Vec<(Id, String, String)> = self
            .db
            .courses_of_class(class_id)
            .map(|c| (c.id, c.title(), c.scale.label().to_string()))
            .collect();
        ui.horizontal_wrapped(|ui| {
            for (id, title, scale) in courses {
                let btn = egui::Button::new(RichText::new(format!(
                    "{}  {}\n{}",
                    ic::BOOK_OPEN,
                    title,
                    scale
                )))
                .min_size(egui::vec2(170.0, 52.0))
                .fill(p.sheet);
                if ui.add(btn).clicked() {
                    self.open_view(View::Course(id));
                }
            }
            if !ro {
                let btn = egui::Button::new(
                    RichText::new(format!("{}  Fach hinzufügen", ic::PLUS)).color(p.accent_text),
                )
                .min_size(egui::vec2(170.0, 52.0));
                if ui.add(btn).clicked() {
                    self.dialog = Some(Dialog::new_course(&self.db, class_id));
                }
            }
        });
        ui.add_space(16.0);

        // Schülerliste
        ui.horizontal(|ui| {
            section_title(ui, "Schüler");
            if !ro {
                ui.add_space(12.0);
                if ui.button(format!("{} Hinzufügen", ic::USER_PLUS)).clicked() {
                    self.add_student(&ctx, class_id);
                }
                if ui
                    .button(format!("{} Importieren …", ic::CLIPBOARD_TEXT))
                    .clicked()
                {
                    self.dialog = Some(Dialog::Import {
                        class: class_id,
                        text: String::new(),
                    });
                }
                if ui
                    .button(format!("{} Sortieren", ic::SORT_ASCENDING))
                    .clicked()
                {
                    self.sort_students(&ctx, class_id);
                }
            }
        });
        ui.add_space(4.0);

        let students = self
            .db
            .class(class_id)
            .map(|c| c.students.clone())
            .unwrap_or_default();
        if students.is_empty() {
            ui.label(RichText::new("Noch keine Schüler. Füge sie einzeln hinzu oder importiere eine Liste (CSV / aus Excel kopiert).").color(p.text_weak));
            return;
        }
        let mut delete: Option<Id> = None;
        let mut pdf_for: Option<Id> = None;
        let mut moved: Option<(usize, usize)> = None;
        card(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Grid::new("students")
                        .striped(true)
                        .num_columns(5)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            for h in ["Nr.", "Nachname", "Vorname", "Geschlecht", ""] {
                                ui.label(
                                    RichText::new(h).font(theme::bold(13.0)).color(p.text_weak),
                                );
                            }
                            ui.end_row();
                            for (i, s) in students.iter().enumerate() {
                                ui.label(RichText::new(format!("{}", i + 1)).color(p.text_weak));
                                let mut st = s.clone();
                                let mut changed = false;
                                let mut focus_gained = false;
                                let r1 = ui
                                    .add_enabled_ui(!ro, |ui| {
                                        ui.add_sized(
                                            [200.0, 24.0],
                                            egui::TextEdit::singleline(&mut st.last_name)
                                                .id(egui::Id::new(("ln", s.id))),
                                        )
                                    })
                                    .inner;
                                let r2 = ui
                                    .add_enabled_ui(!ro, |ui| {
                                        ui.add_sized(
                                            [200.0, 24.0],
                                            egui::TextEdit::singleline(&mut st.first_name)
                                                .id(egui::Id::new(("fn", s.id))),
                                        )
                                    })
                                    .inner;
                                focus_gained |= r1.gained_focus() || r2.gained_focus();
                                changed |= r1.changed() || r2.changed();
                                ui.add_enabled_ui(!ro, |ui| {
                                    egui::ComboBox::from_id_salt(("g", s.id))
                                        .selected_text(st.gender.label())
                                        .width(140.0)
                                        .show_ui(ui, |ui| {
                                            for g in Gender::ALL {
                                                if ui
                                                    .selectable_value(&mut st.gender, g, g.label())
                                                    .clicked()
                                                {
                                                    focus_gained = true;
                                                    changed = true;
                                                }
                                            }
                                        });
                                });
                                ui.horizontal(|ui| {
                                    if ui
                                        .small_button(ic::FILE_PDF)
                                        .on_hover_text("Schülerbogen als PDF")
                                        .clicked()
                                    {
                                        pdf_for = Some(s.id);
                                    }
                                    if !ro {
                                        if ui
                                            .small_button(ic::ARROW_UP)
                                            .on_hover_text("Nach oben")
                                            .clicked()
                                            && i > 0
                                        {
                                            moved = Some((i, i - 1));
                                        }
                                        if ui
                                            .small_button(ic::ARROW_DOWN)
                                            .on_hover_text("Nach unten")
                                            .clicked()
                                            && i + 1 < students.len()
                                        {
                                            moved = Some((i, i + 1));
                                        }
                                        if ui
                                            .small_button(ic::TRASH)
                                            .on_hover_text("Schüler löschen")
                                            .clicked()
                                        {
                                            delete = Some(s.id);
                                        }
                                    }
                                });
                                ui.end_row();
                                if focus_gained {
                                    self.checkpoint();
                                }
                                if changed {
                                    if let Some(c) = self.db.class_mut(class_id)
                                        && let Some(x) =
                                            c.students.iter_mut().find(|x| x.id == s.id)
                                    {
                                        *x = st;
                                    }
                                    self.touch(&ctx);
                                }
                            }
                        });
                });
        });
        if let Some((a, b)) = moved {
            self.checkpoint();
            if let Some(c) = self.db.class_mut(class_id) {
                c.students.swap(a, b);
            }
            self.touch(&ctx);
        }
        if let Some(sid) = pdf_for {
            self.export_pdf(
                &ctx,
                crate::reports::PdfKind::Sheets {
                    class: class_id,
                    student: Some(sid),
                    course: None,
                },
            );
        }
        if let Some(sid) = delete {
            let name = students
                .iter()
                .find(|s| s.id == sid)
                .map(|s| self.student_name(s, cfg.first_name_first))
                .unwrap_or_default();
            self.dialog = Some(Dialog::Confirm {
                title: "Schüler löschen".into(),
                text: format!(
                    "„{name}“ und alle Noten dieses Schülers in allen Fächern der Klasse löschen?"
                ),
                action: crate::dialogs::ConfirmAction::DeleteStudent(class_id, sid),
            });
        }
    }

    pub fn add_student(&mut self, ctx: &egui::Context, class_id: Id) {
        self.checkpoint();
        let id = self.db.alloc_id();
        if let Some(c) = self.db.class_mut(class_id) {
            c.students.push(Student {
                id,
                last_name: String::new(),
                first_name: String::new(),
                gender: Gender::Ohne,
            });
        }
        self.touch(ctx);
        self.open_view(View::Class(class_id));
        ctx.memory_mut(|m| m.request_focus(egui::Id::new(("ln", id))));
    }

    pub fn sort_students(&mut self, ctx: &egui::Context, class_id: Id) {
        self.checkpoint();
        if let Some(c) = self.db.class_mut(class_id) {
            c.students.sort_by(|a, b| {
                let k = |s: &Student| (s.last_name.to_lowercase(), s.first_name.to_lowercase());
                k(a).cmp(&k(b))
            });
        }
        self.touch(ctx);
        self.grid = Default::default();
    }

    pub fn add_area(&mut self, ctx: &egui::Context, course_id: Id) {
        self.checkpoint();
        let id = self.db.alloc_id();
        if let Some(c) = self.db.course_mut(course_id) {
            let n = c.scheme.areas.len() + 1;
            c.scheme.areas.push(Area {
                id,
                name: format!("Leistungsbereich {n}"),
                weight: 1.0,
                columns: Vec::new(),
            });
        }
        self.touch(ctx);
    }

    pub fn add_column_dialog(&mut self, course_id: Id, half: Half) {
        let Some(c) = self.db.course(course_id) else {
            return;
        };
        let Some(area) = c.scheme.areas.first() else {
            self.dialog = Some(Dialog::Message {
                title: "Kein Leistungsbereich".into(),
                text: "Lege zuerst im Bewertungsschema einen Leistungsbereich an.".into(),
            });
            return;
        };
        let n = area.columns.iter().filter(|c| c.half == half).count() + 1;
        self.dialog = Some(Dialog::AddColumn {
            course: course_id,
            area: area.id,
            name: format!("Leistung {n}"),
            weight: 1.0,
            half,
        });
    }

    pub fn export_course_csv(&mut self, ctx: &egui::Context, course_id: Id) {
        let Some(course) = self.db.course(course_id) else {
            return;
        };
        let Some(class) = self.db.class(course.class_id) else {
            return;
        };
        let csv = gradecraft_core::csv::export_course(class, course);
        let file = format!(
            "Noten_{}_{}.csv",
            class.name,
            course.short.clone().max(course.subject.clone())
        );
        let file = file.replace(['/', '\\', ':', ' '], "_");
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(file)
            .add_filter("CSV (Excel)", &["csv"])
            .save_file()
        {
            match std::fs::write(&path, csv) {
                Ok(()) => self.notify(ctx, &format!("Exportiert: {}", path.display()), false),
                Err(e) => self.notify(ctx, &format!("Export fehlgeschlagen: {e}"), true),
            }
        }
    }

    /// Editor für Leistungsbereiche, Spalten und Gewichtungen.
    pub fn scheme_editor(&mut self, ui: &mut egui::Ui, course_id: Id) {
        let ctx = ui.ctx().clone();
        let p = Palette::get(&ctx);
        let ro = self.readonly();
        let Some(course) = self.db.course(course_id).cloned() else {
            return;
        };
        let year_name = self
            .db
            .class(course.class_id)
            .and_then(|c| self.db.year(c.year_id))
            .map(|y| y.name.clone())
            .unwrap_or_default();
        let mut scheme = course.scheme.clone();
        let original = scheme.clone();
        let grades_per_col = |col: Id| course.grades.iter().filter(|g| g.column == col).count();
        let mut focus = false;
        let mut delete_area: Option<Id> = None;
        let mut delete_col: Option<Id> = None;
        let mut add_col: Option<(Id, Half)> = None;
        let total: f64 = scheme.areas.iter().map(|a| a.weight).sum();

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_enabled_ui(!ro, |ui| {
                card(ui, |ui| {
                    section_title(ui, "Schuljahresnote");
                    ui.add_space(4.0);
                    let mut mode_h = matches!(scheme.year_mode, YearMode::Halbjahre { .. });
                    if ui.radio_value(&mut mode_h, true, "Aus Halbjahresnote und 2. Halbjahr gewichten").changed() {
                        focus = true;
                    }
                    if let YearMode::Halbjahre { w1, w2 } = &mut scheme.year_mode {
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            ui.label("1. Halbjahr");
                            focus |= weight_edit(ui, w1).gained_focus();
                            ui.label(":");
                            focus |= weight_edit(ui, w2).gained_focus();
                            ui.label("2. Halbjahr");
                        });
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            ui.label(RichText::new("Eine festgelegte (ggf. pädagogische) Halbjahresnote geht so in die Schuljahresnote ein.").size(12.0).color(p.text_weak));
                        });
                    }
                    if ui.radio_value(&mut mode_h, false, "Aus allen Einzelnoten des Schuljahres").changed() {
                        focus = true;
                    }
                    let is_h = matches!(scheme.year_mode, YearMode::Halbjahre { .. });
                    if mode_h != is_h {
                        scheme.year_mode = if mode_h { YearMode::Halbjahre { w1: 1.0, w2: 1.0 } } else { YearMode::AlleNoten };
                    }
                });
                ui.add_space(10.0);

                for (ai, area) in scheme.areas.iter_mut().enumerate() {
                    card(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(ic::STACK).size(18.0).color(p.accent_text));
                            let r = ui.add(egui::TextEdit::singleline(&mut area.name).font(theme::bold(15.0)).desired_width(260.0));
                            focus |= r.gained_focus();
                            ui.add_space(12.0);
                            ui.label("Gewichtung");
                            focus |= weight_edit(ui, &mut area.weight).gained_focus();
                            let pct = if total > 0.0 { area.weight / total * 100.0 } else { 0.0 };
                            ui.label(RichText::new(format!("= {pct:.0} %")).color(p.accent_text).strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(format!("{} Bereich löschen", ic::TRASH)).clicked() {
                                    delete_area = Some(area.id);
                                }
                                let _ = ai;
                            });
                        });
                        ui.add_space(6.0);
                        for half in [Half::H1, Half::H2] {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(half.label()).strong().color(p.text_weak));
                                if ui.small_button(format!("{} Leistung", ic::PLUS)).clicked() {
                                    add_col = Some((area.id, half));
                                }
                            });
                            let ids: Vec<Id> = area.columns.iter().filter(|c| c.half == half).map(|c| c.id).collect();
                            if ids.is_empty() {
                                ui.label(RichText::new("    – keine –").color(p.text_weak));
                            }
                            for id in ids {
                                let col = area.columns.iter_mut().find(|c| c.id == id).unwrap();
                                ui.horizontal(|ui| {
                                    ui.add_space(20.0);
                                    let r = ui.add(egui::TextEdit::singleline(&mut col.name).desired_width(200.0));
                                    focus |= r.gained_focus();
                                    ui.label("Wertigkeit");
                                    focus |= weight_edit(ui, &mut col.weight).gained_focus();
                                    egui::ComboBox::from_id_salt(("half", id)).selected_text(col.half.label()).width(110.0).show_ui(ui, |ui| {
                                        for h in [Half::H1, Half::H2] {
                                            if ui.selectable_value(&mut col.half, h, h.label()).clicked() {
                                                focus = true;
                                            }
                                        }
                                    });
                                    // Verknüpfung mit der Stundenerfassung
                                    let mut linked = col.oral.is_some();
                                    if ui
                                        .checkbox(&mut linked, "Stundennoten")
                                        .on_hover_text("Note automatisch aus den mündlichen Noten (Register „Mündlich“) des Zeitraums bilden")
                                        .changed()
                                    {
                                        focus = true;
                                        col.oral = linked.then(|| crate::oral::default_range(&year_name, col.half));
                                    }
                                    if let Some(range) = col.oral.as_mut() {
                                        let (c1, r1) = crate::oral::date_edit(ui, egui::Id::new(("oral-from", id)), &mut range.from, 80.0);
                                        ui.label("–");
                                        let (c2, r2) = crate::oral::date_edit(ui, egui::Id::new(("oral-to", id)), &mut range.to, 80.0);
                                        focus |= c1 || c2 || r1.gained_focus() || r2.gained_focus();
                                    }
                                    let n = grades_per_col(id);
                                    if n > 0 {
                                        ui.label(RichText::new(format!("{n} Noten")).size(12.0).color(p.text_weak));
                                    }
                                    if ui.small_button(ic::TRASH).on_hover_text("Leistung löschen").clicked() {
                                        delete_col = Some(id);
                                    }
                                });
                            }
                            ui.add_space(4.0);
                        }
                    });
                    ui.add_space(8.0);
                }
                ui.horizontal(|ui| {
                    if primary_button(ui, &format!("{}  Leistungsbereich hinzufügen", ic::PLUS)).clicked() {
                        self.add_area(&ctx, course_id);
                    }
                    ui.label(RichText::new(format!(
                        "Gewichtung gesamt: {}",
                        scheme.areas.iter().map(|a| fmt_weight(a.weight)).collect::<Vec<_>>().join(" : ")
                    )).color(p.text_weak));
                });
            });
        });

        if focus {
            self.checkpoint();
        }
        if serde_json::to_string(&scheme).ok() != serde_json::to_string(&original).ok() {
            if let Some(c) = self.db.course_mut(course_id) {
                c.scheme = scheme;
            }
            self.touch(&ctx);
        }
        if let Some((area, half)) = add_col
            && let Some(a) = self
                .db
                .course(course_id)
                .and_then(|c| c.scheme.areas.iter().find(|a| a.id == area))
        {
            let n = a.columns.iter().filter(|c| c.half == half).count() + 1;
            self.dialog = Some(Dialog::AddColumn {
                course: course_id,
                area,
                name: format!("Leistung {n}"),
                weight: 1.0,
                half,
            });
        }
        if let Some(a) = delete_area {
            self.dialog = Some(Dialog::confirm_delete_area(&self.db, course_id, a));
        }
        if let Some(c) = delete_col {
            self.dialog = Some(Dialog::confirm_delete_column(&self.db, course_id, c));
        }
    }
}
