//! Eine geöffnete Notendatei: Zustand, Speichern, Rückgängig, Navigation.

use eframe::egui;
use gradecraft_core::store::Vault;
use gradecraft_core::{Database, Id, Period};

use crate::config::AppConfig;
use crate::dialogs::Dialog;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RibbonTab {
    Start,
    Klasse,
    Bewertung,
    Berichte,
    Ansicht,
    Hilfe,
}

impl RibbonTab {
    pub const ALL: [RibbonTab; 6] = [
        RibbonTab::Start,
        RibbonTab::Klasse,
        RibbonTab::Bewertung,
        RibbonTab::Berichte,
        RibbonTab::Ansicht,
        RibbonTab::Hilfe,
    ];
    pub fn label(self) -> &'static str {
        match self {
            RibbonTab::Start => "Start",
            RibbonTab::Klasse => "Klasse",
            RibbonTab::Bewertung => "Bewertung",
            RibbonTab::Berichte => "Berichte",
            RibbonTab::Ansicht => "Ansicht",
            RibbonTab::Hilfe => "Hilfe",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Welcome,
    Class(Id),
    Course(Id),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CourseTab {
    H1,
    H2,
    /// Stundenerfassung (mündliche Mitarbeit)
    Oral,
    Schema,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackstagePage {
    Info,
    SaveAs,
    Backup,
    Export,
    Years,
    Templates,
    Settings,
}

/// Auswahl und Bearbeitung in der Notentabelle.
#[derive(Default)]
pub struct GridState {
    /// (Zeile, bearbeitbare Spalte)
    pub sel: Option<(usize, usize)>,
    /// Text der gerade bearbeiteten Zelle
    pub edit: Option<crate::grid::EditCell>,
    pub edit_started: bool,
    pub error: Option<String>,
    pub scroll_to_sel: bool,
    /// Umbenennen im Tabellenkopf
    pub head_edit: Option<crate::grid::HeadEdit>,
}

pub enum SessionEvent {
    None,
    Lock,
    Close,
}

pub struct Session {
    pub vault: Vault,
    pub db: Database,
    pub dirty: bool,
    pub last_change: f64,
    pub undo: Vec<Database>,
    pub redo: Vec<Database>,
    pub tab: RibbonTab,
    pub backstage: Option<BackstagePage>,
    pub year: Id,
    pub view: View,
    pub course_tab: CourseTab,
    pub grid: GridState,
    pub dialog: Option<Dialog>,
    pub search: String,
    pub status: Option<(String, f64, bool)>,
    pub last_activity: f64,
    pub event: SessionEvent,
    pub close_after_save: bool,
    /// Datum der Stundenerfassung
    pub oral_date: chrono::NaiveDate,
    /// Offene Notenspiegel-Fenster: (Fach, Leistung)
    pub dist_windows: Vec<(Id, Id)>,
    /// Geöffnete Notenbesprechung (eigenes Fenster)
    pub review: Option<crate::review::Review>,
    /// Noten manuell verborgen (Sichtschutz)
    pub privacy_hidden: bool,
    /// Bitte an die App: jetzt nach Updates suchen.
    pub request_update_check: bool,
}

const UNDO_LIMIT: usize = 100;

impl Session {
    pub fn new(vault: Vault, db: Database) -> Self {
        let year = db
            .school_years
            .iter()
            .rev()
            .find(|y| !y.archived)
            .or(db.school_years.last())
            .map(|y| y.id)
            .unwrap_or(0);
        Session {
            vault,
            db,
            dirty: false,
            last_change: 0.0,
            undo: Vec::new(),
            redo: Vec::new(),
            tab: RibbonTab::Start,
            backstage: None,
            year,
            view: View::Welcome,
            course_tab: CourseTab::H1,
            grid: GridState::default(),
            dialog: None,
            search: String::new(),
            status: None,
            last_activity: 0.0,
            event: SessionEvent::None,
            close_after_save: false,
            request_update_check: false,
            oral_date: crate::oral::today(),
            dist_windows: Vec::new(),
            privacy_hidden: false,
            review: None,
        }
    }

    /// Vor jeder Änderung aufrufen: legt einen Rückgängig-Punkt an.
    pub fn checkpoint(&mut self) {
        self.undo.push(self.db.clone());
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Nach jeder Änderung aufrufen.
    pub fn touch(&mut self, ctx: &egui::Context) {
        self.dirty = true;
        self.last_change = ctx.input(|i| i.time);
    }

    pub fn undo(&mut self, ctx: &egui::Context) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.db, prev));
            self.fix_view();
            self.touch(ctx);
            self.notify(ctx, "Rückgängig gemacht", false);
        }
    }

    pub fn redo(&mut self, ctx: &egui::Context) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.db, next));
            self.fix_view();
            self.touch(ctx);
            self.notify(ctx, "Wiederhergestellt", false);
        }
    }

    /// Ansicht zurücksetzen, falls das gewählte Objekt nicht mehr existiert.
    pub fn fix_view(&mut self) {
        let ok = match self.view {
            View::Welcome => true,
            View::Class(c) => self.db.class(c).is_some(),
            View::Course(c) => self.db.course(c).is_some(),
        };
        if !ok {
            self.view = View::Welcome;
        }
        if self.db.year(self.year).is_none() {
            self.year = self.db.school_years.last().map(|y| y.id).unwrap_or(0);
        }
        self.grid = GridState::default();
    }

    pub fn save(&mut self, ctx: &egui::Context) -> bool {
        match self.vault.save(&self.db) {
            Ok(()) => {
                self.dirty = false;
                true
            }
            Err(e) => {
                self.notify(ctx, &format!("Speichern fehlgeschlagen: {e}"), true);
                false
            }
        }
    }

    pub fn notify(&mut self, ctx: &egui::Context, text: &str, error: bool) {
        self.status = Some((text.to_string(), ctx.input(|i| i.time), error));
    }

    pub fn is_readonly_class(&self, class: Id) -> bool {
        self.db.is_class_archived(class)
    }

    pub fn year_archived(&self) -> bool {
        self.db.year(self.year).is_some_and(|y| y.archived)
    }

    /// Klasse der aktuellen Ansicht.
    pub fn current_class(&self) -> Option<Id> {
        match self.view {
            View::Class(c) => Some(c),
            View::Course(c) => self.db.course(c).map(|c| c.class_id),
            View::Welcome => None,
        }
    }

    pub fn current_course(&self) -> Option<Id> {
        match self.view {
            View::Course(c) => Some(c),
            _ => None,
        }
    }

    pub fn readonly(&self) -> bool {
        self.current_class()
            .is_some_and(|c| self.is_readonly_class(c))
            || self.year_archived()
    }

    pub fn open_view(&mut self, view: View) {
        if self.view != view {
            self.grid = GridState::default();
        }
        if let Some(year) = match view {
            View::Class(c) => self.db.class(c).map(|c| c.year_id),
            View::Course(c) => self
                .db
                .course(c)
                .and_then(|c| self.db.class(c.class_id))
                .map(|c| c.year_id),
            View::Welcome => None,
        } {
            self.year = year;
        }
        self.view = view;
        self.backstage = None;
    }

    /// Wird einmal pro Frame aufgerufen (Autospeichern, Sperre, Tastenkürzel).
    pub fn tick(&mut self, ctx: &egui::Context, cfg: &AppConfig) {
        let now = ctx.input(|i| i.time);
        let active = ctx.input(|i| !i.events.is_empty() || i.pointer.is_moving());
        if active || self.last_activity == 0.0 {
            self.last_activity = now;
        }

        if self.dirty && cfg.autosave && now - self.last_change > 1.0 {
            self.save(ctx);
        }
        if self.dirty && cfg.autosave {
            ctx.request_repaint_after(std::time::Duration::from_millis(1100));
        }

        let lock_after = self.db.settings.auto_lock_minutes;
        if lock_after > 0 {
            if now - self.last_activity > f64::from(lock_after) * 60.0 {
                self.event = SessionEvent::Lock;
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(10));
        }

        let nothing_focused = ctx.memory(|m| m.focused().is_none());
        let (save, undo, redo, lock) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::S),
                nothing_focused && i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z),
                nothing_focused
                    && (i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
                        || i.consume_key(
                            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                            egui::Key::Z,
                        )),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::L),
            )
        });
        if save && self.save(ctx) {
            self.notify(ctx, "Gespeichert", false);
        }
        if undo && self.dialog.is_none() {
            self.undo(ctx);
        }
        if redo && self.dialog.is_none() {
            self.redo(ctx);
        }
        if lock {
            self.event = SessionEvent::Lock;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F9)) {
            self.privacy_hidden = !self.privacy_hidden;
        }
    }

    pub fn student_name(&self, s: &gradecraft_core::Student, first_name_first: bool) -> String {
        if first_name_first && !s.first_name.is_empty() {
            format!("{} {}", s.first_name, s.last_name)
        } else {
            s.display_name()
        }
    }

    /// Bestätigt alle offenen Vorschläge des Zeitraums im aktuellen Fach.
    pub fn confirm_all(&mut self, ctx: &egui::Context, course_id: Id, period: Period) {
        let Some(course) = self.db.course(course_id) else {
            return;
        };
        let Some(class) = self.db.class(course.class_id) else {
            return;
        };
        let todo: Vec<(Id, f64)> = class
            .students
            .iter()
            .filter(|s| course.final_grade(s.id, period).is_none())
            .filter_map(|s| {
                gradecraft_core::calc::suggestion(course, s.id, period).map(|g| (s.id, g))
            })
            .collect();
        if todo.is_empty() {
            self.notify(ctx, "Keine offenen Vorschläge", false);
            return;
        }
        self.checkpoint();
        let n = todo.len();
        if let Some(course) = self.db.course_mut(course_id) {
            for (student, grade) in todo {
                course.set_final(gradecraft_core::FinalRecord {
                    student,
                    period,
                    grade,
                    suggestion: Some(grade),
                    reason: String::new(),
                });
            }
        }
        self.touch(ctx);
        self.notify(ctx, &format!("{n} Vorschläge bestätigt"), false);
    }
}
