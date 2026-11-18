//! Modale Dialoge.

use eframe::egui::{self, RichText};
use egui_phosphor::regular as ic;
use gradecraft_core::calc;
use gradecraft_core::{
    Column, Course, Database, FinalRecord, Half, Id, Period, ScaleKind, SchoolClass, Template,
};

use crate::session::{Session, View};
use crate::theme::{self, Palette};
use crate::widgets::{primary_button, secondary_button, weight_edit};

pub enum ConfirmAction {
    DeleteClass(Id),
    DeleteCourse(Id),
    DeleteStudent(Id, Id),
    DeleteArea(Id, Id),
    DeleteColumn(Id, Id),
    ApplyTemplate(Id, Id),
    DeleteTemplate(Id),
    DeleteYear(Id),
}

pub enum Dialog {
    NewClass {
        name: String,
    },
    RenameClass {
        id: Id,
        name: String,
    },
    NewCourse {
        class: Id,
        subject: String,
        short: String,
        scale: ScaleKind,
        template: Option<Id>,
        written: usize,
        other: usize,
    },
    EditCourse {
        id: Id,
        subject: String,
        short: String,
    },
    AddColumn {
        course: Id,
        area: Id,
        name: String,
        weight: f64,
        half: Half,
    },
    EditColumn {
        course: Id,
        column: Id,
        name: String,
        weight: f64,
        half: Half,
    },
    Final {
        course: Id,
        student: Id,
        period: Period,
        grade: f64,
        reason: String,
    },
    Comment {
        course: Id,
        student: Id,
        column: Id,
        text: String,
    },
    Import {
        class: Id,
        text: String,
    },
    SaveTemplate {
        course: Id,
        name: String,
    },
    ApplyTemplate {
        course: Id,
        template: Option<Id>,
    },
    ChangePassword {
        old: String,
        new1: String,
        new2: String,
        error: Option<String>,
    },
    NewYear {
        name: String,
        carry: Vec<(Id, bool, String)>,
        with_courses: bool,
    },
    Confirm {
        title: String,
        text: String,
        action: ConfirmAction,
    },
    Message {
        title: String,
        text: String,
    },
    UnsavedOnExit,
    InputHelp,
    Privacy,
    About,
}

impl Dialog {
    pub fn new_course(_db: &Database, class: Id) -> Self {
        Dialog::NewCourse {
            class,
            subject: String::new(),
            short: String::new(),
            scale: ScaleKind::Noten,
            template: None,
            written: 4,
            other: 4,
        }
    }

    pub fn final_grade(db: &Database, course: Id, student: Id, period: Period) -> Self {
        let c = db.course(course);
        let existing = c.and_then(|c| c.final_grade(student, period));
        let sug = c.and_then(|c| calc::suggestion(c, student, period));
        Dialog::Final {
            course,
            student,
            period,
            grade: existing
                .map(|f| f.grade)
                .or(sug)
                .unwrap_or_else(|| c.map(|c| c.scale.best()).unwrap_or(1.0)),
            reason: existing.map(|f| f.reason.clone()).unwrap_or_default(),
        }
    }

    pub fn confirm_delete_class(db: &Database, class: Id) -> Self {
        let c = db.class(class);
        let n_courses = db.courses_of_class(class).count();
        Dialog::Confirm {
            title: "Klasse löschen".into(),
            text: format!(
                "Klasse „{}“ mit {} Schülern, {} Fächern und allen Noten endgültig löschen?",
                c.map(|c| c.name.as_str()).unwrap_or(""),
                c.map(|c| c.students.len()).unwrap_or(0),
                n_courses
            ),
            action: ConfirmAction::DeleteClass(class),
        }
    }

    pub fn confirm_delete_course(db: &Database, course: Id) -> Self {
        let c = db.course(course);
        Dialog::Confirm {
            title: "Fach löschen".into(),
            text: format!(
                "Fach „{}“ mit {} eingetragenen Noten endgültig löschen?",
                c.map(|c| c.title()).unwrap_or_default(),
                c.map(|c| c.grades.len()).unwrap_or(0)
            ),
            action: ConfirmAction::DeleteCourse(course),
        }
    }

    pub fn confirm_delete_area(db: &Database, course: Id, area: Id) -> Self {
        let c = db.course(course);
        let a = c.and_then(|c| c.scheme.areas.iter().find(|a| a.id == area));
        let n = c
            .zip(a)
            .map(|(c, a)| {
                c.grades
                    .iter()
                    .filter(|g| a.columns.iter().any(|col| col.id == g.column))
                    .count()
            })
            .unwrap_or(0);
        Dialog::Confirm {
            title: "Leistungsbereich löschen".into(),
            text: format!(
                "Leistungsbereich „{}“ mit {} Spalten und {n} Noten löschen?",
                a.map(|a| a.name.as_str()).unwrap_or(""),
                a.map(|a| a.columns.len()).unwrap_or(0)
            ),
            action: ConfirmAction::DeleteArea(course, area),
        }
    }

    pub fn confirm_delete_column(db: &Database, course: Id, column: Id) -> Self {
        let c = db.course(course);
        let name = c
            .and_then(|c| c.find_column(column))
            .map(|(_, col)| col.name.clone())
            .unwrap_or_default();
        let n = c
            .map(|c| c.grades.iter().filter(|g| g.column == column).count())
            .unwrap_or(0);
        Dialog::Confirm {
            title: "Leistung löschen".into(),
            text: format!("Leistung „{name}“ mit {n} eingetragenen Noten löschen?"),
            action: ConfirmAction::DeleteColumn(course, column),
        }
    }
}

enum Outcome {
    Keep,
    Close,
}

impl Session {
    pub fn show_dialog(&mut self, ctx: &egui::Context, cfg: &crate::config::AppConfig) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let p = Palette::get(ctx);
        let mut outcome = Outcome::Keep;
        let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));

        let modal = egui::Modal::new(egui::Id::new("dialog")).show(ctx, |ui| {
            ui.set_min_width(420.0);
            ui.set_max_width(640.0);
            match &mut dialog {
                Dialog::NewClass { name } => {
                    title(ui, ic::CHALKBOARD_TEACHER, "Neue Klasse");
                    ui.label("Klassenbezeichnung (z. B. 9b oder IFK126)");
                    let r = ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                    r.request_focus();
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if buttons(ui, "Anlegen", !name.trim().is_empty()) || (enter && !name.trim().is_empty()) {
                        self.checkpoint();
                        let id = self.db.alloc_id();
                        self.db.classes.push(SchoolClass { id, year_id: self.year, name: name.trim().to_string(), students: Vec::new() });
                        self.touch(ctx);
                        self.open_view(View::Class(id));
                        outcome = Outcome::Close;
                    } else if cancelled(ui) {
                        outcome = Outcome::Close;
                    }
                }
                Dialog::RenameClass { id, name } => {
                    title(ui, ic::PENCIL_SIMPLE, "Klasse umbenennen");
                    let r = ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                    r.request_focus();
                    if buttons(ui, "Übernehmen", !name.trim().is_empty()) {
                        self.checkpoint();
                        if let Some(c) = self.db.class_mut(*id) {
                            c.name = name.trim().to_string();
                        }
                        self.touch(ctx);
                        outcome = Outcome::Close;
                    } else if cancelled(ui) {
                        outcome = Outcome::Close;
                    }
                }
                Dialog::NewCourse { class, subject, short, scale, template, written, other } => {
                    let cname = self.db.class(*class).map(|c| c.name.clone()).unwrap_or_default();
                    title(ui, ic::BOOK_OPEN, &format!("Neues Fach in Klasse {cname}"));
                    egui::Grid::new("nc").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                        ui.label("Fachbezeichnung");
                        ui.add(egui::TextEdit::singleline(subject).hint_text("z. B. Deutsch").desired_width(260.0));
                        ui.end_row();
                        ui.label("Kürzel");
                        ui.add(egui::TextEdit::singleline(short).hint_text("z. B. D").desired_width(80.0));
                        ui.end_row();
                        ui.label("Notenskala");
                        ui.horizontal(|ui| {
                            for s in ScaleKind::ALL {
                                ui.radio_value(scale, s, s.label());
                            }
                        });
                        ui.end_row();
                        ui.label("Bewertungsschema");
                        egui::ComboBox::from_id_salt("tpl")
                            .selected_text(template.and_then(|t| self.db.templates.iter().find(|x| x.id == t)).map(|t| t.name.clone()).unwrap_or("Standard (schriftlich 1 : sonstige 1)".into()))
                            .width(260.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(template, None, "Standard (schriftlich 1 : sonstige 1)");
                                for t in &self.db.templates {
                                    ui.selectable_value(template, Some(t.id), format!("Vorlage: {}", t.name));
                                }
                            });
                        ui.end_row();
                        if template.is_none() {
                            ui.label("Klassenarbeiten / Schuljahr");
                            ui.add(egui::DragValue::new(written).range(0..=12));
                            ui.end_row();
                            ui.label("Sonstige Leistungen / Schuljahr");
                            ui.add(egui::DragValue::new(other).range(0..=24));
                            ui.end_row();
                        }
                    });
                    if template.is_none() {
                        ui.label(RichText::new("Die Spalten werden gleichmäßig auf beide Halbjahre verteilt und können danach frei umbenannt, ergänzt und gewichtet werden.").size(12.0).color(p.text_weak));
                    }
                    if buttons(ui, "Anlegen", !subject.trim().is_empty()) {
                        self.checkpoint();
                        let (scheme, sc) = match template.and_then(|t| self.db.templates.iter().find(|x| x.id == t).cloned()) {
                            Some(t) => (self.db.clone_scheme(&t.scheme), t.scale),
                            None => (self.db.default_scheme(*written, *other), *scale),
                        };
                        let sc = if template.is_some() { sc } else { *scale };
                        let id = self.db.alloc_id();
                        self.db.courses.push(Course {
                            id,
                            class_id: *class,
                            subject: subject.trim().to_string(),
                            short: short.trim().to_string(),
                            scale: sc,
                            scheme,
                            grades: Vec::new(),
                            finals: Vec::new(),
                        });
                        self.touch(ctx);
                        self.open_view(View::Course(id));
                        self.course_tab = crate::session::CourseTab::H1;
                        outcome = Outcome::Close;
                    } else if cancelled(ui) {
                        outcome = Outcome::Close;
                    }
                }
                Dialog::EditCourse { id, subject, short } => {
                    title(ui, ic::PENCIL_SIMPLE, "Fach bearbeiten");
                    egui::Grid::new("ec").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                        ui.label("Fachbezeichnung");
                        ui.add(egui::TextEdit::singleline(subject).desired_width(260.0));
                        ui.end_row();
                        ui.label("Kürzel");
                        ui.add(egui::TextEdit::singleline(short).desired_width(80.0));
                        ui.end_row();
                    });
                    if buttons(ui, "Übernehmen", !subject.trim().is_empty()) {
                        self.checkpoint();
                        if let Some(c) = self.db.course_mut(*id) {
                            c.subject = subject.trim().to_string();
                            c.short = short.trim().to_string();
                        }
                        self.touch(ctx);
                        outcome = Outcome::Close;
                    } else if cancelled(ui) {
                        outcome = Outcome::Close;
                    }
                }
                Dialog::AddColumn { course, area, name, weight, half } => {
                    title(ui, ic::LIST_PLUS, "Leistung hinzufügen");
                    let areas: Vec<(Id, String)> = self.db.course(*course).map(|c| c.scheme.areas.iter().map(|a| (a.id, a.name.clone())).collect()).unwrap_or_default();
                    column_form(ui, name, weight, half, Some((area, &areas)));
                    if buttons(ui, "Hinzufügen", !name.trim().is_empty()) {
                        self.checkpoint();
                        let id = self.db.alloc_id();
                        if let Some(a) = self.db.course_mut(*course).and_then(|c| c.scheme.areas.iter_mut().find(|a| a.id == *area)) {
                            a.columns.push(Column { id, name: name.trim().to_string(), weight: *weight, half: *half });
                        }
                        self.touch(ctx);
                        outcome = Outcome::Close;
                    } else if cancelled(ui) {
                        outcome = Outcome::Close;
                    }
                }
                Dialog::EditColumn { course, column, name, weight, half } => {
                    title(ui, ic::PENCIL_SIMPLE, "Leistung bearbeiten");
                    column_form(ui, name, weight, half, None);
                    if buttons(ui, "Übernehmen", !name.trim().is_empty()) {
                        self.checkpoint();
                        if let Some(c) = self.db.course_mut(*course) {
                            for a in &mut c.scheme.areas {
                                if let Some(col) = a.columns.iter_mut().find(|c| c.id == *column) {
                                    col.name = name.trim().to_string();
                                    col.weight = *weight;
                                    col.half = *half;
                                }
                            }
                        }
                        self.touch(ctx);
                        outcome = Outcome::Close;
                    } else if cancelled(ui) {
                        outcome = Outcome::Close;
                    }
                }
                Dialog::Final { course, student, period, grade, reason } => {
                    let Some(c) = self.db.course(*course).cloned() else {
                        outcome = Outcome::Close;
                        return;
                    };
                    let sname = self
                        .db
                        .class(c.class_id)
                        .and_then(|k| k.students.iter().find(|s| s.id == *student))
                        .map(|s| self.student_name(s, cfg.first_name_first))
                        .unwrap_or_default();
                    title(ui, ic::SEAL_CHECK, &format!("{} – {}", period.label(), sname));
                    let avg = calc::period_average(&c, *student, *period);
                    let sug = calc::suggestion(&c, *student, *period);
                    let scale = c.scale;
                    egui::Grid::new("fg").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                        for a in &c.scheme.areas {
                            let halves: &[Half] = match (*period, c.scheme.year_mode) {
                                (Period::Halbjahr, _) => &[Half::H1],
                                (Period::Schuljahr, gradecraft_core::YearMode::AlleNoten) => &[Half::H1, Half::H2],
                                (Period::Schuljahr, _) => &[Half::H2],
                            };
                            ui.label(format!("Ø {} (×{})", a.name, crate::widgets::fmt_weight(a.weight)));
                            ui.label(calc::area_average(&c, *student, a.id, halves).map(|v| scale.format_avg(v)).unwrap_or("–".into()));
                            ui.end_row();
                        }
                        if *period == Period::Schuljahr
                            && let gradecraft_core::YearMode::Halbjahre { w1, w2 } = c.scheme.year_mode {
                                ui.label(format!("Halbjahresnote (×{}) / 2. HJ (×{})", crate::widgets::fmt_weight(w1), crate::widgets::fmt_weight(w2)));
                                ui.label(
                                    c.final_grade(*student, Period::Halbjahr)
                                        .map(|f| scale.format_final(f.grade))
                                        .or_else(|| calc::period_average(&c, *student, Period::Halbjahr).map(|v| format!("Ø {}", scale.format_avg(v))))
                                        .unwrap_or("–".into()),
                                );
                                ui.end_row();
                            }
                        ui.label(RichText::new("Gesamtdurchschnitt").strong());
                        ui.label(RichText::new(avg.map(|v| scale.format_avg(v)).unwrap_or("–".into())).strong());
                        ui.end_row();
                        ui.label(RichText::new("Rechnerischer Vorschlag").strong());
                        ui.label(RichText::new(sug.map(|v| scale.format_final_long(v)).unwrap_or("–".into())).strong().color(p.accent_text));
                        ui.end_row();
                    });
                    ui.add_space(10.0);
                    if let Some(s) = sug
                        && primary_button(ui, &format!("{}  Vorschlag {} bestätigen", ic::CHECK, scale.format_final(s))).clicked() {
                            self.checkpoint();
                            if let Some(cm) = self.db.course_mut(*course) {
                                cm.set_final(FinalRecord { student: *student, period: *period, grade: s, suggestion: Some(s), reason: String::new() });
                            }
                            self.touch(ctx);
                            outcome = Outcome::Close;
                        }
                    ui.add_space(10.0);
                    ui.separator();
                    ui.label(RichText::new(format!("{} Pädagogische Notenbildung", ic::HAND_HEART)).font(theme::bold(15.0)).color(p.accent_text));
                    ui.horizontal_wrapped(|ui| {
                        for g in scale.final_grades() {
                            let sel = (*grade - g).abs() < 1e-9;
                            let txt = RichText::new(scale.format_final(g)).size(15.0);
                            let txt = if sel { txt.color(egui::Color32::WHITE).strong() } else { txt };
                            let mut b = egui::Button::new(txt).min_size(egui::vec2(34.0, 30.0));
                            if sel {
                                b = b.fill(theme::ACCENT);
                            }
                            if ui.add(b).on_hover_text(scale.word(g)).clicked() {
                                *grade = g;
                            }
                        }
                    });
                    ui.label(format!("Gewählt: {}", scale.format_final_long(*grade)));
                    ui.label("Begründung (wird gespeichert und exportiert)");
                    ui.add(egui::TextEdit::multiline(reason).desired_rows(3).desired_width(f32::INFINITY).hint_text("z. B. deutliche Leistungssteigerung im Verlauf des Halbjahres"));
                    let deviates = sug.is_none_or(|s| (s - *grade).abs() > 1e-9);
                    if deviates && reason.trim().is_empty() {
                        ui.label(RichText::new(format!("{} Bei Abweichung vom Vorschlag wird eine Begründung empfohlen.", ic::INFO)).size(12.0).color(p.warn));
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if primary_button(ui, "Note festlegen").clicked() {
                            self.checkpoint();
                            if let Some(cm) = self.db.course_mut(*course) {
                                cm.set_final(FinalRecord { student: *student, period: *period, grade: *grade, suggestion: sug, reason: reason.trim().to_string() });
                            }
                            self.touch(ctx);
                            outcome = Outcome::Close;
                        }
                        let has = c.final_grade(*student, *period).is_some();
                        if has && secondary_button(ui, &format!("{} Festlegung aufheben", ic::X)).clicked() {
                            self.checkpoint();
                            if let Some(cm) = self.db.course_mut(*course) {
                                cm.clear_final(*student, *period);
                            }
                            self.touch(ctx);
                            outcome = Outcome::Close;
                        }
                        if secondary_button(ui, "Abbrechen").clicked() {
                            outcome = Outcome::Close;
                        }
                    });
                }
                Dialog::Comment { course, student, column, text } => {
                    title(ui, ic::CHAT_TEXT, "Kommentar zur Note");
                    ui.add(egui::TextEdit::multiline(text).desired_rows(3).desired_width(f32::INFINITY));
                    if buttons(ui, "Speichern", true) {
                        self.checkpoint();
                        if let Some(g) = self.db.course_mut(*course).and_then(|c| c.grades.iter_mut().find(|g| g.student == *student && g.column == *column)) {
                            g.comment = text.trim().to_string();
                        }
                        self.touch(ctx);
                        outcome = Outcome::Close;
                    } else if cancelled(ui) {
                        outcome = Outcome::Close;
                    }
                }
                Dialog::Import { class, text } => {
                    title(ui, ic::CLIPBOARD_TEXT, "Schülerliste importieren");
                    ui.label("Füge die Liste hier ein (z. B. aus Excel, SchILD oder WebUntis kopiert) oder lade eine CSV-Datei.");
                    ui.label(RichText::new("Erkannt werden Spalten Nachname, Vorname, Geschlecht (mit oder ohne Kopfzeile) sowie „Nachname, Vorname“.").size(12.0).color(p.text_weak));
                    ui.horizontal(|ui| {
                        if ui.button(format!("{} CSV-Datei laden …", ic::FOLDER_OPEN)).clicked()
                            && let Some(path) = rfd::FileDialog::new().add_filter("CSV / Text", &["csv", "txt"]).pick_file() {
                                match std::fs::read(&path) {
                                    Ok(b) => *text = decode_text(&b),
                                    Err(e) => self.notify(ctx, &format!("Datei nicht lesbar: {e}"), true),
                                }
                            }
                    });
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                        ui.add(egui::TextEdit::multiline(text).desired_rows(8).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace).hint_text("Nachname;Vorname;Geschlecht\nMüller;Anna;w\nSchmidt;Ben;m"));
                    });
                    let parsed = gradecraft_core::csv::parse_students(text);
                    ui.label(RichText::new(format!("Vorschau: {} Schüler erkannt", parsed.len())).strong());
                    egui::ScrollArea::vertical().id_salt("prev").max_height(140.0).show(ui, |ui| {
                        for s in parsed.iter().take(50) {
                            ui.label(format!("{}, {}  ({})", s.last_name, s.first_name, s.gender.short()));
                        }
                    });
                    if buttons(ui, &format!("{} Schüler übernehmen", parsed.len()), !parsed.is_empty()) {
                        self.checkpoint();
                        let ids: Vec<Id> = parsed.iter().map(|_| self.db.alloc_id()).collect();
                        if let Some(c) = self.db.class_mut(*class) {
                            for (s, id) in parsed.into_iter().zip(ids) {
                                c.students.push(gradecraft_core::Student { id, last_name: s.last_name, first_name: s.first_name, gender: s.gender });
                            }
                        }
                        self.touch(ctx);
                        outcome = Outcome::Close;
                    } else if cancelled(ui) {
                        outcome = Outcome::Close;
                    }
                }
                Dialog::SaveTemplate { course, name } => {
                    title(ui, ic::BOOKMARK_SIMPLE, "Als Vorlage speichern");
                    ui.label("Speichert Leistungsbereiche, Spalten, Gewichtungen und Notenskala (ohne Noten).");
                    ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                    if buttons(ui, "Speichern", !name.trim().is_empty()) {
                        if let Some(c) = self.db.course(*course).cloned() {
                            self.checkpoint();
                            let scheme = self.db.clone_scheme(&c.scheme);
                            let id = self.db.alloc_id();
                            self.db.templates.push(Template { id, name: name.trim().to_string(), scale: c.scale, scheme });
                            self.touch(ctx);
                            self.notify(ctx, "Vorlage gespeichert", false);
                        }
                        outcome = Outcome::Close;
                    } else if cancelled(ui) {
                        outcome = Outcome::Close;
                    }
                }
                Dialog::ApplyTemplate { course, template } => {
                    title(ui, ic::COPY_SIMPLE, "Vorlage anwenden");
                    for t in &self.db.templates {
                        ui.radio_value(template, Some(t.id), format!("{} ({})", t.name, t.scale.label()));
                    }
                    let n = self.db.course(*course).map(|c| c.grades.len()).unwrap_or(0);
                    if n > 0 {
                        ui.label(RichText::new(format!("{} Achtung: Das bisherige Schema wird ersetzt – die {n} eingetragenen Noten gehen dabei verloren.", ic::WARNING)).color(p.bad));
                    }
                    if buttons(ui, "Anwenden", template.is_some()) {
                        if let Some(t) = *template {
                            if n > 0 {
                                self.dialog = Some(Dialog::Confirm {
                                    title: "Vorlage anwenden".into(),
                                    text: format!("Schema ersetzen und {n} Noten löschen?"),
                                    action: ConfirmAction::ApplyTemplate(*course, t),
                                });
                                return;
                            }
                            self.apply_template(ctx, *course, t);
                        }
                        outcome = Outcome::Close;
                    } else if cancelled(ui) {
                        outcome = Outcome::Close;
                    }
                }
                Dialog::ChangePassword { old, new1, new2, error } => {
                    title(ui, ic::KEY, "Passwort ändern");
                    egui::Grid::new("pw").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                        ui.label("Aktuelles Passwort");
                        ui.add(egui::TextEdit::singleline(old).password(true));
                        ui.end_row();
                        ui.label("Neues Passwort");
                        ui.add(egui::TextEdit::singleline(new1).password(true));
                        ui.end_row();
                        ui.label("Wiederholen");
                        ui.add(egui::TextEdit::singleline(new2).password(true));
                        ui.end_row();
                    });
                    password_strength(ui, new1);
                    if let Some(e) = error {
                        ui.label(RichText::new(e.as_str()).color(p.bad));
                    }
                    if buttons(ui, "Ändern", !new1.is_empty()) {
                        if !self.vault.verify_password(old) {
                            *error = Some("Das aktuelle Passwort ist falsch.".into());
                        } else if new1 != new2 {
                            *error = Some("Die neuen Passwörter stimmen nicht überein.".into());
                        } else if new1.chars().count() < 8 {
                            *error = Some("Das Passwort muss mindestens 8 Zeichen haben.".into());
                        } else {
                            match self.vault.change_password(new1, &self.db) {
                                Ok(()) => {
                                    self.dirty = false;
                                    self.notify(ctx, "Passwort geändert", false);
                                    outcome = Outcome::Close;
                                }
                                Err(e) => *error = Some(e.to_string()),
                            }
                        }
                    } else if cancelled(ui) {
                        outcome = Outcome::Close;
                    }
                }
                Dialog::NewYear { name, carry, with_courses } => {
                    title(ui, ic::CALENDAR_PLUS, "Neues Schuljahr");
                    ui.horizontal(|ui| {
                        ui.label("Bezeichnung");
                        ui.add(egui::TextEdit::singleline(name).desired_width(120.0));
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("Klassen übernehmen (mit Schülern, neue Bezeichnung möglich):").strong());
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for (id, on, new_name) in carry.iter_mut() {
                            let old = self.db.class(*id).map(|c| c.name.clone()).unwrap_or_default();
                            ui.horizontal(|ui| {
                                ui.checkbox(on, format!("{old}  →"));
                                ui.add_enabled(*on, egui::TextEdit::singleline(new_name).desired_width(100.0));
                            });
                        }
                    });
                    ui.checkbox(with_courses, "Fächer und Bewertungsschemata mit übernehmen (ohne Noten)");
                    if buttons(ui, "Schuljahr anlegen", !name.trim().is_empty()) {
                        self.checkpoint();
                        let list: Vec<(Id, String)> = carry.iter().filter(|(_, on, _)| *on).map(|(id, _, n)| (*id, n.trim().to_string())).collect();
                        let y = self.db.new_school_year(name.trim(), &list, *with_courses);
                        self.year = y;
                        self.view = View::Welcome;
                        self.touch(ctx);
                        outcome = Outcome::Close;
                    } else if cancelled(ui) {
                        outcome = Outcome::Close;
                    }
                }
                Dialog::Confirm { title: t, text, action } => {
                    title(ui, ic::WARNING, t);
                    ui.label(text.as_str());
                    ui.label(RichText::new("Kann mit „Rückgängig“ (Strg+Z) wiederhergestellt werden.").size(12.0).color(p.text_weak));
                    if buttons(ui, "Löschen", true) {
                        self.run_confirm(ctx, action);
                        outcome = Outcome::Close;
                    } else if cancelled(ui) {
                        outcome = Outcome::Close;
                    }
                }
                Dialog::Message { title: t, text } => {
                    title(ui, ic::INFO, t);
                    ui.label(text.as_str());
                    if primary_button(ui, "OK").clicked() {
                        outcome = Outcome::Close;
                    }
                }
                Dialog::UnsavedOnExit => {
                    title(ui, ic::FLOPPY_DISK, "Änderungen speichern?");
                    ui.label("Es gibt ungespeicherte Änderungen.");
                    ui.horizontal(|ui| {
                        if primary_button(ui, "Speichern und beenden").clicked() && self.save(ctx) {
                            self.close_after_save = true;
                            outcome = Outcome::Close;
                        }
                        if secondary_button(ui, "Verwerfen").clicked() {
                            self.dirty = false;
                            self.close_after_save = true;
                            outcome = Outcome::Close;
                        }
                        if secondary_button(ui, "Abbrechen").clicked() {
                            outcome = Outcome::Close;
                        }
                    });
                }
                Dialog::InputHelp => {
                    title(ui, ic::KEYBOARD, "Noteneingabe");
                    egui::Grid::new("help").num_columns(2).spacing([16.0, 6.0]).striped(true).show(ui, |ui| {
                        for (k, v) in [
                            ("1 … 6", "Note (Noten-Skala)"),
                            ("2+  /  2-", "Tendenz: ±⅓ Note (2+ = 1,67; 2- = 2,33)"),
                            ("2-3", "Zwischennote (2,5)"),
                            ("0 … 15", "Punkte (Punkte-Skala)"),
                            ("6 / 0", "ungenügend wegen schlechter Leistung"),
                            ("T", "ungenügend wegen Täuschung (zählt als 6 bzw. 0 P)"),
                            ("U", "ungenügend wegen unentschuldigter Nichtteilnahme"),
                            ("F  oder  -", "fehlt / entschuldigt – zählt nicht zum Schnitt"),
                            ("Enter / Tab", "speichern und nach unten / rechts"),
                            ("Pfeiltasten", "Zelle wechseln"),
                            ("F2 / Doppelklick", "Zelle bearbeiten"),
                            ("Entf", "Eintrag löschen"),
                            ("Strg+Z / Strg+Y", "Rückgängig / Wiederholen"),
                            ("Strg+S / Strg+L", "Speichern / Sperren"),
                        ] {
                            ui.label(RichText::new(k).strong());
                            ui.label(v);
                            ui.end_row();
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("Rundung: kaufmännisch (2,50 → 3). Bereiche ohne Noten werden bei der Gewichtung nicht berücksichtigt.").size(12.0).color(p.text_weak));
                    if primary_button(ui, "OK").clicked() {
                        outcome = Outcome::Close;
                    }
                }
                Dialog::Privacy => {
                    title(ui, ic::SHIELD_CHECK, "Datenschutz");
                    for line in [
                        "Alle Daten bleiben auf deinem Gerät – GradeCraft überträgt keine Schüler- oder Notendaten.",
                        "Einzige Netzwerkverbindung: die (abschaltbare) Suche nach Updates bei GitHub, ohne Datenübertragung.",
                        "Die Notendatei ist mit AES-256-GCM verschlüsselt; der Schlüssel wird mit Argon2id aus deinem Passwort abgeleitet.",
                        "Ohne Passwort sind die Daten nicht lesbar – auch nicht für uns. Bitte Passwort sicher aufbewahren!",
                        "Sicherungskopien werden ebenfalls verschlüsselt im Unterordner „Sicherungen“ abgelegt.",
                        "Nach Inaktivität sperrt sich GradeCraft automatisch (einstellbar unter Datei → Optionen).",
                        "Exporte (CSV) sind NICHT verschlüsselt – bitte entsprechend sorgfältig speichern und löschen.",
                    ] {
                        ui.label(format!("•  {line}"));
                    }
                    if primary_button(ui, "OK").clicked() {
                        outcome = Outcome::Close;
                    }
                }
                Dialog::About => {
                    title(ui, ic::GRADUATION_CAP, "Über GradeCraft");
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.label("Notenverwaltung für Lehrkräfte – lokal, verschlüsselt, in Rust geschrieben.");
                    ui.hyperlink_to("Projektseite auf GitHub", format!("https://github.com/{}", crate::updater::UPDATE_REPO));
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "GradeCraft ist freie Software: Du kannst sie unter den Bedingungen der GNU General Public License, \
                             Version 3 oder (nach deiner Wahl) jeder späteren Version, weitergeben und/oder verändern. \
                             GradeCraft wird OHNE JEDE GEWÄHRLEISTUNG bereitgestellt.",
                        )
                        .size(12.0)
                        .color(p.text_weak),
                    );
                    ui.hyperlink_to("Lizenztext (GPL-3.0)", "https://www.gnu.org/licenses/gpl-3.0.html");
                    ui.label(RichText::new("Schrift in PDF-Dokumenten: Inter (SIL Open Font License 1.1). Icons: Phosphor (MIT).").size(12.0).color(p.text_weak));
                    if primary_button(ui, "OK").clicked() {
                        outcome = Outcome::Close;
                    }
                }
            }
        });
        let _ = modal;
        if esc && !matches!(dialog, Dialog::UnsavedOnExit) {
            outcome = Outcome::Close;
        }
        if matches!(outcome, Outcome::Keep) && self.dialog.is_none() {
            self.dialog = Some(dialog);
        }
    }

    fn apply_template(&mut self, ctx: &egui::Context, course: Id, template: Id) {
        let Some(t) = self.db.templates.iter().find(|t| t.id == template).cloned() else {
            return;
        };
        self.checkpoint();
        let scheme = self.db.clone_scheme(&t.scheme);
        if let Some(c) = self.db.course_mut(course) {
            c.scheme = scheme;
            c.scale = t.scale;
            c.grades.clear();
            c.finals.clear();
        }
        self.touch(ctx);
        self.notify(ctx, "Vorlage angewendet", false);
    }

    fn run_confirm(&mut self, ctx: &egui::Context, action: &ConfirmAction) {
        if let ConfirmAction::ApplyTemplate(c, t) = action {
            self.apply_template(ctx, *c, *t);
            return;
        }
        self.checkpoint();
        match *action {
            ConfirmAction::DeleteClass(c) => self.db.delete_class(c),
            ConfirmAction::DeleteCourse(c) => self.db.delete_course(c),
            ConfirmAction::DeleteStudent(c, s) => self.db.delete_student(c, s),
            ConfirmAction::DeleteArea(c, a) => {
                if let Some(course) = self.db.course_mut(c) {
                    let cols: Vec<Id> = course
                        .scheme
                        .areas
                        .iter()
                        .filter(|x| x.id == a)
                        .flat_map(|x| x.columns.iter().map(|c| c.id))
                        .collect();
                    course.scheme.areas.retain(|x| x.id != a);
                    course.grades.retain(|g| !cols.contains(&g.column));
                }
            }
            ConfirmAction::DeleteColumn(c, col) => {
                if let Some(course) = self.db.course_mut(c) {
                    for a in &mut course.scheme.areas {
                        a.columns.retain(|x| x.id != col);
                    }
                    course.grades.retain(|g| g.column != col);
                }
            }
            ConfirmAction::DeleteTemplate(t) => self.db.templates.retain(|x| x.id != t),
            ConfirmAction::DeleteYear(y) => {
                let classes: Vec<Id> = self.db.classes_of_year(y).map(|c| c.id).collect();
                for c in classes {
                    self.db.delete_class(c);
                }
                self.db.school_years.retain(|x| x.id != y);
            }
            ConfirmAction::ApplyTemplate(..) => unreachable!(),
        }
        self.touch(ctx);
        self.fix_view();
    }
}

fn title(ui: &mut egui::Ui, icon: &str, text: &str) {
    let p = Palette::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(RichText::new(icon).size(22.0).color(p.accent_text));
        ui.label(RichText::new(text).font(theme::bold(18.0)));
    });
    ui.add_space(8.0);
}

/// OK/Abbrechen-Zeile. Gibt `true` zurück, wenn OK geklickt wurde.
fn buttons(ui: &mut egui::Ui, ok: &str, enabled: bool) -> bool {
    ui.add_space(10.0);
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.add_enabled_ui(enabled, |ui| {
            clicked = primary_button(ui, ok).clicked();
        });
        if secondary_button(ui, "Abbrechen").clicked() {
            ui.memory_mut(|m| m.data.insert_temp(egui::Id::new("dlg-cancel"), true));
        }
    });
    clicked
}

fn cancelled(ui: &mut egui::Ui) -> bool {
    ui.memory_mut(|m| {
        m.data
            .remove_temp::<bool>(egui::Id::new("dlg-cancel"))
            .unwrap_or(false)
    })
}

fn column_form(
    ui: &mut egui::Ui,
    name: &mut String,
    weight: &mut f64,
    half: &mut Half,
    area: Option<(&mut Id, &Vec<(Id, String)>)>,
) {
    egui::Grid::new("colform")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            if let Some((area, areas)) = area {
                ui.label("Leistungsbereich");
                egui::ComboBox::from_id_salt("area")
                    .selected_text(
                        areas
                            .iter()
                            .find(|(id, _)| id == area)
                            .map(|(_, n)| n.as_str())
                            .unwrap_or(""),
                    )
                    .width(240.0)
                    .show_ui(ui, |ui| {
                        for (id, n) in areas {
                            ui.selectable_value(area, *id, n);
                        }
                    });
                ui.end_row();
            }
            ui.label("Bezeichnung");
            ui.add(
                egui::TextEdit::singleline(name)
                    .hint_text("z. B. KA 3, Referat, Vokabeltest")
                    .desired_width(240.0),
            );
            ui.end_row();
            ui.label("Wertigkeit");
            ui.horizontal(|ui| {
                weight_edit(ui, weight);
                ui.label(RichText::new("(1 = normal, 2 = doppelt)").size(12.0));
            });
            ui.end_row();
            ui.label("Halbjahr");
            ui.horizontal(|ui| {
                ui.radio_value(half, Half::H1, "1. Halbjahr");
                ui.radio_value(half, Half::H2, "2. Halbjahr");
            });
            ui.end_row();
        });
}

/// Einfache Passwortstärke-Anzeige.
pub fn password_strength(ui: &mut egui::Ui, pw: &str) {
    if pw.is_empty() {
        return;
    }
    let p = Palette::get(ui.ctx());
    let mut score = 0;
    if pw.chars().count() >= 8 {
        score += 1;
    }
    if pw.chars().count() >= 12 {
        score += 1;
    }
    if pw.chars().any(|c| c.is_ascii_digit()) {
        score += 1;
    }
    if pw.chars().any(|c| c.is_uppercase()) && pw.chars().any(|c| c.is_lowercase()) {
        score += 1;
    }
    if pw.chars().any(|c| !c.is_alphanumeric()) {
        score += 1;
    }
    let (txt, col) = match score {
        0..=1 => ("schwach", p.bad),
        2..=3 => ("mittel", p.warn),
        _ => ("stark", p.good),
    };
    ui.label(
        RichText::new(format!("Passwortstärke: {txt}"))
            .color(col)
            .size(12.0),
    );
}

/// Liest UTF-8 (mit/ohne BOM) oder Windows-1252 (ältere Excel-CSV).
pub fn decode_text(b: &[u8]) -> String {
    match std::str::from_utf8(b) {
        Ok(s) => s.trim_start_matches('\u{feff}').to_string(),
        Err(_) => b
            .iter()
            .map(|&c| match c {
                0x80 => '€',
                0x8A => 'Š',
                0x9A => 'š',
                0x8E => 'Ž',
                0x9E => 'ž',
                0x96 => '–',
                0x97 => '—',
                c => char::from(c), // Latin-1 deckt ÄÖÜäöüß ab
            })
            .collect(),
    }
}
