//! Stundenerfassung (mündliche Mitarbeit) und Notenspiegel-Fenster.

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, vec2};
use egui_phosphor::regular as ic;
use gradecraft_core::{Course, Half, Id, OralMark, OralRange, OralValue, calc};

use crate::config::AppConfig;
use crate::session::Session;
use crate::theme::{self, Palette};
use crate::widgets::{card, weight_drag};

const WEEKDAYS: [&str; 7] = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];

pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

pub fn fmt_date(d: NaiveDate) -> String {
    d.format("%d.%m.%Y").to_string()
}

pub fn fmt_date_long(d: NaiveDate) -> String {
    format!(
        "{}, {}",
        WEEKDAYS[d.weekday().num_days_from_monday() as usize],
        fmt_date(d)
    )
}

/// Liest „18.11.2026“, „18.11.26“ oder „18.11.“ (aktuelles Jahr).
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let parts: Vec<&str> = s
        .trim()
        .trim_end_matches('.')
        .split('.')
        .map(str::trim)
        .collect();
    let (d, m) = (parts.first()?.parse().ok()?, parts.get(1)?.parse().ok()?);
    let y = match parts.get(2) {
        None | Some(&"") => today().year(),
        Some(y) if y.len() == 2 => 2000 + y.parse::<i32>().ok()?,
        Some(y) => y.parse().ok()?,
    };
    NaiveDate::from_ymd_opt(y, m, d)
}

/// Nächster Schultag (Wochenenden überspringen).
fn step_day(d: NaiveDate, forward: bool) -> NaiveDate {
    let mut n = d;
    loop {
        n += Duration::days(if forward { 1 } else { -1 });
        if !matches!(n.weekday(), Weekday::Sat | Weekday::Sun) {
            return n;
        }
    }
}

/// Standardzeitraum eines Halbjahres aus dem Schuljahresnamen „2026/27“:
/// 1. HJ 01.08.–31.01., 2. HJ 01.02.–31.07.
pub fn default_range(year_name: &str, half: Half) -> OralRange {
    let start: i32 = year_name
        .split('/')
        .next()
        .and_then(|y| y.trim().parse().ok())
        .unwrap_or_else(|| today().year());
    let d = |y, m, day| NaiveDate::from_ymd_opt(y, m, day).unwrap();
    match half {
        Half::H1 => OralRange {
            from: d(start, 8, 1),
            to: d(start + 1, 1, 31),
        },
        Half::H2 => OralRange {
            from: d(start + 1, 2, 1),
            to: d(start + 1, 7, 31),
        },
    }
}

/// Datumsfeld (TT.MM.JJJJ). Gibt `true` zurück, wenn sich das Datum geändert hat.
pub fn date_edit(
    ui: &mut egui::Ui,
    id: egui::Id,
    date: &mut NaiveDate,
    width: f32,
) -> (bool, egui::Response) {
    let mut text: String = ui
        .data_mut(|d| d.get_temp(id))
        .unwrap_or_else(|| fmt_date(*date));
    let resp = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(id.with("te"))
            .desired_width(width),
    );
    let mut changed = false;
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    } else {
        if resp.lost_focus()
            && let Some(d) = parse_date(&text)
        {
            changed = d != *date;
            *date = d;
        }
        ui.data_mut(|d| d.remove::<String>(id));
    }
    (changed, resp)
}

impl Session {
    /// Ansicht „Mündlich“: Stundennoten per Klick vergeben.
    pub fn oral_view(&mut self, ui: &mut egui::Ui, cfg: &AppConfig, course_id: Id) {
        let ctx = ui.ctx().clone();
        let p = Palette::get(&ctx);
        let ro = self.readonly();
        let Some(course) = self.db.course(course_id).cloned() else {
            return;
        };
        let students = self
            .db
            .class(course.class_id)
            .map(|c| c.students.clone())
            .unwrap_or_default();
        let date = self.oral_date;

        // Datumsleiste
        ui.horizontal(|ui| {
            if ui
                .button(RichText::new(ic::CARET_LEFT).size(16.0))
                .on_hover_text("Vorheriger Schultag")
                .clicked()
            {
                self.oral_date = step_day(date, false);
            }
            let mut d = self.oral_date;
            ui.label(
                RichText::new(WEEKDAYS[d.weekday().num_days_from_monday() as usize])
                    .font(theme::bold(15.0)),
            );
            let (changed, _) = date_edit(ui, egui::Id::new("oral-date"), &mut d, 92.0);
            if changed {
                self.oral_date = d;
            }
            if ui
                .button(RichText::new(ic::CARET_RIGHT).size(16.0))
                .on_hover_text("Nächster Schultag")
                .clicked()
            {
                self.oral_date = step_day(date, true);
            }
            if ui.button(format!("{} Heute", ic::CALENDAR_CHECK)).clicked() {
                self.oral_date = today();
            }
            ui.add_space(16.0);
            let mut dates: Vec<NaiveDate> = course.oral.iter().map(|o| o.date).collect();
            dates.sort_unstable();
            dates.dedup();
            dates.reverse();
            egui::ComboBox::from_id_salt("oral-dates")
                .selected_text(format!("{} erfasste Stunden", dates.len()))
                .width(170.0)
                .show_ui(ui, |ui| {
                    for d in dates {
                        let n = course.oral.iter().filter(|o| o.date == d).count();
                        if ui
                            .selectable_label(
                                d == self.oral_date,
                                format!("{}  ({n})", fmt_date_long(d)),
                            )
                            .clicked()
                        {
                            self.oral_date = d;
                        }
                    }
                });
        });
        let date = self.oral_date;
        ui.add_space(6.0);

        // Verknüpfte Spalten
        let linked: Vec<(String, OralRange)> = course
            .scheme
            .areas
            .iter()
            .flat_map(|a| a.columns.iter())
            .filter_map(|c| c.oral.map(|r| (c.name.clone(), r)))
            .collect();
        let active = linked
            .iter()
            .find(|(_, r)| r.contains(date))
            .map(|(n, r)| (n.clone(), *r));
        ui.horizontal_wrapped(|ui| {
            if linked.is_empty() {
                ui.label(
                    RichText::new(format!(
                        "{} Noch keine Spalte verknüpft. Im Bewertungsschema bei einer Leistung (z. B. SoMi 1) „Stundennoten“ aktivieren – dann wird die Note dort automatisch berechnet.",
                        ic::INFO
                    ))
                    .color(p.warn),
                );
            } else {
                ui.label(RichText::new("Fließt ein in:").color(p.text_weak));
                for (name, r) in &linked {
                    let on = r.contains(date);
                    let txt = RichText::new(format!("{name} ({})", r.label())).size(12.5);
                    ui.label(if on { txt.color(p.accent_text).strong() } else { txt.color(p.text_weak) });
                }
            }
        });
        ui.add_space(8.0);

        let tile_w = 236.0;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // Kacheln in Zeilen anordnen (Spaltenzahl nach verfügbarer Breite)
            let per_row = ((ui.available_width() + 10.0) / (tile_w + 10.0)).floor().max(1.0) as usize;
            for chunk in students.chunks(per_row) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                for s in chunk {
                    let today_entry = course.oral_on(s.id, date).map(|o| o.value);
                    let range = active.as_ref().map(|(_, r)| *r);
                    let (avg, n) = match range {
                        Some(r) => (course.oral_average(s.id, r), course.oral_in_range(s.id, r).count()),
                        None => (None, 0),
                    };
                    let mut history: Vec<(NaiveDate, OralValue)> = course
                        .oral
                        .iter()
                        .filter(|o| o.student == s.id && o.date != date)
                        .map(|o| (o.date, o.value))
                        .collect();
                    history.sort_by_key(|(d, _)| std::cmp::Reverse(*d));
                    history.truncate(8);

                    let mut set: Option<Option<OralValue>> = None;
                    let frame = egui::Frame::new()
                        .fill(if today_entry.is_some() { p.accent_soft.gamma_multiply(0.55) } else { p.sheet })
                        .stroke(Stroke::new(1.0, if today_entry.is_some() { p.accent_text.gamma_multiply(0.5) } else { p.border }))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(egui::Margin::same(10));
                    frame.show(ui, |ui| {
                      ui.vertical(|ui| {
                        ui.set_width(tile_w - 20.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(self.student_name(s, cfg.first_name_first)).font(theme::bold(14.0)));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                // Menü: direkt eine Note vergeben oder Eintrag löschen
                                ui.menu_button(RichText::new(ic::DOTS_THREE).size(16.0), |ui| {
                                    ui.label(RichText::new(format!("Note für {}", fmt_date(date))).strong());
                                    ui.horizontal_wrapped(|ui| {
                                        for g in course.scale.final_grades() {
                                            if ui.add_enabled(!ro, egui::Button::new(course.scale.format_final(g))).clicked() {
                                                set = Some(Some(OralValue::Note(g)));
                                                ui.close();
                                            }
                                        }
                                    });
                                    if ui
                                        .add_enabled(!ro && today_entry.is_some(), egui::Button::new(format!("{} Eintrag löschen", ic::TRASH)))
                                        .clicked()
                                    {
                                        set = Some(None);
                                        ui.close();
                                    }
                                })
                                .response
                                .on_hover_text("Note statt Kürzel vergeben");
                                if let Some(a) = avg {
                                    ui.label(RichText::new(format!("Ø {}", course.scale.format_avg(a))).color(p.accent_text).strong())
                                        .on_hover_text(format!("{n} Stundennoten im Zeitraum"));
                                }
                            });
                        });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            for m in OralMark::ALL {
                                let sel = today_entry == Some(OralValue::Mark(m));
                                let mut b = egui::Button::new(RichText::new(m.label()).size(15.0).strong().color(if sel { Color32::WHITE } else { p.text }))
                                    .min_size(vec2(38.0, 30.0))
                                    .corner_radius(CornerRadius::same(5));
                                if sel {
                                    b = b.fill(theme::ACCENT);
                                }
                                let r = ui.add_enabled(!ro, b).on_hover_text(format!(
                                    "{} = {}",
                                    m.label(),
                                    course.scale.format(course.oral_scheme.values[m.index()])
                                ));
                                if r.clicked() {
                                    set = Some(if sel { None } else { Some(OralValue::Mark(m)) });
                                }
                            }
                        });
                        if let Some(OralValue::Note(v)) = today_entry {
                            ui.label(RichText::new(format!("Heute: Note {}", course.scale.format(v))).color(p.accent_text));
                        }
                        // Verlauf
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            if history.is_empty() {
                                ui.label(RichText::new("noch keine weiteren Einträge").size(11.0).color(p.text_weak));
                            }
                            for (d, v) in &history {
                                let txt = match v {
                                    OralValue::Mark(m) => m.label().to_string(),
                                    OralValue::Note(n) => course.scale.format(*n),
                                };
                                let in_range = range.is_some_and(|r| r.contains(*d));
                                ui.label(RichText::new(txt).size(11.5).color(if in_range { p.text } else { p.text_weak }))
                                    .on_hover_text(fmt_date_long(*d));
                            }
                        });
                    });
                    });
                    if let Some(v) = set {
                        self.checkpoint();
                        if let Some(c) = self.db.course_mut(course_id) {
                            c.set_oral(s.id, date, v);
                        }
                        self.touch(&ctx);
                    }
                }
            });
            ui.add_space(10.0);
            }

            // Umrechnung
            ui.add_space(14.0);
            let mut scheme = course.oral_scheme;
            let mut focus = false;
            card(ui, |ui| {
                ui.label(RichText::new("Umrechnung der Kürzel").font(theme::bold(14.0)).color(p.accent_text));
                ui.add_enabled_ui(!ro, |ui| {
                    ui.horizontal(|ui| {
                        for m in OralMark::ALL {
                            ui.label(RichText::new(m.label()).strong());
                            let v = &mut scheme.values[m.index()];
                            let r = ui.add(weight_drag(v).range(0.0..=15.0).speed(0.1));
                            focus |= r.gained_focus() || r.drag_started();
                            ui.add_space(8.0);
                        }
                        if ui.button("Standard").clicked() {
                            focus = true;
                            scheme = gradecraft_core::OralScheme::for_scale(course.scale);
                        }
                    });
                });
                ui.label(
                    RichText::new("Über „⋯“ auf einer Karte lässt sich direkt eine Note vergeben. Erneuter Klick auf das gewählte Kürzel entfernt es. Pro Schüler und Tag zählt ein Eintrag.")
                        .size(12.0)
                        .color(p.text_weak),
                );
            });
            if focus {
                self.checkpoint();
            }
            if scheme != course.oral_scheme {
                if let Some(c) = self.db.course_mut(course_id) {
                    c.oral_scheme = scheme;
                }
                self.touch(&ctx);
            }
        });
    }

    /// Schwebende Notenspiegel-Fenster (aktualisieren sich live).
    pub fn distribution_windows(&mut self, ctx: &egui::Context) {
        let p = Palette::get(ctx);
        let mut close = Vec::new();
        let mut pdf: Option<(Id, Id)> = None;
        for (i, &(course_id, column)) in self.dist_windows.iter().enumerate() {
            let Some(course) = self.db.course(course_id) else {
                close.push(i);
                continue;
            };
            let Some((area, col)) = course.find_column(column) else {
                close.push(i);
                continue;
            };
            let class = self
                .db
                .class(course.class_id)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let mut open = true;
            egui::Window::new(
                RichText::new(format!("{} Notenspiegel {}", ic::CHART_BAR, col.name))
                    .font(theme::bold(15.0)),
            )
            .id(egui::Id::new(("dist-win", course_id, column)))
            .open(&mut open)
            .resizable(false)
            .collapsible(true)
            .default_pos(
                ctx.content_rect().center() - vec2(200.0, 160.0) + vec2(24.0, 24.0) * i as f32,
            )
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} · Klasse {class} · {} · {}",
                        course.title(),
                        area.name,
                        col.half.label()
                    ))
                    .color(p.text_weak),
                );
                ui.add_space(8.0);
                distribution_chart(ui, course, column);
                ui.add_space(8.0);
                if ui
                    .button(format!("{} Als PDF speichern", ic::FILE_PDF))
                    .clicked()
                {
                    pdf = Some((course_id, column));
                }
            });
            if !open {
                close.push(i);
            }
        }
        for i in close.into_iter().rev() {
            self.dist_windows.remove(i);
        }
        if let Some((c, col)) = pdf {
            self.export_pdf(ctx, crate::reports::PdfKind::Distribution(c, Some(col)));
        }
    }

    pub fn open_distribution(&mut self, course: Id, column: Id) {
        if !self.dist_windows.contains(&(course, column)) {
            self.dist_windows.push((course, column));
        }
    }
}

fn level_label(course: &Course, level: u8) -> String {
    let word = course.scale.word(f64::from(level));
    match course.scale {
        gradecraft_core::ScaleKind::Noten => format!("{level}  {word}"),
        gradecraft_core::ScaleKind::Punkte => {
            let r = ["15–13", "12–10", "9–7", "6–4", "3–1", "0"][level as usize - 1];
            format!("{r} P  {word}")
        }
    }
}

/// Balkendiagramm mit Kennzahlen.
fn distribution_chart(ui: &mut egui::Ui, course: &Course, column: Id) {
    let p = Palette::get(ui.ctx());
    let dist = calc::distribution(course, column);
    let (avg, n) = calc::column_stats(course, column);
    let max = dist.iter().copied().max().unwrap_or(0).max(1) as f32;
    let bar_w = 240.0;
    egui::Grid::new(("dist", column))
        .num_columns(3)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            for (i, count) in dist.iter().enumerate() {
                let level = i as u8 + 1;
                ui.label(level_label(course, level));
                let (rect, _) = ui.allocate_exact_size(vec2(bar_w, 18.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(3), p.stripe);
                if *count > 0 {
                    let w = bar_w * *count as f32 / max;
                    let color = if level >= 5 { p.bad } else { theme::ACCENT };
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(rect.min, vec2(w, rect.height())),
                        CornerRadius::same(3),
                        color,
                    );
                }
                ui.label(RichText::new(format!("{count}")).font(theme::bold(14.0)));
                ui.end_row();
            }
        });
    ui.add_space(6.0);
    let below = dist[4] + dist[5];
    ui.horizontal_wrapped(|ui| {
        let stat = |ui: &mut egui::Ui, k: &str, v: String| {
            ui.label(RichText::new(k).color(p.text_weak));
            ui.label(RichText::new(v).strong());
            ui.add_space(10.0);
        };
        stat(
            ui,
            "Ø",
            avg.map(|a| course.scale.format_avg(a))
                .unwrap_or("–".into()),
        );
        stat(ui, "Noten", format!("{n}"));
        stat(
            ui,
            "unter ausreichend",
            format!(
                "{below} ({:.0} %)",
                if n > 0 {
                    below as f64 / n as f64 * 100.0
                } else {
                    0.0
                }
            ),
        );
    });
}
