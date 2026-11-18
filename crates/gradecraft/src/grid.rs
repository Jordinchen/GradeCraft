//! Notentabelle eines Fachs (Excel-ähnliche Eingabe).

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Key, Rect, RichText, Sense, Stroke, pos2, vec2,
};
use egui_phosphor::regular as ic;
use gradecraft_core::calc;
use gradecraft_core::{Course, Entry, FinalRecord, Half, Id, Period, UngenuegendGrund, YearMode};

use crate::config::AppConfig;
use crate::dialogs::Dialog;
use crate::session::{CourseTab, Session};
use crate::theme::{self, Palette};
use crate::widgets::{centered_text_rect, fmt_weight, weight_drag};

const ROW_H: f32 = 30.0;
/// Kopfzeilen: Leistungsbereich, Leistung, Wertigkeit
const HEAD_ROW1: f32 = 28.0;
const HEAD_ROW2: f32 = 26.0;
const HEAD_ROW3: f32 = 26.0;
const HEAD_H: f32 = HEAD_ROW1 + HEAD_ROW2 + HEAD_ROW3;

#[derive(Clone, Copy, PartialEq)]
enum GCol {
    Num,
    Name,
    /// `w`: Spaltenbreite, passend zur Beschriftung
    Grade {
        area: Id,
        col: Id,
        w: f32,
    },
    /// `w`: so breit, dass das Bereichsband (Name, Gewicht, %) hineinpasst
    AreaAvg {
        area: Id,
        w: f32,
    },
    HalfFinal,
    Total,
    Suggest,
    Final,
}

impl GCol {
    fn width(self) -> f32 {
        match self {
            GCol::Num => 34.0,
            GCol::Name => 210.0,
            GCol::Grade { w, .. } => w,
            GCol::AreaAvg { w, .. } => w,
            GCol::HalfFinal => 74.0,
            GCol::Total => 78.0,
            GCol::Suggest => 82.0,
            GCol::Final => 128.0,
        }
    }
    fn area(self) -> Option<Id> {
        match self {
            GCol::Grade { area, .. } | GCol::AreaAvg { area, .. } => Some(area),
            _ => None,
        }
    }
}

/// Was im Tabellenkopf umbenannt wird.
#[derive(Clone, Copy, PartialEq)]
pub enum HeadTarget {
    Area(Id),
    Column(Id),
}

/// Laufende Umbenennung im Tabellenkopf.
pub struct HeadEdit {
    pub target: HeadTarget,
    pub text: String,
    pub started: bool,
}

/// Änderungen aus dem Tabellenkopf (werden nach dem Zeichnen angewendet).
enum HeadChange {
    Checkpoint,
    AreaName(Id, String),
    ColumnName(Id, String),
    AreaWeight(Id, f64),
    ColumnWeight(Id, f64),
}

/// Zelle, die gerade bearbeitet wird.
pub struct EditCell {
    pub student: Id,
    pub column: Id,
    pub text: String,
    pub fresh: bool,
}

fn columns(ui: &egui::Ui, course: &Course, half: Half) -> Vec<GCol> {
    let mut cols = vec![GCol::Num, GCol::Name];
    let text_w = |text: String, font: FontId| {
        ui.painter()
            .layout_no_wrap(text, font, Color32::WHITE)
            .size()
            .x
    };
    for a in &course.scheme.areas {
        if area_hidden(course, a, half) {
            continue;
        }
        let mut band_w = 0.0;
        for c in a.columns.iter().filter(|c| c.half == half) {
            let w = (text_w(c.name.clone(), FontId::proportional(12.0)) + 18.0).clamp(66.0, 220.0);
            band_w += w;
            cols.push(GCol::Grade {
                area: a.id,
                col: c.id,
                w,
            });
        }
        // Bereichsband: Name + Gewichtsfeld (44) + „= 100 %“ + Abstände
        let needed = text_w(a.name.clone(), theme::bold(12.5))
            + 44.0
            + text_w("= 100 %".into(), FontId::proportional(12.0))
            + 36.0;
        let avg_w = (needed - band_w).max(70.0);
        cols.push(GCol::AreaAvg {
            area: a.id,
            w: avg_w,
        });
    }
    if half == Half::H2 && matches!(course.scheme.year_mode, YearMode::Halbjahre { .. }) {
        cols.push(GCol::HalfFinal);
    }
    cols.extend([GCol::Total, GCol::Suggest, GCol::Final]);
    cols
}

/// Bereiche ohne Leistungen in diesem Halbjahr werden ausgeblendet –
/// außer wenn die Schuljahresnote aus allen Einzelnoten gebildet wird.
fn area_hidden(course: &Course, area: &gradecraft_core::Area, half: Half) -> bool {
    let all_year = half == Half::H2 && course.scheme.year_mode == YearMode::AlleNoten;
    !all_year && !area.columns.iter().any(|c| c.half == half)
}

fn halves_for_avg(course: &Course, half: Half) -> Vec<Half> {
    if half == Half::H2 && course.scheme.year_mode == YearMode::AlleNoten {
        vec![Half::H1, Half::H2]
    } else {
        vec![half]
    }
}

impl Session {
    pub fn course_view(&mut self, ui: &mut egui::Ui, cfg: &AppConfig) {
        let Some(course_id) = self.current_course() else {
            return;
        };
        let Some(course) = self.db.course(course_id) else {
            return;
        };
        let Some(class) = self.db.class(course.class_id) else {
            return;
        };
        let p = Palette::get(ui.ctx());
        let title = format!("{} · Klasse {}", course.title(), class.name);
        let year = self
            .db
            .year(class.year_id)
            .map(|y| y.name.clone())
            .unwrap_or_default();
        let weights = weight_summary(course);

        ui.horizontal(|ui| {
            ui.label(RichText::new(ic::BOOK_OPEN).size(26.0).color(p.accent_text));
            ui.vertical(|ui| {
                ui.label(RichText::new(title).font(theme::bold(22.0)));
                ui.label(
                    RichText::new(format!(
                        "Schuljahr {year} · {} · {weights}",
                        course.scale.label()
                    ))
                    .color(p.text_weak),
                );
            });
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            for (tab, label) in [
                (CourseTab::H1, "1. Halbjahr"),
                (CourseTab::H2, "2. Halbjahr · Schuljahr"),
                (CourseTab::Oral, "Mündlich"),
                (CourseTab::Schema, "Bewertungsschema"),
            ] {
                if pill(ui, label, self.course_tab == tab).clicked() {
                    self.course_tab = tab;
                    self.grid = Default::default();
                }
            }
        });
        ui.add_space(8.0);

        match self.course_tab {
            CourseTab::H1 => self.grade_grid(ui, cfg, course_id, Half::H1),
            CourseTab::H2 => self.grade_grid(ui, cfg, course_id, Half::H2),
            CourseTab::Oral => self.oral_view(ui, cfg, course_id),
            CourseTab::Schema => self.scheme_editor(ui, course_id),
        }
    }

    fn grade_grid(&mut self, ui: &mut egui::Ui, cfg: &AppConfig, course_id: Id, half: Half) {
        let ctx = ui.ctx().clone();
        let p = Palette::get(&ctx);
        let ro = self.readonly();
        let course = self.db.course(course_id).unwrap().clone();
        let students = self
            .db
            .class(course.class_id)
            .map(|c| c.students.clone())
            .unwrap_or_default();
        let period = if half == Half::H1 {
            Period::Halbjahr
        } else {
            Period::Schuljahr
        };
        let cols = columns(ui, &course, half);
        let edit_cols: Vec<usize> = cols
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(c, GCol::Grade { .. }))
            .map(|(i, _)| i)
            .collect();
        let avg_halves = halves_for_avg(&course, half);

        if students.is_empty() {
            crate::widgets::card(ui, |ui| {
                ui.label("Diese Klasse hat noch keine Schüler.");
                if !ro
                    && ui
                        .button(format!("{} Schüler hinzufügen", ic::USER_PLUS))
                        .clicked()
                {
                    let c = course.class_id;
                    self.open_view(crate::session::View::Class(c));
                }
            });
            return;
        }
        if edit_cols.is_empty() {
            crate::widgets::card(ui, |ui| {
                ui.label(format!(
                    "Für das {} sind noch keine Leistungen (Spalten) angelegt.",
                    half.label()
                ));
                if !ro
                    && ui
                        .button(format!("{} Leistung hinzufügen", ic::LIST_PLUS))
                        .clicked()
                {
                    self.add_column_dialog(course_id, half);
                }
            });
            return;
        }

        // ---------- Tastatur ----------
        if self.dialog.is_none() {
            self.grid_keyboard(&ctx, &course, &students, &cols, &edit_cols, ro);
        }

        let total_w: f32 = cols.iter().map(|c| c.width()).sum();
        let stats_h = if cfg.show_stats { ROW_H } else { 0.0 };

        egui::Frame::new()
            .fill(p.sheet)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(6))
            .show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt(("grid-h", course_id, half == Half::H1))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                            self.grid_header(ui, &course, &cols, total_w, half, ro);
                            let max_h = (ui.available_height() - stats_h - 40.0).max(ROW_H * 3.0);
                            egui::ScrollArea::vertical()
                                .id_salt(("grid-v", course_id, half == Half::H1))
                                .max_height(max_h)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                                    for (row, s) in students.iter().enumerate() {
                                        self.grid_row(
                                            ui,
                                            cfg,
                                            &course,
                                            s,
                                            row,
                                            &cols,
                                            &edit_cols,
                                            &avg_halves,
                                            period,
                                            total_w,
                                            ro,
                                        );
                                    }
                                });
                            if cfg.show_stats
                                && let Some(col) = stats_row(
                                    ui,
                                    &course,
                                    &students,
                                    &cols,
                                    &avg_halves,
                                    period,
                                    total_w,
                                )
                            {
                                self.open_distribution(course_id, col);
                            }
                        });
                    });
            });

        ui.add_space(6.0);
        let hidden: Vec<(Id, String)> = course
            .scheme
            .areas
            .iter()
            .filter(|a| area_hidden(&course, a, half))
            .map(|a| (a.id, a.name.clone()))
            .collect();
        if !hidden.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("{} Ohne Leistungen im {}:", ic::INFO, half.label()))
                        .size(12.5)
                        .color(p.text_weak),
                );
                for (area, name) in hidden {
                    let btn = egui::Button::new(
                        RichText::new(format!("{} {name}", ic::PLUS))
                            .size(12.5)
                            .color(p.accent_text),
                    );
                    if !ro
                        && ui
                            .add(btn)
                            .on_hover_text("Leistung in diesem Bereich hinzufügen")
                            .clicked()
                    {
                        self.dialog = Some(Dialog::AddColumn {
                            course: course_id,
                            area,
                            name: "Leistung 1".into(),
                            weight: 1.0,
                            half,
                        });
                    }
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            let t = |s: &str| RichText::new(s).size(12.0).color(p.text_weak);
            ui.label(t(&format!("{} Eingabe: {}", ic::KEYBOARD, course.scale.input_hint())));
            ui.label(t("· T = ungenügend (Täuschung) · U = ungenügend (unentschuldigt) · F = fehlt (zählt nicht)"));
            ui.label(t("· Enter/Tab/Pfeiltasten zum Wechseln, Entf zum Löschen, Rechtsklick für mehr"));
        });
    }

    fn grid_keyboard(
        &mut self,
        ctx: &egui::Context,
        course: &Course,
        students: &[gradecraft_core::Student],
        cols: &[GCol],
        edit_cols: &[usize],
        ro: bool,
    ) {
        let n_rows = students.len();
        let n_cols = edit_cols.len();
        if let Some((r, c)) = self.grid.sel
            && (r >= n_rows || c >= n_cols)
        {
            self.grid.sel = Some((r.min(n_rows - 1), c.min(n_cols - 1)));
        }
        if self.grid.edit.is_some() {
            return; // Eingabe läuft über das Textfeld
        }
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let Some((r, c)) = self.grid.sel else { return };
        let mut new = (r as i64, c as i64);
        let mut start_edit: Option<String> = None;
        let mut clear = false;
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, Key::ArrowDown)
                || i.consume_key(egui::Modifiers::NONE, Key::Enter)
            {
                new.0 += 1;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::ArrowUp)
                || i.consume_key(egui::Modifiers::SHIFT, Key::Enter)
            {
                new.0 -= 1;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::ArrowRight)
                || i.consume_key(egui::Modifiers::NONE, Key::Tab)
            {
                new.1 += 1;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::ArrowLeft)
                || i.consume_key(egui::Modifiers::SHIFT, Key::Tab)
            {
                new.1 -= 1;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::F2) {
                start_edit = Some(String::new()); // vorhandenen Wert laden (unten)
            }
            if i.consume_key(egui::Modifiers::NONE, Key::Delete)
                || i.consume_key(egui::Modifiers::NONE, Key::Backspace)
            {
                clear = true;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::Escape) {
                self.grid.sel = None;
            }
            for e in &i.events {
                if let egui::Event::Text(t) = e
                    && !t.trim().is_empty()
                    && !i.modifiers.command
                {
                    start_edit = Some(t.clone());
                }
            }
        });
        if self.grid.sel.is_none() {
            return;
        }
        let new = (
            new.0.clamp(0, n_rows as i64 - 1) as usize,
            new.1.clamp(0, n_cols as i64 - 1) as usize,
        );
        if new != (r, c) {
            self.grid.sel = Some(new);
            self.grid.scroll_to_sel = true;
            return;
        }
        let GCol::Grade { col, .. } = cols[edit_cols[c]] else {
            return;
        };
        let student = students[r].id;
        if ro {
            return;
        }
        if clear {
            if course.grade(student, col).is_some() {
                self.set_cell(ctx, course.id, student, col, None);
            }
        } else if let Some(t) = start_edit {
            let (text, fresh) = if t.is_empty() {
                (
                    course
                        .grade(student, col)
                        .map(|g| g.entry.format(course.scale))
                        .unwrap_or_default(),
                    false,
                )
            } else {
                (t, true)
            };
            self.grid.edit = Some(EditCell {
                student,
                column: col,
                text,
                fresh,
            });
            self.grid.edit_started = true;
        }
    }

    fn set_cell(
        &mut self,
        ctx: &egui::Context,
        course: Id,
        student: Id,
        column: Id,
        entry: Option<Entry>,
    ) {
        let same = self
            .db
            .course(course)
            .map(|c| c.grade(student, column).map(|g| g.entry) == entry)
            .unwrap_or(true);
        if same {
            return;
        }
        self.checkpoint();
        if let Some(c) = self.db.course_mut(course) {
            c.set_grade(student, column, entry);
        }
        self.touch(ctx);
    }

    fn grid_header(
        &mut self,
        ui: &mut egui::Ui,
        course: &Course,
        cols: &[GCol],
        total_w: f32,
        half: Half,
        ro: bool,
    ) {
        let ctx = ui.ctx().clone();
        let p = Palette::get(&ctx);
        let (rect, _) = ui.allocate_exact_size(vec2(total_w, HEAD_H), Sense::hover());
        // Eigener Unterbereich: Eingabefelder (ui.put) dürfen den Layout-Cursor
        // der Tabelle nicht verschieben – sonst überlappen Kopf und Zeilen.
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt("grid-head"));
        let ui = &mut child;
        let painter = ui.painter_at(rect);
        painter.rect_filled(
            rect,
            CornerRadius {
                nw: 6,
                ne: 6,
                sw: 0,
                se: 0,
            },
            p.header,
        );
        let total_weight: f64 = course.scheme.areas.iter().map(|a| a.weight).sum();
        let y1 = rect.top() + HEAD_ROW1;
        let y2 = y1 + HEAD_ROW2;
        let mut changes: Vec<HeadChange> = Vec::new();
        let mut x = rect.left();
        let mut i = 0;
        let small = FontId::proportional(12.0);
        while i < cols.len() {
            let col = cols[i];
            if let Some(area_id) = col.area() {
                // Bereichsband über alle Spalten dieses Bereichs
                let start = x;
                let mut j = i;
                while j < cols.len() && cols[j].area() == Some(area_id) {
                    j += 1;
                }
                let band_w: f32 = cols[i..j].iter().map(|c| c.width()).sum();
                let area = course
                    .scheme
                    .areas
                    .iter()
                    .find(|a| a.id == area_id)
                    .unwrap();
                let pct = if total_weight > 0.0 {
                    area.weight / total_weight * 100.0
                } else {
                    0.0
                };
                let band = Rect::from_min_size(pos2(start, rect.top()), vec2(band_w, HEAD_ROW1));
                self.area_band(ui, band, area, pct, ro, &mut changes);
                painter.line_segment(
                    [pos2(start + 4.0, y1), pos2(start + band_w - 4.0, y1)],
                    Stroke::new(1.0, p.border),
                );
                painter.line_segment(
                    [pos2(start + 4.0, y2), pos2(start + band_w - 4.0, y2)],
                    Stroke::new(0.5, p.border),
                );

                for c in &cols[i..j] {
                    let r2 = Rect::from_min_size(pos2(x, y1), vec2(c.width(), HEAD_ROW2));
                    let r3 = Rect::from_min_size(pos2(x, y2), vec2(c.width(), HEAD_ROW3));
                    match *c {
                        GCol::Grade { col, .. } => {
                            let column = area.columns.iter().find(|cc| cc.id == col).unwrap();
                            self.column_head(ui, r2, course, column, ro, &mut changes);
                            // Wertigkeit der Leistung
                            let mut w = column.weight;
                            if ro {
                                painter.text(
                                    r3.center(),
                                    Align2::CENTER_CENTER,
                                    format!("×{}", fmt_weight(w)),
                                    small.clone(),
                                    p.text_weak,
                                );
                            } else {
                                let resp = ui
                                    .put(
                                        Rect::from_center_size(
                                            r3.center(),
                                            vec2((c.width() - 18.0).min(48.0), 20.0),
                                        ),
                                        weight_drag(&mut w).prefix("×"),
                                    )
                                    .on_hover_text(format!(
                                        "Wertigkeit von „{}“ (ziehen oder klicken und tippen)",
                                        column.name
                                    ));
                                if resp.drag_started() || resp.gained_focus() {
                                    changes.push(HeadChange::Checkpoint);
                                }
                                if resp.changed() {
                                    changes.push(HeadChange::ColumnWeight(col, w));
                                }
                            }
                        }
                        GCol::AreaAvg { .. } => {
                            let r = r2.union(r3);
                            painter.rect_filled(r, CornerRadius::ZERO, p.avg_bg);
                            let label = if half == Half::H2
                                && course.scheme.year_mode == YearMode::AlleNoten
                            {
                                "Ø Jahr"
                            } else {
                                "Ø"
                            };
                            painter.text(
                                r.center(),
                                Align2::CENTER_CENTER,
                                label,
                                theme::bold(12.0),
                                p.text,
                            );
                        }
                        _ => {}
                    }
                    x += c.width();
                }
                painter.line_segment(
                    [
                        pos2(start + band_w, rect.top()),
                        pos2(start + band_w, rect.bottom()),
                    ],
                    Stroke::new(1.0, p.border),
                );
                i = j;
                continue;
            }
            let r = Rect::from_min_size(pos2(x, rect.top()), vec2(col.width(), HEAD_H));
            let (label, bg) = match col {
                GCol::Num => ("Nr.".to_string(), None),
                GCol::Name => ("Name".to_string(), None),
                GCol::HalfFinal => ("Halbjahres-\nnote".to_string(), Some(p.avg_bg)),
                GCol::Total => (
                    (if half == Half::H1 {
                        "Ø Halbjahr"
                    } else {
                        "Ø Schuljahr"
                    })
                    .to_string(),
                    Some(p.avg_bg),
                ),
                GCol::Suggest => ("Vorschlag".to_string(), Some(p.avg_bg)),
                GCol::Final => (
                    (if half == Half::H1 {
                        "Halbjahresnote"
                    } else {
                        "Schuljahresnote"
                    })
                    .to_string(),
                    Some(p.final_bg),
                ),
                _ => (String::new(), None),
            };
            if let Some(bg) = bg {
                painter.rect_filled(r, CornerRadius::ZERO, bg);
            }
            if col == GCol::Name {
                let top = Rect::from_min_max(r.min, pos2(r.right(), y2));
                painter.text(
                    top.left_center() + vec2(8.0, 0.0),
                    Align2::LEFT_CENTER,
                    label,
                    theme::bold(12.5),
                    p.text,
                );
                // Beschriftung der Wertigkeitszeile
                painter.text(
                    pos2(r.right() - 8.0, y2 + HEAD_ROW3 / 2.0),
                    Align2::RIGHT_CENTER,
                    "Wertigkeit",
                    small.clone(),
                    p.text_weak,
                );
            } else {
                painter.text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    label,
                    theme::bold(12.5),
                    p.text,
                );
            }
            if col == GCol::Total {
                let tip = match (half, course.scheme.year_mode) {
                    (Half::H1, _) => {
                        "Gewichteter Durchschnitt der Leistungsbereiche im 1. Halbjahr".to_string()
                    }
                    (Half::H2, YearMode::AlleNoten) => {
                        "Alle Noten des Schuljahres, gewichtet nach Leistungsbereichen".to_string()
                    }
                    (Half::H2, YearMode::Halbjahre { w1, w2 }) => format!(
                        "Halbjahresnote (×{}) und Durchschnitt 2. Halbjahr (×{})",
                        fmt_weight(w1),
                        fmt_weight(w2)
                    ),
                };
                ui.interact(r, ui.id().with("tot-tip"), Sense::hover())
                    .on_hover_text(tip);
            }
            x += col.width();
            i += 1;
        }
        painter.line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(1.0, p.border),
        );
        self.apply_head_changes(&ctx, course.id, changes);
    }

    /// Bereichsband: Name (klicken zum Umbenennen), Gewicht und Anteil in %.
    fn area_band(
        &mut self,
        ui: &mut egui::Ui,
        band: Rect,
        area: &gradecraft_core::Area,
        pct: f64,
        ro: bool,
        changes: &mut Vec<HeadChange>,
    ) {
        let p = Palette::get(ui.ctx());
        let name_font = theme::bold(12.5);
        let pct_text = format!("= {pct:.0} %");
        let pct_w = ui
            .painter()
            .layout_no_wrap(pct_text.clone(), FontId::proportional(12.0), p.text)
            .size()
            .x;
        let weight_w = if ro { 0.0 } else { 44.0 };
        let name_w_full = ui
            .painter()
            .layout_no_wrap(area.name.clone(), name_font.clone(), p.text)
            .size()
            .x;
        let max_name_w = (band.width() - weight_w - pct_w - 30.0).max(40.0);
        let name_w = name_w_full.min(max_name_w);
        let total = name_w + 8.0 + weight_w + 6.0 + pct_w;
        let mut x = (band.center().x - total / 2.0).max(band.left() + 6.0);
        let cy = band.center().y;

        let name_rect =
            Rect::from_min_size(pos2(x, band.top() + 2.0), vec2(name_w, band.height() - 4.0));
        let editing = self
            .grid
            .head_edit
            .as_ref()
            .is_some_and(|e| e.target == HeadTarget::Area(area.id));
        if editing {
            let r = Rect::from_min_max(
                pos2(band.left() + 4.0, band.top()),
                pos2(band.right() - weight_w - pct_w - 14.0, band.bottom()),
            );
            self.head_text_edit(ui, r, name_font, changes);
        } else {
            let galley =
                ui.painter()
                    .layout(area.name.clone(), name_font, p.accent_text, name_w + 1.0);
            ui.painter_at(name_rect).galley(
                pos2(x, cy - galley.size().y.min(name_rect.height()) / 2.0),
                galley,
                p.accent_text,
            );
            let resp = ui.interact(
                name_rect,
                ui.id().with(("area-name", area.id)),
                Sense::CLICK,
            );
            if !ro {
                let resp = resp
                    .on_hover_text("Klicken zum Umbenennen")
                    .on_hover_cursor(egui::CursorIcon::Text);
                if resp.clicked() {
                    self.grid.head_edit = Some(HeadEdit {
                        target: HeadTarget::Area(area.id),
                        text: area.name.clone(),
                        started: true,
                    });
                }
            }
        }
        x += name_w + 8.0;
        if !ro {
            let mut w = area.weight;
            let resp = ui
                .put(
                    Rect::from_min_size(pos2(x, cy - 10.0), vec2(weight_w, 20.0)),
                    weight_drag(&mut w).prefix("×"),
                )
                .on_hover_text(format!(
                    "Gewichtung von „{}“ gegenüber den anderen Bereichen",
                    area.name
                ));
            if resp.drag_started() || resp.gained_focus() {
                changes.push(HeadChange::Checkpoint);
            }
            if resp.changed() {
                changes.push(HeadChange::AreaWeight(area.id, w));
            }
            x += weight_w + 6.0;
        }
        ui.painter().text(
            pos2(x, cy),
            Align2::LEFT_CENTER,
            pct_text,
            FontId::proportional(12.0),
            p.accent_text,
        );
    }

    /// Name einer Leistung im Kopf: klicken zum Umbenennen, Tooltip mit Statistik.
    fn column_head(
        &mut self,
        ui: &mut egui::Ui,
        r: Rect,
        course: &Course,
        column: &gradecraft_core::Column,
        ro: bool,
        changes: &mut Vec<HeadChange>,
    ) {
        let p = Palette::get(ui.ctx());
        let col = column.id;
        let editing = self
            .grid
            .head_edit
            .as_ref()
            .is_some_and(|e| e.target == HeadTarget::Column(col));
        if editing {
            self.head_text_edit(ui, r, FontId::proportional(12.0), changes);
            return;
        }
        ui.painter_at(r).text(
            r.center(),
            Align2::CENTER_CENTER,
            &column.name,
            FontId::proportional(12.0),
            p.text,
        );
        let resp = ui.interact(r, ui.id().with(("head", col)), Sense::CLICK);
        let (avg, n) = calc::column_stats(course, col);
        let dist = calc::distribution(course, col);
        let resp = resp.on_hover_ui(|ui| {
            ui.label(RichText::new(&column.name).strong());
            ui.label(format!("Wertigkeit: {}", fmt_weight(column.weight)));
            ui.label(format!(
                "Ø Klasse: {}  ({} Noten)",
                avg.map(|a| course.scale.format_avg(a))
                    .unwrap_or("–".into()),
                n
            ));
            ui.label(format!(
                "Notenspiegel: 1: {} · 2: {} · 3: {} · 4: {} · 5: {} · 6: {}",
                dist[0], dist[1], dist[2], dist[3], dist[4], dist[5]
            ));
            if !ro {
                ui.label(RichText::new("Klicken: umbenennen · Rechtsklick: mehr").size(11.0));
            }
        });
        if ro {
            return;
        }
        if resp.clicked() {
            self.grid.head_edit = Some(HeadEdit {
                target: HeadTarget::Column(col),
                text: column.name.clone(),
                started: true,
            });
        }
        let cid = course.id;
        resp.context_menu(|ui| {
            if ui
                .button(format!("{} Leistung bearbeiten …", ic::PENCIL_SIMPLE))
                .clicked()
            {
                self.dialog = Some(Dialog::EditColumn {
                    course: cid,
                    column: col,
                    name: column.name.clone(),
                    weight: column.weight,
                    half: column.half,
                });
            }
            if ui
                .button(format!("{} Leistung löschen", ic::TRASH))
                .clicked()
            {
                self.dialog = Some(Dialog::confirm_delete_column(&self.db, cid, col));
            }
            ui.separator();
            if ui
                .button(format!("{} Notenspiegel als PDF", ic::CHART_BAR))
                .clicked()
            {
                let ctx = ui.ctx().clone();
                self.export_pdf(&ctx, crate::reports::PdfKind::Distribution(cid, Some(col)));
            }
        });
    }

    /// Textfeld zum Umbenennen im Kopf. Enter/Klick woanders übernimmt, Esc bricht ab.
    fn head_text_edit(
        &mut self,
        ui: &mut egui::Ui,
        r: Rect,
        font: FontId,
        changes: &mut Vec<HeadChange>,
    ) {
        let p = Palette::get(ui.ctx());
        let id = egui::Id::new("grid-head-editor");
        let text_rect = centered_text_rect(ui, r, &font);
        let Some(edit) = self.grid.head_edit.as_mut() else {
            return;
        };
        ui.painter()
            .rect_filled(r.shrink(1.0), CornerRadius::same(3), p.sheet);
        let resp = ui.put(
            text_rect,
            egui::TextEdit::singleline(&mut edit.text)
                .id(id)
                .font(font)
                .horizontal_align(egui::Align::Center)
                .vertical_align(egui::Align::Center)
                .margin(egui::Margin::symmetric(2, 1))
                .frame(egui::Frame::NONE),
        );
        ui.painter().rect_stroke(
            r.shrink(1.0),
            CornerRadius::same(3),
            Stroke::new(1.5, p.accent_text),
            egui::StrokeKind::Inside,
        );
        if std::mem::take(&mut edit.started) {
            resp.request_focus();
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                let n = edit.text.chars().count();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(n),
                    )));
                state.store(ui.ctx(), id);
            }
            return;
        }
        if resp.lost_focus() {
            let esc = ui.input(|i| i.key_pressed(Key::Escape));
            let edit = self.grid.head_edit.take().unwrap();
            let name = edit.text.trim().to_string();
            if !esc && !name.is_empty() {
                changes.push(match edit.target {
                    HeadTarget::Area(a) => HeadChange::AreaName(a, name),
                    HeadTarget::Column(c) => HeadChange::ColumnName(c, name),
                });
            }
        }
    }

    fn apply_head_changes(&mut self, ctx: &egui::Context, course_id: Id, changes: Vec<HeadChange>) {
        if changes.is_empty() {
            return;
        }
        let mut modified = false;
        for ch in changes {
            if let HeadChange::Checkpoint = ch {
                self.checkpoint();
                continue;
            }
            // Umbenennen ist ein eigener Rückgängig-Schritt
            if matches!(ch, HeadChange::AreaName(..) | HeadChange::ColumnName(..)) {
                self.checkpoint();
            }
            let Some(c) = self.db.course_mut(course_id) else {
                return;
            };
            for a in &mut c.scheme.areas {
                match &ch {
                    HeadChange::AreaName(id, n) if a.id == *id => a.name = n.clone(),
                    HeadChange::AreaWeight(id, w) if a.id == *id => a.weight = *w,
                    _ => {}
                }
                for col in &mut a.columns {
                    match &ch {
                        HeadChange::ColumnName(id, n) if col.id == *id => col.name = n.clone(),
                        HeadChange::ColumnWeight(id, w) if col.id == *id => col.weight = *w,
                        _ => {}
                    }
                }
            }
            modified = true;
        }
        if modified {
            self.touch(ctx);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn grid_row(
        &mut self,
        ui: &mut egui::Ui,
        cfg: &AppConfig,
        course: &Course,
        s: &gradecraft_core::Student,
        row: usize,
        cols: &[GCol],
        edit_cols: &[usize],
        avg_halves: &[Half],
        period: Period,
        total_w: f32,
        ro: bool,
    ) {
        let ctx = ui.ctx().clone();
        let p = Palette::get(&ctx);
        let scale = course.scale;
        let (rect, _) = ui.allocate_exact_size(vec2(total_w, ROW_H), Sense::hover());
        // Eigener Unterbereich: Eingabefelder (ui.put) dürfen den Layout-Cursor
        // der Tabelle nicht verschieben – sonst überlappen Kopf und Zeilen.
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .id_salt(("grid-row", s.id)),
        );
        let ui = &mut child;
        if !ui.is_rect_visible(rect)
            && !(self.grid.scroll_to_sel && self.grid.sel.is_some_and(|(r, _)| r == row))
        {
            return;
        }
        let row_selected = self.grid.sel.is_some_and(|(r, _)| r == row);
        let bg = if row_selected {
            p.accent_soft.gamma_multiply(0.5)
        } else if row % 2 == 1 {
            p.stripe
        } else {
            p.sheet
        };
        ui.painter().rect_filled(rect, CornerRadius::ZERO, bg);
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(0.5, p.border),
        );

        let mut x = rect.left();
        for (ci, col) in cols.iter().enumerate() {
            let r = Rect::from_min_size(pos2(x, rect.top()), vec2(col.width(), ROW_H));
            x += col.width();
            let painter = ui.painter_at(r);
            match *col {
                GCol::Num => {
                    painter.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        format!("{}", row + 1),
                        FontId::proportional(12.0),
                        p.text_weak,
                    );
                }
                GCol::Name => {
                    let name = self.student_name(s, cfg.first_name_first);
                    painter.text(
                        r.left_center() + vec2(8.0, 0.0),
                        Align2::LEFT_CENTER,
                        name,
                        FontId::proportional(14.0),
                        p.text,
                    );
                    let resp = ui.interact(r, ui.id().with(("name", s.id)), Sense::CLICK);
                    if resp.clicked() {
                        self.grid.sel = Some((row, self.grid.sel.map(|(_, c)| c).unwrap_or(0)));
                    }
                }
                GCol::Grade { col: column, .. } => {
                    let ecol = edit_cols.iter().position(|&e| e == ci).unwrap();
                    let selected = self.grid.sel == Some((row, ecol));
                    let record = course.grade(s.id, column);
                    let editing = self
                        .grid
                        .edit
                        .as_ref()
                        .is_some_and(|e| e.student == s.id && e.column == column);
                    if selected && self.grid.scroll_to_sel {
                        ui.scroll_to_rect(r, None);
                        self.grid.scroll_to_sel = false;
                    }
                    if editing {
                        self.cell_editor(ui, r, course, s.id, column, row, ecol, edit_cols.len());
                    } else {
                        if let Some(g) = record {
                            let (text, color) = match g.entry {
                                Entry::Note(v) => (scale.format(v), p.grade_color(scale.level(v))),
                                Entry::Ungenuegend(_) => (g.entry.format(scale), p.bad),
                                Entry::Fehlt => ("f".to_string(), p.text_weak),
                            };
                            painter.text(
                                r.center(),
                                Align2::CENTER_CENTER,
                                text,
                                FontId::proportional(14.5),
                                color,
                            );
                            if !g.comment.is_empty() {
                                let tri = vec![
                                    r.right_top(),
                                    r.right_top() + vec2(-7.0, 0.0),
                                    r.right_top() + vec2(0.0, 7.0),
                                ];
                                painter.add(egui::Shape::convex_polygon(
                                    tri,
                                    p.accent_text,
                                    Stroke::NONE,
                                ));
                            }
                        } else if let Some((Entry::Note(v), true)) =
                            course.effective_entry(s.id, column)
                        {
                            // Automatisch aus Stundennoten – kursiv in Akzentfarbe
                            painter.text(
                                r.center(),
                                Align2::CENTER_CENTER,
                                scale.format_avg(v),
                                FontId::proportional(13.0),
                                p.accent_text,
                            );
                            painter.text(
                                r.right_top() + vec2(-3.0, 2.0),
                                Align2::RIGHT_TOP,
                                ic::CHAT_CIRCLE_TEXT,
                                FontId::proportional(9.0),
                                p.accent_text.gamma_multiply(0.7),
                            );
                        }
                        if selected {
                            painter.rect_stroke(
                                r.shrink(1.0),
                                CornerRadius::same(2),
                                Stroke::new(2.0, p.accent_text),
                                egui::StrokeKind::Inside,
                            );
                        }
                        let resp =
                            ui.interact(r, ui.id().with(("cell", s.id, column)), Sense::CLICK);
                        let resp = if let Some(g) = record {
                            let mut t = g.entry.describe(scale);
                            if !g.comment.is_empty() {
                                t.push_str(&format!("\n{} {}", ic::CHAT_TEXT, g.comment));
                            }
                            resp.on_hover_text(t)
                        } else if let Some(range) = course
                            .find_column(column)
                            .and_then(|(_, c)| c.oral)
                            .filter(|_| course.effective_entry(s.id, column).is_some())
                        {
                            resp.on_hover_text(format!(
                                "Automatisch aus {} Stundennoten ({}).\nEine eingetragene Note hat Vorrang.",
                                course.oral_in_range(s.id, range).count(),
                                range.label()
                            ))
                        } else {
                            resp
                        };
                        if resp.clicked() {
                            self.grid.sel = Some((row, ecol));
                        }
                        if resp.double_clicked() && !ro {
                            self.grid.edit = Some(EditCell {
                                student: s.id,
                                column,
                                text: record.map(|g| g.entry.format(scale)).unwrap_or_default(),
                                fresh: false,
                            });
                            self.grid.edit_started = true;
                        }
                        let cid = course.id;
                        let sid = s.id;
                        resp.context_menu(|ui| {
                            self.grid.sel = Some((row, ecol));
                            ui.set_min_width(260.0);
                            let w = scale.format(scale.worst());
                            let items: [(String, Option<Entry>); 4] = [
                                (
                                    format!("{} Fehlt (zählt nicht)", ic::MINUS_CIRCLE),
                                    Some(Entry::Fehlt),
                                ),
                                (
                                    format!("{} {w} – ungenügend wegen Täuschung", ic::WARNING),
                                    Some(Entry::Ungenuegend(UngenuegendGrund::Taeuschung)),
                                ),
                                (
                                    format!(
                                        "{} {w} – ungenügend wegen unentschuldigter Nichtteilnahme",
                                        ic::WARNING
                                    ),
                                    Some(Entry::Ungenuegend(UngenuegendGrund::Unentschuldigt)),
                                ),
                                (format!("{} Eintrag löschen", ic::TRASH), None),
                            ];
                            for (label, entry) in items {
                                if ui.add_enabled(!ro, egui::Button::new(label)).clicked() {
                                    self.set_cell(&ctx, cid, sid, column, entry);
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui
                                .add_enabled(
                                    !ro && record.is_some(),
                                    egui::Button::new(format!("{} Kommentar …", ic::CHAT_TEXT)),
                                )
                                .clicked()
                            {
                                self.dialog = Some(Dialog::Comment {
                                    course: cid,
                                    student: sid,
                                    column,
                                    text: record.map(|g| g.comment.clone()).unwrap_or_default(),
                                });
                                ui.close();
                            }
                        });
                    }
                }
                GCol::AreaAvg { area, .. } => {
                    painter.rect_filled(r, CornerRadius::ZERO, p.avg_bg.gamma_multiply(0.7));
                    if let Some(v) = calc::area_average(course, s.id, area, avg_halves) {
                        painter.text(
                            r.center(),
                            Align2::CENTER_CENTER,
                            scale.format_avg(v),
                            FontId::proportional(13.5),
                            p.grade_color(scale.level(v)),
                        );
                        ui.interact(r, ui.id().with(("aavg", s.id, area)), Sense::hover())
                            .on_hover_text(format!(
                                "Notenvorschlag Bereich: {}",
                                scale.format_final_long(scale.round_final(v))
                            ));
                    }
                }
                GCol::HalfFinal => {
                    painter.rect_filled(r, CornerRadius::ZERO, p.avg_bg.gamma_multiply(0.7));
                    if let Some(f) = course.final_grade(s.id, Period::Halbjahr) {
                        painter.text(
                            r.center(),
                            Align2::CENTER_CENTER,
                            scale.format_final(f.grade),
                            theme::bold(14.0),
                            p.text,
                        );
                    } else if let Some(v) = calc::period_average(course, s.id, Period::Halbjahr) {
                        painter.text(
                            r.center(),
                            Align2::CENTER_CENTER,
                            scale.format_avg(v),
                            FontId::proportional(13.0),
                            p.text_weak,
                        );
                        ui.interact(r, ui.id().with(("hf", s.id)), Sense::hover())
                            .on_hover_text("Halbjahresnote noch nicht festgelegt – es wird der Durchschnitt verwendet.");
                    }
                }
                GCol::Total => {
                    painter.rect_filled(r, CornerRadius::ZERO, p.avg_bg.gamma_multiply(0.7));
                    if let Some(v) = calc::period_average(course, s.id, period) {
                        painter.text(
                            r.center(),
                            Align2::CENTER_CENTER,
                            scale.format_avg(v),
                            theme::bold(13.5),
                            p.grade_color(scale.level(v)),
                        );
                    }
                }
                GCol::Suggest => {
                    painter.rect_filled(r, CornerRadius::ZERO, p.avg_bg.gamma_multiply(0.7));
                    if let Some(v) = calc::suggestion(course, s.id, period) {
                        painter.text(
                            r.center(),
                            Align2::CENTER_CENTER,
                            scale.format_final(v),
                            theme::bold(15.0),
                            p.text_weak,
                        );
                        ui.interact(r, ui.id().with(("sug", s.id)), Sense::hover())
                            .on_hover_text(scale.format_final_long(v));
                    }
                }
                GCol::Final => self.final_cell(ui, r, course, s.id, period, ro),
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn cell_editor(
        &mut self,
        ui: &mut egui::Ui,
        r: Rect,
        course: &Course,
        student: Id,
        column: Id,
        row: usize,
        ecol: usize,
        n_cols: usize,
    ) {
        let ctx = ui.ctx().clone();
        let p = Palette::get(&ctx);
        let id = egui::Id::new("grade-cell-editor");
        let started = std::mem::take(&mut self.grid.edit_started);
        let has_error = self.grid.error.is_some();

        // Navigationstasten abfangen, bevor das Textfeld sie verarbeitet
        // (Tab würde sonst den Fokus auf ein anderes Element verschieben).
        // Wichtig: Fokus vor `input_mut` abfragen, sonst Deadlock.
        let focused = !started && ctx.memory(|m| m.has_focus(id));
        let (tab, back_tab, up, down) = ctx.input_mut(|i| {
            if !focused {
                return (false, false, false, false);
            }
            (
                i.consume_key(egui::Modifiers::NONE, Key::Tab),
                i.consume_key(egui::Modifiers::SHIFT, Key::Tab),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
            )
        });

        let edit = self.grid.edit.as_mut().unwrap();
        ui.painter().rect_filled(r, CornerRadius::ZERO, p.sheet);
        let font = FontId::proportional(14.5);
        // Textfeld vertikal mittig, passend zur tatsächlichen Schrifthöhe
        let text_rect = centered_text_rect(ui, r, &font);
        let te = egui::TextEdit::singleline(&mut edit.text)
            .id(id)
            .horizontal_align(egui::Align::Center)
            .vertical_align(egui::Align::Center)
            .font(font)
            .margin(egui::Margin::symmetric(2, 1))
            .frame(egui::Frame::NONE)
            .lock_focus(true); // Tab verschiebt den Fokus nicht
        let resp = ui.put(text_rect, te);
        ui.painter().rect_stroke(
            r.shrink(1.0),
            CornerRadius::same(2),
            Stroke::new(2.0, if has_error { p.bad } else { p.accent_text }),
            egui::StrokeKind::Inside,
        );
        if started {
            resp.request_focus();
            if let Some(mut state) = egui::TextEdit::load_state(&ctx, id) {
                let n = edit.text.chars().count();
                let range = if edit.fresh {
                    egui::text::CCursorRange::one(egui::text::CCursor::new(n))
                } else {
                    egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(n),
                    )
                };
                state.cursor.set_char_range(Some(range));
                state.store(&ctx, id);
            }
        }
        if let Some(err) = &self.grid.error {
            resp.show_tooltip_text(err.clone());
        }

        let lost = resp.lost_focus();
        if !(lost || tab || back_tab || up || down) {
            return;
        }
        let (enter, shift, esc) = ctx.input(|i| {
            (
                i.key_pressed(Key::Enter),
                i.modifiers.shift,
                i.key_pressed(Key::Escape),
            )
        });
        if esc {
            self.end_edit(&ctx, id);
            return;
        }
        let navigate = enter || tab || back_tab || up || down;
        let text = self.grid.edit.as_ref().unwrap().text.clone();
        match Entry::parse(&text, course.scale) {
            Ok(entry) => {
                self.end_edit(&ctx, id);
                self.set_cell(&ctx, course.id, student, column, entry);
                if !navigate {
                    return; // Klick woanders hin
                }
                let (mut nr, mut nc) = (row as i64, ecol as i64);
                if enter {
                    nr += if shift { -1 } else { 1 };
                } else if down {
                    nr += 1;
                } else if up {
                    nr -= 1;
                } else if tab {
                    nc += 1;
                } else if back_tab {
                    nc -= 1;
                }
                let n_rows = self
                    .db
                    .class(course.class_id)
                    .map(|c| c.students.len())
                    .unwrap_or(1) as i64;
                self.grid.sel = Some((
                    nr.clamp(0, n_rows - 1) as usize,
                    nc.clamp(0, n_cols as i64 - 1) as usize,
                ));
                self.grid.scroll_to_sel = true;
            }
            Err(e) if navigate => {
                // Fehler anzeigen und weiter bearbeiten
                self.grid.error = Some(format!(
                    "{e}. Erlaubt: {}, T, U, F",
                    course.scale.input_hint()
                ));
                self.grid.edit_started = true;
                if let Some(ed) = self.grid.edit.as_mut() {
                    ed.fresh = false;
                }
            }
            Err(e) => {
                // Fokus verloren durch Klick: ungültige Eingabe verwerfen
                self.notify(&ctx, &format!("{e} – Eingabe verworfen"), true);
                self.end_edit(&ctx, id);
            }
        }
    }

    fn end_edit(&mut self, ctx: &egui::Context, id: egui::Id) {
        self.grid.edit = None;
        self.grid.error = None;
        ctx.memory_mut(|m| m.surrender_focus(id));
    }

    fn final_cell(
        &mut self,
        ui: &mut egui::Ui,
        r: Rect,
        course: &Course,
        student: Id,
        period: Period,
        ro: bool,
    ) {
        let ctx = ui.ctx().clone();
        let p = Palette::get(&ctx);
        let scale = course.scale;
        let painter = ui.painter_at(r);
        painter.rect_filled(r, CornerRadius::ZERO, p.final_bg.gamma_multiply(0.8));
        let suggestion = calc::suggestion(course, student, period);
        let record = course.final_grade(student, period);
        let resp = ui.interact(
            r,
            ui.id().with(("final", student, period == Period::Halbjahr)),
            Sense::CLICK,
        );
        if resp.hovered() && !ro {
            painter.rect_stroke(
                r.shrink(1.0),
                CornerRadius::same(2),
                Stroke::new(1.0, p.accent_text),
                egui::StrokeKind::Inside,
            );
        }
        match record {
            Some(f) => {
                let ped = f.is_pedagogical();
                let changed = !ped && suggestion.is_some_and(|s| Some(s) != f.suggestion);
                painter.text(
                    r.center() - vec2(10.0, 0.0),
                    Align2::CENTER_CENTER,
                    scale.format_final(f.grade),
                    theme::bold(16.0),
                    if ped { p.accent_text } else { p.text },
                );
                let (icon, color) = if changed {
                    (ic::WARNING, p.warn)
                } else if ped {
                    (ic::HAND_HEART, p.accent_text)
                } else {
                    (ic::CHECK_CIRCLE, p.good)
                };
                painter.text(
                    r.right_center() - vec2(18.0, 0.0),
                    Align2::CENTER_CENTER,
                    icon,
                    FontId::proportional(16.0),
                    color,
                );
                let mut tip = if ped {
                    "Pädagogische Note".to_string()
                } else {
                    "Vorschlag bestätigt".to_string()
                };
                if let Some(s) = f.suggestion {
                    tip.push_str(&format!(
                        "\nRechnerischer Vorschlag bei Festlegung: {}",
                        scale.format_final(s)
                    ));
                }
                if changed {
                    tip.push_str(&format!(
                        "\n{} Der aktuelle Vorschlag ({}) weicht inzwischen ab!",
                        ic::WARNING,
                        suggestion
                            .map(|s| scale.format_final(s))
                            .unwrap_or_default()
                    ));
                }
                if !f.reason.is_empty() {
                    tip.push_str(&format!("\nBegründung: {}", f.reason));
                }
                let resp = resp.on_hover_text(tip);
                if resp.clicked() && !ro {
                    self.dialog = Some(Dialog::final_grade(&self.db, course.id, student, period));
                }
            }
            None => {
                if let Some(s) = suggestion {
                    // Linke Hälfte: Dialog, rechte Hälfte: direkt bestätigen
                    let check = Rect::from_min_max(pos2(r.right() - 44.0, r.top()), r.max);
                    painter.text(
                        r.left_center() + vec2(16.0, 0.0),
                        Align2::LEFT_CENTER,
                        "offen",
                        FontId::proportional(12.0),
                        p.text_weak,
                    );
                    let cresp = ui.interact(
                        check,
                        ui.id()
                            .with(("confirm", student, period == Period::Halbjahr)),
                        Sense::CLICK,
                    );
                    let hovered = cresp.hovered() && !ro;
                    if hovered {
                        painter.rect_filled(
                            check.shrink(3.0),
                            CornerRadius::same(4),
                            theme::ACCENT,
                        );
                    }
                    painter.text(
                        check.center(),
                        Align2::CENTER_CENTER,
                        format!("{} {}", ic::CHECK, scale.format_final(s)),
                        theme::bold(13.0),
                        if hovered {
                            Color32::WHITE
                        } else {
                            p.accent_text
                        },
                    );
                    let cresp = cresp.on_hover_text(format!(
                        "Vorschlag {} übernehmen",
                        scale.format_final_long(s)
                    ));
                    if cresp.clicked() && !ro {
                        self.checkpoint();
                        if let Some(c) = self.db.course_mut(course.id) {
                            c.set_final(FinalRecord {
                                student,
                                period,
                                grade: s,
                                suggestion: Some(s),
                                reason: String::new(),
                            });
                        }
                        self.touch(&ctx);
                    } else if resp
                        .on_hover_text("Klicken: Note festlegen (auch pädagogisch)")
                        .clicked()
                        && !ro
                    {
                        self.dialog =
                            Some(Dialog::final_grade(&self.db, course.id, student, period));
                    }
                }
            }
        }
    }
}

fn stats_row(
    ui: &mut egui::Ui,
    course: &Course,
    students: &[gradecraft_core::Student],
    cols: &[GCol],
    avg_halves: &[Half],
    period: Period,
    total_w: f32,
) -> Option<Id> {
    let p = Palette::get(ui.ctx());
    let scale = course.scale;
    let mut clicked = None;
    let (rect, _) = ui.allocate_exact_size(vec2(total_w, ROW_H), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: 6,
            se: 6,
        },
        p.header,
    );
    painter.line_segment(
        [rect.left_top(), rect.right_top()],
        Stroke::new(1.0, p.border),
    );
    let mean =
        |vals: Vec<f64>| -> Option<f64> { calc::weighted_mean(vals.into_iter().map(|v| (v, 1.0))) };
    let mut x = rect.left();
    for col in cols {
        let r = Rect::from_min_size(pos2(x, rect.top()), vec2(col.width(), ROW_H));
        x += col.width();
        // Schaltfläche „Notenspiegel“ rechts in jeder Leistungsspalte
        if let GCol::Grade { col: column, .. } = *col {
            let btn =
                Rect::from_center_size(pos2(r.right() - 13.0, r.center().y), vec2(20.0, 20.0));
            let resp = ui
                .interact(btn, ui.id().with(("dist-btn", column)), Sense::CLICK)
                .on_hover_text("Notenspiegel anzeigen");
            if resp.hovered() {
                painter.rect_filled(btn, CornerRadius::same(4), p.accent_soft);
            }
            painter.text(
                btn.center(),
                Align2::CENTER_CENTER,
                ic::CHART_BAR,
                FontId::proportional(14.0),
                p.accent_text,
            );
            if resp.clicked() {
                clicked = Some(column);
            }
        }
        let text = match *col {
            GCol::Name => Some(("Klassendurchschnitt".to_string(), Align2::LEFT_CENTER)),
            GCol::Grade { col, .. } => calc::column_stats(course, col)
                .0
                .map(|v| (scale.format_avg(v), Align2::CENTER_CENTER)),
            GCol::AreaAvg { area, .. } => mean(
                students
                    .iter()
                    .filter_map(|s| calc::area_average(course, s.id, area, avg_halves))
                    .collect(),
            )
            .map(|v| (scale.format_avg(v), Align2::CENTER_CENTER)),
            GCol::Total => mean(
                students
                    .iter()
                    .filter_map(|s| calc::period_average(course, s.id, period))
                    .collect(),
            )
            .map(|v| (scale.format_avg(v), Align2::CENTER_CENTER)),
            GCol::Final => {
                let n = students
                    .iter()
                    .filter(|s| course.final_grade(s.id, period).is_some())
                    .count();
                Some((
                    format!("{n}/{} festgelegt", students.len()),
                    Align2::CENTER_CENTER,
                ))
            }
            _ => None,
        };
        if let Some((t, align)) = text {
            let pos = if align == Align2::LEFT_CENTER {
                r.left_center() + vec2(8.0, 0.0)
            } else {
                r.center()
            };
            // In Leistungsspalten Platz für die Schaltfläche lassen
            let pos = if matches!(col, GCol::Grade { .. }) {
                pos - vec2(10.0, 0.0)
            } else {
                pos
            };
            painter.text(pos, align, t, theme::bold(12.5), p.text_weak);
        }
    }
    clicked
}

fn weight_summary(course: &Course) -> String {
    let total: f64 = course.scheme.areas.iter().map(|a| a.weight).sum();
    if course.scheme.areas.is_empty() {
        return "keine Leistungsbereiche".into();
    }
    course
        .scheme
        .areas
        .iter()
        .map(|a| {
            format!(
                "{} {:.0} %",
                a.name,
                if total > 0.0 {
                    a.weight / total * 100.0
                } else {
                    0.0
                }
            )
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Runde Umschalt-Schaltfläche.
pub fn pill(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    let p = Palette::get(ui.ctx());
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        if selected {
            theme::bold(13.5)
        } else {
            FontId::proportional(13.5)
        },
        p.text,
    );
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 28.0, 30.0), Sense::click());
    let (fill, color) = if selected {
        (theme::ACCENT, Color32::WHITE)
    } else if resp.hovered() {
        (p.accent_soft, p.text)
    } else {
        (p.sheet, p.text)
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(15),
        fill,
        Stroke::new(1.0, if selected { theme::ACCENT } else { p.border }),
        egui::StrokeKind::Inside,
    );
    ui.painter().galley_with_override_text_color(
        rect.center() - galley.size() / 2.0,
        galley,
        color,
    );
    resp
}
