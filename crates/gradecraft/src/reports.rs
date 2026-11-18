//! PDF-Berichte erzeugen, speichern und öffnen.

use eframe::egui;
use gradecraft_core::pdf;
use gradecraft_core::{Id, Period};

use crate::session::Session;

#[derive(Clone, Copy, Debug)]
pub enum PdfKind {
    /// Notenliste eines Fachs
    CourseList(Id),
    /// Konferenzübersicht einer Klasse
    Conference(Id, Period),
    /// Schülerbögen: Klasse, optional ein Schüler, optional ein Fach
    Sheets {
        class: Id,
        student: Option<Id>,
        course: Option<Id>,
    },
    /// Notenspiegel eines Fachs, optional nur eine Leistung
    Distribution(Id, Option<Id>),
}

/// Zeichen, die in Dateinamen nicht erlaubt sind, ersetzen.
fn safe(s: &str) -> String {
    s.chars()
        .map(|c| if "/\\:*?\"<>| ".contains(c) { '_' } else { c })
        .collect()
}

impl Session {
    fn pdf_filename(&self, kind: PdfKind) -> String {
        let course_label = |id: Id| {
            self.db.course(id).map(|c| {
                let k = self
                    .db
                    .class(c.class_id)
                    .map(|k| k.name.clone())
                    .unwrap_or_default();
                format!(
                    "{}_{}",
                    k,
                    if c.short.is_empty() {
                        &c.subject
                    } else {
                        &c.short
                    }
                )
            })
        };
        let class_name = |id: Id| {
            self.db
                .class(id)
                .map(|c| c.name.clone())
                .unwrap_or_default()
        };
        let name = match kind {
            PdfKind::CourseList(c) => format!("Notenliste_{}", course_label(c).unwrap_or_default()),
            PdfKind::Conference(c, p) => format!(
                "Konferenz_{}_{}",
                class_name(c),
                if p == Period::Halbjahr {
                    "Halbjahr"
                } else {
                    "Schuljahr"
                }
            ),
            PdfKind::Sheets { class, student, .. } => match student.and_then(|s| {
                self.db
                    .class(class)?
                    .students
                    .iter()
                    .find(|x| x.id == s)
                    .cloned()
            }) {
                Some(s) => format!(
                    "Schuelerbogen_{}_{}_{}",
                    class_name(class),
                    s.last_name,
                    s.first_name
                ),
                None => format!("Schuelerboegen_{}", class_name(class)),
            },
            PdfKind::Distribution(c, col) => {
                let col_name = col
                    .and_then(|id| {
                        self.db
                            .course(c)?
                            .find_column(id)
                            .map(|(_, x)| x.name.clone())
                    })
                    .map(|n| format!("_{n}"))
                    .unwrap_or_default();
                format!(
                    "Notenspiegel_{}{}",
                    course_label(c).unwrap_or_default(),
                    col_name
                )
            }
        };
        format!("{}.pdf", safe(&name))
    }

    /// Erzeugt das PDF, fragt nach dem Speicherort und öffnet es anschließend.
    pub fn export_pdf(&mut self, ctx: &egui::Context, kind: PdfKind) {
        let header = pdf::Header::from_settings(&self.db.settings);
        let result = match kind {
            PdfKind::CourseList(c) => pdf::course_list(&self.db, c, &header),
            PdfKind::Conference(c, p) => pdf::conference(&self.db, c, p, &header),
            PdfKind::Sheets {
                class,
                student,
                course,
            } => pdf::student_sheets(&self.db, class, student, course, &header),
            PdfKind::Distribution(c, col) => pdf::grade_distribution(&self.db, c, col, &header),
        };
        let bytes = match result {
            Ok(b) => b,
            Err(e) => {
                self.notify(ctx, &format!("PDF nicht möglich: {e}"), true);
                return;
            }
        };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(self.pdf_filename(kind))
            .add_filter("PDF-Dokument", &["pdf"])
            .save_file()
        else {
            return;
        };
        let path = if path.extension().is_none() {
            path.with_extension("pdf")
        } else {
            path
        };
        match std::fs::write(&path, bytes) {
            Ok(()) => {
                self.notify(ctx, &format!("PDF gespeichert: {}", path.display()), false);
                open_file(&path);
            }
            Err(e) => self.notify(
                ctx,
                &format!("PDF konnte nicht gespeichert werden: {e}"),
                true,
            ),
        }
    }
}

/// Öffnet eine Datei mit dem Standardprogramm des Systems.
pub fn open_file(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}
