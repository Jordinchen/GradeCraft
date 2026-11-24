//! Notenbesprechung: eigenes Fenster mit den Noten genau eines Schülers –
//! z. B. für den Beamer oder einen zweiten Bildschirm im Schülergespräch.

use eframe::egui::{self, RichText, Stroke, ViewportBuilder, ViewportId, vec2};
use egui_phosphor::regular as ic;
use gradecraft_core::{Course, Entry, Half, Id, OralMark, OralValue, Period, YearMode, calc};

use crate::config::AppConfig;
use crate::session::Session;
use crate::theme::{self, Palette};
use crate::widgets::fmt_weight;

/// Offene Notenbesprechung.
#[derive(Clone, Copy, PartialEq)]
pub struct Review {
    pub class: Id,
    pub student: Id,
    pub course: Id,
    /// Vorschläge und Zeugnisnoten anzeigen
    pub show_final: bool,
    /// Schriftgröße (Faktor)
    pub scale: f32,
}

enum Action {
    None,
    Close,
    Student(Id),
    Course(Id),
    ShowFinal(bool),
    Scale(f32),
}

impl Session {
    /// Öffnet die Besprechung für einen Schüler. Ohne Fach wird das aktuell
    /// geöffnete bzw. das erste Fach der Klasse gewählt.
    pub fn open_review(&mut self, class: Id, student: Id, course: Option<Id>) {
        let course = course
            .or_else(|| {
                self.current_course()
                    .filter(|c| self.db.course(*c).is_some_and(|c| c.class_id == class))
            })
            .or_else(|| self.db.courses_of_class(class).next().map(|c| c.id));
        let Some(course) = course else {
            self.dialog = Some(crate::dialogs::Dialog::Message {
                title: "Notenbesprechung".into(),
                text: "Diese Klasse hat noch kein Fach.".into(),
            });
            return;
        };
        let (show_final, scale) = self
            .review
            .map(|r| (r.show_final, r.scale))
            .unwrap_or((true, 1.0));
        self.review = Some(Review {
            class,
            student,
            course,
            show_final,
            scale,
        });
    }

    /// Zeichnet das Besprechungsfenster (eigenes Betriebssystem-Fenster).
    pub fn review_window(&mut self, ctx: &egui::Context, cfg: &AppConfig) {
        let Some(review) = self.review else { return };
        let (Some(class), Some(course)) =
            (self.db.class(review.class), self.db.course(review.course))
        else {
            self.review = None;
            return;
        };
        let Some(student) = class.students.iter().find(|s| s.id == review.student) else {
            self.review = None;
            return;
        };
        let name = self.student_name(student, cfg.first_name_first);
        let builder = ViewportBuilder::default()
            .with_title(format!("Notenbesprechung – {name}"))
            .with_app_id("gradecraft-review")
            .with_inner_size([820.0, 700.0])
            .with_min_inner_size([520.0, 400.0]);
        let db = &self.db;
        let action = ctx.show_viewport_immediate(
            ViewportId::from_hash_of("gradecraft-review"),
            builder,
            |ui, _class| {
                if ui.input(|i| i.viewport().close_requested()) {
                    return Action::Close;
                }
                review_ui(ui, db, class, course, student.id, &name, review)
            },
        );
        match action {
            Action::None => {}
            Action::Close => self.review = None,
            Action::Student(s) => {
                self.review = Some(Review {
                    student: s,
                    ..review
                })
            }
            Action::Course(c) => {
                self.review = Some(Review {
                    course: c,
                    ..review
                })
            }
            Action::ShowFinal(b) => {
                self.review = Some(Review {
                    show_final: b,
                    ..review
                })
            }
            Action::Scale(f) => self.review = Some(Review { scale: f, ..review }),
        }
    }
}

fn review_ui(
    ui: &mut egui::Ui,
    db: &gradecraft_core::Database,
    class: &gradecraft_core::SchoolClass,
    course: &Course,
    student: Id,
    name: &str,
    review: Review,
) -> Action {
    let p = Palette::get(ui.ctx());
    let f = review.scale;
    let mut action = Action::None;
    ui.painter().rect_filled(ui.max_rect(), 0.0, p.canvas);

    egui::Frame::new()
        .inner_margin(egui::Margin::same(20))
        .show(ui, |ui| {
            // Bedienleiste
            ui.horizontal(|ui| {
                let idx = class
                    .students
                    .iter()
                    .position(|s| s.id == student)
                    .unwrap_or(0);
                if ui
                    .add_enabled(
                        idx > 0,
                        egui::Button::new(format!("{} Vorheriger", ic::CARET_LEFT)),
                    )
                    .clicked()
                {
                    action = Action::Student(class.students[idx - 1].id);
                }
                ui.label(
                    RichText::new(format!("{} / {}", idx + 1, class.students.len()))
                        .color(p.text_weak),
                );
                if ui
                    .add_enabled(
                        idx + 1 < class.students.len(),
                        egui::Button::new(format!("Nächster {}", ic::CARET_RIGHT)),
                    )
                    .clicked()
                {
                    action = Action::Student(class.students[idx + 1].id);
                }
                ui.add_space(12.0);
                egui::ComboBox::from_id_salt("review-course")
                    .selected_text(course.title())
                    .show_ui(ui, |ui| {
                        for c in db.courses_of_class(class.id) {
                            if ui.selectable_label(c.id == course.id, c.title()).clicked() {
                                action = Action::Course(c.id);
                            }
                        }
                    });
                ui.add_space(12.0);
                let mut show = review.show_final;
                if ui
                    .checkbox(&mut show, "Vorschlag & Zeugnisnote zeigen")
                    .changed()
                {
                    action = Action::ShowFinal(show);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(RichText::new(ic::MAGNIFYING_GLASS_PLUS).size(16.0))
                        .on_hover_text("Größer")
                        .clicked()
                    {
                        action = Action::Scale((f + 0.1).min(2.0));
                    }
                    if ui
                        .button(RichText::new(ic::MAGNIFYING_GLASS_MINUS).size(16.0))
                        .on_hover_text("Kleiner")
                        .clicked()
                    {
                        action = Action::Scale((f - 0.1).max(0.7));
                    }
                });
            });
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            // Kopf
            let year = db
                .year(class.year_id)
                .map(|y| y.name.clone())
                .unwrap_or_default();
            ui.label(RichText::new(name).font(theme::bold(30.0 * f)));
            ui.label(
                RichText::new(format!(
                    "{} · Klasse {} · Schuljahr {year}",
                    course.title(),
                    class.name
                ))
                .size(15.0 * f)
                .color(p.text_weak),
            );
            ui.add_space(10.0 * f);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for half in [Half::H1, Half::H2] {
                        half_section(ui, course, student, half, review);
                    }
                    oral_summary(ui, course, student, f);
                });
        });
    action
}

fn half_section(ui: &mut egui::Ui, course: &Course, student: Id, half: Half, review: Review) {
    let p = Palette::get(ui.ctx());
    let f = review.scale;
    let scale = course.scale;
    let has_columns = course
        .scheme
        .areas
        .iter()
        .any(|a| a.columns.iter().any(|c| c.half == half));
    if !has_columns {
        return;
    }
    let period = if half == Half::H1 {
        Period::Halbjahr
    } else {
        Period::Schuljahr
    };
    let all_year = half == Half::H2 && course.scheme.year_mode == YearMode::AlleNoten;
    let halves: Vec<Half> = if all_year {
        vec![Half::H1, Half::H2]
    } else {
        vec![half]
    };

    egui::Frame::new()
        .fill(p.sheet)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(half.label())
                    .font(theme::bold(20.0 * f))
                    .color(p.accent_text),
            );
            ui.add_space(6.0 * f);
            let total: f64 = course.scheme.areas.iter().map(|a| a.weight).sum();
            egui::Grid::new(("review", half == Half::H1))
                .num_columns(4)
                .spacing([24.0 * f, 8.0 * f])
                .striped(true)
                .show(ui, |ui| {
                    for area in &course.scheme.areas {
                        let cols: Vec<_> = area.columns.iter().filter(|c| c.half == half).collect();
                        if cols.is_empty() && !all_year {
                            continue;
                        }
                        let pct = if total > 0.0 {
                            area.weight / total * 100.0
                        } else {
                            0.0
                        };
                        ui.label(RichText::new(&area.name).font(theme::bold(16.0 * f)));
                        ui.label(
                            RichText::new(format!("{pct:.0} %"))
                                .size(13.0 * f)
                                .color(p.text_weak),
                        );
                        ui.label("");
                        ui.label("");
                        ui.end_row();
                        for col in cols {
                            let eff = course.effective_entry(student, col.id);
                            ui.label(RichText::new(format!("    {}", col.name)).size(16.0 * f));
                            ui.label(
                                RichText::new(if (col.weight - 1.0).abs() > 1e-9 {
                                    format!("×{}", fmt_weight(col.weight))
                                } else {
                                    String::new()
                                })
                                .size(13.0 * f)
                                .color(p.text_weak),
                            );
                            let (text, color) = match eff {
                                Some((Entry::Note(v), true)) => {
                                    (scale.format_avg(v), p.accent_text)
                                }
                                Some((Entry::Note(v), false)) => {
                                    (scale.format(v), p.grade_color(scale.level(v)))
                                }
                                Some((e @ Entry::Ungenuegend(_), _)) => (e.format(scale), p.bad),
                                Some((Entry::Fehlt, _)) => ("–".into(), p.text_weak),
                                None => ("noch offen".into(), p.text_weak),
                            };
                            let big = eff.is_some_and(|(e, _)| e != Entry::Fehlt);
                            ui.label(
                                RichText::new(text)
                                    .font(if big {
                                        theme::bold(20.0 * f)
                                    } else {
                                        egui::FontId::proportional(14.0 * f)
                                    })
                                    .color(color),
                            );
                            // Erläuterung
                            let mut note = match eff {
                                Some((Entry::Ungenuegend(_) | Entry::Fehlt, _)) => {
                                    eff.map(|(e, _)| e.describe(scale)).unwrap_or_default()
                                }
                                Some((_, true)) => col
                                    .oral
                                    .map(|r| {
                                        format!(
                                            "aus {} Stundennoten",
                                            course.oral_in_range(student, r).count()
                                        )
                                    })
                                    .unwrap_or_default(),
                                _ => String::new(),
                            };
                            if let Some(g) = course
                                .grade(student, col.id)
                                .filter(|g| !g.comment.is_empty())
                            {
                                if !note.is_empty() {
                                    note.push_str(" · ");
                                }
                                note.push_str(&g.comment);
                            }
                            ui.label(RichText::new(note).size(13.0 * f).color(p.text_weak));
                            ui.end_row();
                        }
                        let avg = calc::area_average(course, student, area.id, &halves);
                        ui.label(
                            RichText::new(format!("    Durchschnitt {}", area.name))
                                .size(14.0 * f)
                                .color(p.text_weak),
                        );
                        ui.label("");
                        ui.label(
                            RichText::new(avg.map(|v| scale.format_avg(v)).unwrap_or("–".into()))
                                .font(theme::bold(16.0 * f))
                                .color(p.text_weak),
                        );
                        ui.label("");
                        ui.end_row();
                    }
                });
            ui.add_space(10.0 * f);
            ui.separator();
            ui.add_space(6.0 * f);
            ui.horizontal_wrapped(|ui| {
                let stat = |ui: &mut egui::Ui, k: &str, v: String, color: egui::Color32| {
                    ui.label(RichText::new(k).size(15.0 * f).color(p.text_weak));
                    ui.label(RichText::new(v).font(theme::bold(22.0 * f)).color(color));
                    ui.add_space(28.0 * f);
                };
                let avg = calc::period_average(course, student, period);
                stat(
                    ui,
                    if half == Half::H1 {
                        "Durchschnitt Halbjahr"
                    } else {
                        "Durchschnitt Schuljahr"
                    },
                    avg.map(|v| scale.format_avg(v)).unwrap_or("–".into()),
                    p.text,
                );
                if review.show_final {
                    if let Some(s) = calc::suggestion(course, student, period) {
                        stat(ui, "Vorschlag", scale.format_final_long(s), p.text);
                    }
                    if let Some(fin) = course.final_grade(student, period) {
                        stat(
                            ui,
                            period.label(),
                            scale.format_final_long(fin.grade),
                            p.accent_text,
                        );
                    }
                }
            });
            if review.show_final
                && let Some(fin) = course
                    .final_grade(student, period)
                    .filter(|f| f.is_pedagogical() && !f.reason.is_empty())
            {
                ui.label(
                    RichText::new(format!("Begründung: {}", fin.reason))
                        .size(14.0 * f)
                        .color(p.text_weak),
                );
            }
        });
    ui.add_space(14.0 * f);
}

/// Übersicht der mündlichen Mitarbeit (Kürzel je Halbjahr).
fn oral_summary(ui: &mut egui::Ui, course: &Course, student: Id, f: f32) {
    let p = Palette::get(ui.ctx());
    let entries: Vec<_> = course
        .oral
        .iter()
        .filter(|o| o.student == student)
        .collect();
    if entries.is_empty() {
        return;
    }
    egui::Frame::new()
        .fill(p.sheet)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new("Mündliche Mitarbeit")
                    .font(theme::bold(20.0 * f))
                    .color(p.accent_text),
            );
            ui.add_space(6.0 * f);
            ui.horizontal_wrapped(|ui| {
                for m in OralMark::ALL {
                    let n = entries
                        .iter()
                        .filter(|o| o.value == OralValue::Mark(m))
                        .count();
                    ui.label(RichText::new(m.label()).font(theme::bold(18.0 * f)));
                    ui.label(
                        RichText::new(format!("{n}×"))
                            .size(16.0 * f)
                            .color(p.text_weak),
                    );
                    ui.add_space(18.0 * f);
                }
                let notes = entries
                    .iter()
                    .filter(|o| matches!(o.value, OralValue::Note(_)))
                    .count();
                if notes > 0 {
                    ui.label(
                        RichText::new(format!("Noten: {notes}×"))
                            .size(16.0 * f)
                            .color(p.text_weak),
                    );
                }
            });
            ui.label(
                RichText::new(format!("{} Stunden erfasst", entries.len()))
                    .size(13.0 * f)
                    .color(p.text_weak),
            );
        });
    ui.add_space(vec2(0.0, 10.0).y);
}
