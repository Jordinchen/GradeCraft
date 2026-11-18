//! PDF-Berichte: Notenliste, Konferenzübersicht, Schülerbogen, Notenspiegel.
//!
//! Aufbau: Die Berichte erzeugen zunächst eine Liste von Zeichenbefehlen pro
//! Seite (Text, Rechteck, Linie, Bild). Erst am Ende wird daraus mit `krilla`
//! das PDF geschrieben – so kennt jede Fußzeile die Gesamtseitenzahl.
//! Schrift: Inter (SIL Open Font License, siehe `assets/fonts/OFL.txt`).

use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect as KRect, Size, Transform};
use krilla::image::Image;
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::text::{Font, TextDirection};

use crate::calc;
use crate::model::*;
use crate::scale::{ScaleKind, format_decimal};

static FONT_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.otf");
static FONT_BOLD: &[u8] = include_bytes!("../assets/fonts/Inter-Bold.otf");

// A4 in Punkt
const A4_W: f32 = 595.28;
const A4_H: f32 = 841.89;
const MARGIN: f32 = 40.0;
const HEADER_H: f32 = 64.0;
const FOOTER_H: f32 = 26.0;

#[derive(Clone, Copy, PartialEq)]
struct Rgb(u8, u8, u8);

const BLACK: Rgb = Rgb(0x20, 0x1F, 0x1E);
const GREY: Rgb = Rgb(0x6B, 0x69, 0x67);
const LIGHT_GREY: Rgb = Rgb(0xA8, 0xA6, 0xA4);
const BORDER: Rgb = Rgb(0xD6, 0xD3, 0xD0);
const ACCENT: Rgb = Rgb(0x88, 0x00, 0x88);
const ACCENT_SOFT: Rgb = Rgb(0xF4, 0xE3, 0xF4);
const HEAD_BG: Rgb = Rgb(0xF6, 0xF1, 0xF6);
const STRIPE: Rgb = Rgb(0xFA, 0xF9, 0xFA);
const AVG_BG: Rgb = Rgb(0xF3, 0xF0, 0xF5);
const RED: Rgb = Rgb(0xC4, 0x2B, 0x1C);
const RED_SOFT: Rgb = Rgb(0xFB, 0xE6, 0xE3);

/// Angaben für den Seitenkopf.
#[derive(Clone, Default)]
pub struct Header {
    pub school: String,
    pub teacher: String,
    /// PNG- oder JPEG-Daten
    pub logo: Option<Vec<u8>>,
}

impl Header {
    pub fn from_settings(s: &DbSettings) -> Self {
        Header {
            school: s.school_name.clone(),
            teacher: s.teacher_name.clone(),
            logo: s.logo.clone(),
        }
    }
}

/// Prüft, ob Bilddaten als Logo verwendbar sind.
pub fn validate_logo(data: &[u8]) -> Result<(), String> {
    load_image(data).map(|_| ())
}

fn load_image(data: &[u8]) -> Result<Image, String> {
    if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        Image::from_png(data.to_vec().into(), true)
    } else if data.starts_with(&[0xFF, 0xD8]) {
        Image::from_jpeg(data.to_vec().into(), true)
    } else {
        Err("Nur PNG- und JPEG-Bilder werden unterstützt.".into())
    }
}

// ---------------------------------------------------------------------------
// Zeichenbefehle und Schriftmetrik
// ---------------------------------------------------------------------------

enum Op {
    Text {
        x: f32,
        y: f32,
        size: f32,
        bold: bool,
        color: Rgb,
        text: String,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Rgb,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
        color: Rgb,
    },
    Logo {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
}

struct Metrics {
    regular: ttf_parser::Face<'static>,
    bold: ttf_parser::Face<'static>,
}

impl Metrics {
    fn new() -> Self {
        Metrics {
            regular: ttf_parser::Face::parse(FONT_REGULAR, 0).expect("Schrift Inter-Regular"),
            bold: ttf_parser::Face::parse(FONT_BOLD, 0).expect("Schrift Inter-Bold"),
        }
    }

    fn width(&self, text: &str, size: f32, bold: bool) -> f32 {
        let face = if bold { &self.bold } else { &self.regular };
        let upem = f32::from(face.units_per_em());
        let units: f32 = text
            .chars()
            .map(|c| {
                face.glyph_index(c)
                    .and_then(|g| face.glyph_hor_advance(g))
                    .map(f32::from)
                    .unwrap_or(upem * 0.5)
            })
            .sum();
        units * size / upem
    }

    /// Kürzt Text mit „…“, bis er in `max` passt.
    fn fit(&self, text: &str, size: f32, bold: bool, max: f32) -> String {
        if self.width(text, size, bold) <= max {
            return text.to_string();
        }
        let mut chars: Vec<char> = text.chars().collect();
        while !chars.is_empty() {
            chars.pop();
            let t: String = chars.iter().collect::<String>() + "…";
            if self.width(&t, size, bold) <= max {
                return t;
            }
        }
        String::new()
    }

    /// Zeilenumbruch an Wortgrenzen.
    fn wrap(&self, text: &str, size: f32, bold: bool, max: f32) -> Vec<String> {
        let mut lines = Vec::new();
        for para in text.split('\n') {
            let mut line = String::new();
            for word in para.split_whitespace() {
                let candidate = if line.is_empty() {
                    word.to_string()
                } else {
                    format!("{line} {word}")
                };
                if self.width(&candidate, size, bold) <= max || line.is_empty() {
                    line = candidate;
                } else {
                    lines.push(std::mem::take(&mut line));
                    line = word.to_string();
                }
            }
            lines.push(line);
        }
        lines
    }
}

// ---------------------------------------------------------------------------
// Tabellen
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Center,
    Right,
}

struct TCol {
    title: String,
    align: Align,
    width: f32,
    min_width: f32,
}

impl TCol {
    fn new(title: impl Into<String>, align: Align) -> Self {
        TCol {
            title: title.into(),
            align,
            width: 0.0,
            min_width: 22.0,
        }
    }
    fn min(mut self, w: f32) -> Self {
        self.min_width = w;
        self
    }
}

#[derive(Clone, Default)]
struct TCell {
    text: String,
    bold: bool,
    color: Option<Rgb>,
    bg: Option<Rgb>,
}

impl TCell {
    fn new(text: impl Into<String>) -> Self {
        TCell {
            text: text.into(),
            ..Default::default()
        }
    }
    fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    fn color(mut self, c: Rgb) -> Self {
        self.color = Some(c);
        self
    }
    fn bg(mut self, c: Rgb) -> Self {
        self.bg = Some(c);
        self
    }
}

struct Table {
    /// Gruppenzeile über den Spalten: (Titel, Anzahl Spalten)
    groups: Vec<(String, usize)>,
    cols: Vec<TCol>,
    rows: Vec<Vec<TCell>>,
    footer: Option<Vec<TCell>>,
    /// Spalten mit grauem Hintergrund (Durchschnitte, Zeugnisnoten)
    shaded: Vec<usize>,
    font: f32,
}

impl Table {
    fn new(cols: Vec<TCol>) -> Self {
        Table {
            groups: Vec::new(),
            cols,
            rows: Vec::new(),
            footer: None,
            shaded: Vec::new(),
            font: 9.0,
        }
    }

    /// Spaltenbreiten aus dem Inhalt bestimmen; liefert die Gesamtbreite.
    fn autosize(&mut self, m: &Metrics) -> f32 {
        let pad = 10.0;
        for (i, col) in self.cols.iter_mut().enumerate() {
            let longest_word = col
                .title
                .split_whitespace()
                .map(|w| m.width(w, self.font - 0.5, true))
                .fold(0.0, f32::max);
            // Überschrift darf auf bis zu 3 Zeilen umbrechen
            let title = (m.width(&col.title, self.font - 0.5, true) / 2.6).max(longest_word);
            let content = self
                .rows
                .iter()
                .chain(self.footer.iter())
                .filter_map(|r| r.get(i))
                .map(|c| m.width(&c.text, self.font, c.bold))
                .fold(0.0, f32::max);
            col.width = (title.max(content) + pad).max(col.min_width);
        }
        // Gruppentitel müssen in ihre Spalten passen
        let mut start = 0;
        for (title, span) in &self.groups {
            let w: f32 = self.cols[start..start + span].iter().map(|c| c.width).sum();
            let need = m.width(title, self.font - 0.5, true) + pad;
            if need > w && *span > 0 {
                let extra = (need - w) / *span as f32;
                for c in &mut self.cols[start..start + span] {
                    c.width += extra;
                }
            }
            start += span;
        }
        self.total_width()
    }

    fn total_width(&self) -> f32 {
        self.cols.iter().map(|c| c.width).sum()
    }

    /// Passt Breite (und ggf. Schriftgröße) an die verfügbare Breite an.
    fn fit_to(&mut self, available: f32) {
        let total = self.total_width();
        if total > available {
            let f = available / total;
            for c in &mut self.cols {
                c.width *= f;
            }
            self.font = (self.font * f.max(0.75)).max(6.5);
        } else if let Some(name_col) = self.cols.iter().position(|c| c.align == Align::Left) {
            // Restbreite der Namensspalte geben, aber nicht übertreiben
            let extra = (available - total).min(60.0);
            self.cols[name_col].width += extra;
        }
    }
}

// ---------------------------------------------------------------------------
// Seitenlayout
// ---------------------------------------------------------------------------

struct Layout<'a> {
    m: Metrics,
    w: f32,
    h: f32,
    pages: Vec<Vec<Op>>,
    y: f32,
    title: String,
    subtitle: String,
    header: &'a Header,
    logo_size: Option<(f32, f32)>,
}

impl<'a> Layout<'a> {
    fn new(title: &str, subtitle: &str, header: &'a Header, landscape: bool) -> Self {
        let (w, h) = if landscape {
            (A4_H, A4_W)
        } else {
            (A4_W, A4_H)
        };
        let logo_size = header
            .logo
            .as_deref()
            .and_then(|d| load_image(d).ok())
            .map(|img| {
                let (iw, ih) = img.size();
                let max_h = 42.0_f32;
                let max_w = 120.0_f32;
                let scale = (max_h / ih as f32).min(max_w / iw as f32);
                (iw as f32 * scale, ih as f32 * scale)
            });
        let mut l = Layout {
            m: Metrics::new(),
            w,
            h,
            pages: Vec::new(),
            y: 0.0,
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            header,
            logo_size,
        };
        l.new_page();
        l
    }

    fn content_w(&self) -> f32 {
        self.w - 2.0 * MARGIN
    }

    fn bottom(&self) -> f32 {
        self.h - MARGIN - FOOTER_H
    }

    fn ops(&mut self) -> &mut Vec<Op> {
        self.pages.last_mut().unwrap()
    }

    fn new_page(&mut self) {
        self.pages.push(Vec::new());
        let top = MARGIN;
        // Logo und Schule/Lehrkraft rechts
        let mut right = self.w - MARGIN;
        if let Some((lw, lh)) = self.logo_size {
            self.ops().push(Op::Logo {
                x: right - lw,
                y: top - 4.0,
                w: lw,
                h: lh,
            });
            right -= lw + 12.0;
        }
        let school = self.header.school.clone();
        let teacher = self.header.teacher.clone();
        if !school.is_empty() {
            self.text_right(right, top + 10.0, &school, 9.5, true, BLACK);
        }
        if !teacher.is_empty() {
            self.text_right(right, top + 23.0, &teacher, 9.0, false, GREY);
        }
        let title = self.title.clone();
        let subtitle = self.subtitle.clone();
        self.text(MARGIN, top + 14.0, &title, 17.0, true, ACCENT);
        self.text(MARGIN, top + 30.0, &subtitle, 10.0, false, GREY);
        let y = top + HEADER_H - 16.0;
        let x2 = self.w - MARGIN;
        self.line(MARGIN, y, x2, y, 1.2, ACCENT);
        self.y = top + HEADER_H;
    }

    /// Neue Seite, falls `needed` nicht mehr passt. Gibt `true` zurück, wenn umgebrochen wurde.
    fn ensure(&mut self, needed: f32) -> bool {
        if self.y + needed > self.bottom() {
            self.new_page();
            true
        } else {
            false
        }
    }

    fn text(&mut self, x: f32, y: f32, text: &str, size: f32, bold: bool, color: Rgb) {
        if !text.is_empty() {
            self.ops().push(Op::Text {
                x,
                y,
                size,
                bold,
                color,
                text: text.to_string(),
            });
        }
    }

    fn text_right(&mut self, right: f32, y: f32, text: &str, size: f32, bold: bool, color: Rgb) {
        let w = self.m.width(text, size, bold);
        self.text(right - w, y, text, size, bold, color);
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgb) {
        self.ops().push(Op::Rect { x, y, w, h, color });
    }

    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Rgb) {
        self.ops().push(Op::Line {
            x1,
            y1,
            x2,
            y2,
            width,
            color,
        });
    }

    fn heading(&mut self, text: &str) {
        self.ensure(40.0);
        self.y += 6.0;
        self.text(MARGIN, self.y + 11.0, text, 12.5, true, ACCENT);
        self.y += 20.0;
    }

    fn paragraph(&mut self, text: &str, size: f32, color: Rgb) {
        let lines = self.m.wrap(text, size, false, self.content_w());
        for line in lines {
            self.ensure(size * 1.5);
            self.text(MARGIN, self.y + size, &line, size, false, color);
            self.y += size * 1.45;
        }
    }

    fn space(&mut self, h: f32) {
        self.y += h;
    }

    fn table(&mut self, t: &Table) {
        let row_h = t.font * 1.95;
        let head_font = t.font - 0.5;
        // Titelzeile ggf. zweizeilig
        let title_lines: Vec<Vec<String>> = t
            .cols
            .iter()
            .map(|c| {
                let mut l = self.m.wrap(&c.title, head_font, true, c.width - 6.0);
                l.truncate(3);
                l
            })
            .collect();
        let n_title = title_lines
            .iter()
            .map(|l| l.len())
            .max()
            .unwrap_or(1)
            .max(1);
        let title_h = head_font * 1.35 * n_title as f32 + 8.0;
        let group_h = if t.groups.is_empty() {
            0.0
        } else {
            head_font * 1.4 + 8.0
        };
        let head_h = group_h + title_h;

        let draw_head = |l: &mut Layout| {
            let x0 = MARGIN;
            let y0 = l.y;
            let total = t.total_width();
            l.rect(x0, y0, total, head_h, HEAD_BG);
            let mut x = x0;
            if !t.groups.is_empty() {
                let mut ci = 0;
                for (title, span) in &t.groups {
                    let w: f32 = t.cols[ci..ci + span].iter().map(|c| c.width).sum();
                    if !title.is_empty() {
                        let tw = l.m.width(title, head_font, true);
                        l.text(
                            x + (w - tw) / 2.0,
                            y0 + head_font + 4.0,
                            title,
                            head_font,
                            true,
                            ACCENT,
                        );
                        l.line(
                            x + 3.0,
                            y0 + group_h,
                            x + w - 3.0,
                            y0 + group_h,
                            0.5,
                            BORDER,
                        );
                    }
                    if ci > 0 {
                        l.line(x, y0, x, y0 + head_h, 0.6, BORDER);
                    }
                    x += w;
                    ci += span;
                }
                x = x0;
            }
            for (i, c) in t.cols.iter().enumerate() {
                if t.shaded.contains(&i) {
                    l.rect(x, y0 + group_h, c.width, title_h, AVG_BG);
                }
                let lines = &title_lines[i];
                let block = lines.len() as f32 * head_font * 1.35;
                let mut ty = y0 + group_h + (title_h - block) / 2.0 + head_font;
                for line in lines {
                    let tw = l.m.width(line, head_font, true);
                    let tx = match c.align {
                        Align::Left => x + 4.0,
                        Align::Center => x + (c.width - tw) / 2.0,
                        Align::Right => x + c.width - tw - 4.0,
                    };
                    l.text(tx, ty, line, head_font, true, BLACK);
                    ty += head_font * 1.35;
                }
                x += c.width;
            }
            l.line(x0, y0 + head_h, x0 + total, y0 + head_h, 0.8, GREY);
            l.y += head_h;
        };

        self.ensure(head_h + row_h * 2.0);
        draw_head(self);
        let total = t.total_width();
        let n = t.rows.len();
        for (ri, row) in t.rows.iter().chain(t.footer.iter()).enumerate() {
            let is_footer = ri >= n;
            if self.y + row_h > self.bottom() {
                self.new_page();
                draw_head(self);
            }
            let y0 = self.y;
            let bg = if is_footer {
                Some(HEAD_BG)
            } else if ri % 2 == 1 {
                Some(STRIPE)
            } else {
                None
            };
            if let Some(bg) = bg {
                self.rect(MARGIN, y0, total, row_h, bg);
            }
            let mut x = MARGIN;
            for (i, c) in t.cols.iter().enumerate() {
                let cell = row.get(i).cloned().unwrap_or_default();
                if let Some(bg) = cell.bg {
                    self.rect(x, y0, c.width, row_h, bg);
                } else if t.shaded.contains(&i) && !is_footer {
                    self.rect(x, y0, c.width, row_h, AVG_BG);
                }
                let text = self
                    .m
                    .fit(&cell.text, t.font, cell.bold || is_footer, c.width - 6.0);
                let tw = self.m.width(&text, t.font, cell.bold || is_footer);
                let tx = match c.align {
                    Align::Left => x + 4.0,
                    Align::Center => x + (c.width - tw) / 2.0,
                    Align::Right => x + c.width - tw - 4.0,
                };
                let color = cell.color.unwrap_or(if is_footer { GREY } else { BLACK });
                self.text(
                    tx,
                    y0 + row_h / 2.0 + t.font * 0.36,
                    &text,
                    t.font,
                    cell.bold || is_footer,
                    color,
                );
                x += c.width;
            }
            self.line(
                MARGIN,
                y0 + row_h,
                MARGIN + total,
                y0 + row_h,
                if is_footer { 0.8 } else { 0.3 },
                BORDER,
            );
            self.y += row_h;
        }
        // Gruppentrenner durch die ganze Tabelle
        self.space(10.0);
    }

    /// Unterschriftszeilen am Ende.
    fn signatures(&mut self, labels: &[&str]) {
        self.ensure(70.0);
        self.space(34.0);
        let n = labels.len() as f32;
        let gap = 30.0;
        let w = (self.content_w() - gap * (n - 1.0)) / n;
        let date = chrono::Local::now().format("%d.%m.%Y").to_string();
        for (i, label) in labels.iter().enumerate() {
            let x = MARGIN + i as f32 * (w + gap);
            if i == 0 {
                self.text(x + 2.0, self.y - 4.0, &date, 9.5, false, BLACK);
            }
            self.line(x, self.y, x + w, self.y, 0.7, BLACK);
            self.text(x, self.y + 11.0, label, 8.5, false, GREY);
        }
        self.space(20.0);
    }

    fn finish(self) -> Result<Vec<u8>, String> {
        let mut doc = Document::new();
        doc.set_metadata(
            Metadata::new()
                .title(self.title.clone())
                .creator(format!("GradeCraft {}", env!("CARGO_PKG_VERSION"))),
        );
        let regular = Font::new(FONT_REGULAR.to_vec().into(), 0)
            .ok_or("Schrift konnte nicht geladen werden")?;
        let bold =
            Font::new(FONT_BOLD.to_vec().into(), 0).ok_or("Schrift konnte nicht geladen werden")?;
        let logo = self.header.logo.as_deref().and_then(|d| load_image(d).ok());
        let n_pages = self.pages.len();
        let created = format!(
            "Erstellt mit GradeCraft am {} · vertraulich – enthält personenbezogene Daten",
            chrono::Local::now().format("%d.%m.%Y, %H:%M Uhr")
        );
        for (pi, ops) in self.pages.iter().enumerate() {
            let mut page = doc.start_page_with(
                PageSettings::from_wh(self.w, self.h).ok_or("Ungültige Seitengröße")?,
            );
            let mut s = page.surface();
            let fill = |c: Rgb| Fill {
                paint: rgb::Color::new(c.0, c.1, c.2).into(),
                opacity: NormalizedF32::ONE,
                rule: Default::default(),
            };
            let draw_text = |s: &mut krilla::surface::Surface,
                             x: f32,
                             y: f32,
                             size: f32,
                             bold_font: bool,
                             color: Rgb,
                             text: &str| {
                s.set_fill(Some(fill(color)));
                s.set_stroke(None);
                s.draw_text(
                    Point::from_xy(x, y),
                    if bold_font {
                        bold.clone()
                    } else {
                        regular.clone()
                    },
                    size,
                    text,
                    false,
                    TextDirection::Auto,
                );
            };
            for op in ops {
                match op {
                    Op::Text {
                        x,
                        y,
                        size,
                        bold,
                        color,
                        text,
                    } => draw_text(&mut s, *x, *y, *size, *bold, *color, text),
                    Op::Rect { x, y, w, h, color } => {
                        if let Some(r) = KRect::from_xywh(*x, *y, *w, *h) {
                            let mut pb = PathBuilder::new();
                            pb.push_rect(r);
                            if let Some(path) = pb.finish() {
                                s.set_fill(Some(fill(*color)));
                                s.set_stroke(None);
                                s.draw_path(&path);
                            }
                        }
                    }
                    Op::Line {
                        x1,
                        y1,
                        x2,
                        y2,
                        width,
                        color,
                    } => {
                        let mut pb = PathBuilder::new();
                        pb.move_to(*x1, *y1);
                        pb.line_to(*x2, *y2);
                        if let Some(path) = pb.finish() {
                            s.set_fill(None);
                            s.set_stroke(Some(Stroke {
                                paint: rgb::Color::new(color.0, color.1, color.2).into(),
                                width: *width,
                                ..Default::default()
                            }));
                            s.draw_path(&path);
                        }
                    }
                    Op::Logo { x, y, w, h } => {
                        if let (Some(img), Some(size)) = (logo.clone(), Size::from_wh(*w, *h)) {
                            s.push_transform(&Transform::from_translate(*x, *y));
                            s.draw_image(img, size);
                            s.pop();
                        }
                    }
                }
            }
            // Fußzeile
            let fy = self.h - MARGIN + 4.0;
            let m = Metrics::new();
            s.set_fill(None);
            s.set_stroke(Some(Stroke {
                paint: rgb::Color::new(BORDER.0, BORDER.1, BORDER.2).into(),
                width: 0.5,
                ..Default::default()
            }));
            let mut pb = PathBuilder::new();
            pb.move_to(MARGIN, fy - 12.0);
            pb.line_to(self.w - MARGIN, fy - 12.0);
            if let Some(path) = pb.finish() {
                s.draw_path(&path);
            }
            draw_text(&mut s, MARGIN, fy, 7.5, false, LIGHT_GREY, &created);
            let pn = format!("Seite {} von {}", pi + 1, n_pages);
            let pw = m.width(&pn, 8.0, false);
            draw_text(&mut s, self.w - MARGIN - pw, fy, 8.0, false, GREY, &pn);
            s.finish();
            page.finish();
        }
        doc.finish()
            .map_err(|e| format!("PDF konnte nicht erstellt werden: {e:?}"))
    }
}

// ---------------------------------------------------------------------------
// Hilfsfunktionen für Noten
// ---------------------------------------------------------------------------

fn is_deficit(scale: ScaleKind, v: f64) -> bool {
    scale.level(v) >= 5
}

fn entry_cell(scale: ScaleKind, e: Entry) -> TCell {
    match e {
        Entry::Note(v) => {
            let c = TCell::new(scale.format(v));
            if is_deficit(scale, v) {
                c.color(RED)
            } else {
                c
            }
        }
        Entry::Ungenuegend(_) => TCell::new(e.format(scale)).color(RED),
        Entry::Fehlt => TCell::new("f").color(LIGHT_GREY),
    }
}

fn avg_cell(scale: ScaleKind, v: Option<f64>) -> TCell {
    match v {
        Some(v) => {
            let c = TCell::new(scale.format_avg(v));
            if is_deficit(scale, scale.round_final(v)) {
                c.color(RED)
            } else {
                c
            }
        }
        None => TCell::new(""),
    }
}

fn final_cell(course: &Course, student: Id, period: Period) -> TCell {
    let scale = course.scale;
    match course.final_grade(student, period) {
        Some(f) => {
            let mut t = scale.format_final(f.grade);
            if f.is_pedagogical() {
                t.push_str(" P");
            }
            let c = TCell::new(t).bold();
            if is_deficit(scale, f.grade) {
                c.color(RED)
            } else {
                c
            }
        }
        None => TCell::new("–").color(LIGHT_GREY),
    }
}

fn mean(vals: impl IntoIterator<Item = f64>) -> Option<f64> {
    calc::weighted_mean(vals.into_iter().map(|v| (v, 1.0)))
}

fn student_name(s: &Student) -> String {
    s.display_name()
}

fn context(db: &Database, class: &SchoolClass) -> String {
    let year = db
        .year(class.year_id)
        .map(|y| y.name.clone())
        .unwrap_or_default();
    format!("Klasse {} · Schuljahr {}", class.name, year)
}

fn pct(area: &Area, scheme: &Scheme) -> f64 {
    let total: f64 = scheme.areas.iter().map(|a| a.weight).sum();
    if total > 0.0 {
        area.weight / total * 100.0
    } else {
        0.0
    }
}

fn legend(scale: ScaleKind) -> String {
    let w = scale.format(scale.worst());
    format!(
        "Legende: {w} T = ungenügend wegen Täuschung · {w} U = ungenügend wegen unentschuldigter Nichtteilnahme · \
         f = fehlt (zählt nicht) · P = pädagogische Note · – = noch nicht festgelegt. \
         Durchschnitte ungerundet, Vorschläge kaufmännisch gerundet."
    )
}

// ---------------------------------------------------------------------------
// 1. Notenliste je Fach
// ---------------------------------------------------------------------------

fn half_table(course: &Course, class: &SchoolClass, half: Half) -> Table {
    let scale = course.scale;
    let period = if half == Half::H1 {
        Period::Halbjahr
    } else {
        Period::Schuljahr
    };
    let all_year = half == Half::H2 && course.scheme.year_mode == YearMode::AlleNoten;
    let avg_halves: Vec<Half> = if all_year {
        vec![Half::H1, Half::H2]
    } else {
        vec![half]
    };
    // Bereiche ohne Leistungen in diesem Halbjahr ausblenden (wie in der App)
    let shown = |a: &&Area| all_year || a.columns.iter().any(|c| c.half == half);
    let mut cols = vec![
        TCol::new("Nr.", Align::Right).min(20.0),
        TCol::new("Name", Align::Left).min(110.0),
    ];
    let mut groups = vec![(String::new(), 2)];
    let mut shaded = Vec::new();
    for a in course.scheme.areas.iter().filter(|a| shown(a)) {
        let mut span = 0;
        for c in a.columns.iter().filter(|c| c.half == half) {
            let title = if (c.weight - 1.0).abs() > 1e-9 {
                format!(
                    "{} ×{}",
                    c.name,
                    crate::scale::format_decimal(c.weight, 1).trim_end_matches(",0")
                )
            } else {
                c.name.clone()
            };
            cols.push(TCol::new(title, Align::Center));
            span += 1;
        }
        shaded.push(cols.len());
        cols.push(TCol::new(
            if all_year { "Ø Jahr" } else { "Ø" },
            Align::Center,
        ));
        groups.push((
            format!("{} · {:.0} %", a.name, pct(a, &course.scheme)),
            span + 1,
        ));
    }
    let mut zspan = 3;
    if half == Half::H2 && matches!(course.scheme.year_mode, YearMode::Halbjahre { .. }) {
        shaded.push(cols.len());
        cols.push(TCol::new("Halbjahresnote", Align::Center));
        zspan += 1;
    }
    shaded.push(cols.len());
    cols.push(TCol::new(
        if half == Half::H1 {
            "Ø Halbjahr"
        } else {
            "Ø Schuljahr"
        },
        Align::Center,
    ));
    shaded.push(cols.len());
    cols.push(TCol::new("Vorschlag", Align::Center));
    cols.push(
        TCol::new(
            if half == Half::H1 {
                "Halbjahresnote"
            } else {
                "Schuljahresnote"
            },
            Align::Center,
        )
        .min(40.0),
    );
    groups.push(("Zeugnis".into(), zspan));

    let mut t = Table::new(cols);
    t.groups = groups;
    t.shaded = shaded;
    for (i, s) in class.students.iter().enumerate() {
        let mut row = vec![
            TCell::new(format!("{}", i + 1)).color(GREY),
            TCell::new(student_name(s)),
        ];
        for a in course.scheme.areas.iter().filter(|a| shown(a)) {
            for c in a.columns.iter().filter(|c| c.half == half) {
                row.push(
                    course
                        .effective_entry(s.id, c.id)
                        .map(|(e, auto)| {
                            let cell = entry_cell(scale, e);
                            if auto { cell.color(ACCENT) } else { cell }
                        })
                        .unwrap_or_default(),
                );
            }
            row.push(avg_cell(
                scale,
                calc::area_average(course, s.id, a.id, &avg_halves),
            ));
        }
        if half == Half::H2 && matches!(course.scheme.year_mode, YearMode::Halbjahre { .. }) {
            row.push(match course.final_grade(s.id, Period::Halbjahr) {
                Some(f) => TCell::new(scale.format_final(f.grade)),
                None => avg_cell(scale, calc::period_average(course, s.id, Period::Halbjahr))
                    .color(GREY),
            });
        }
        row.push(avg_cell(scale, calc::period_average(course, s.id, period)).bold());
        row.push(
            calc::suggestion(course, s.id, period)
                .map(|v| TCell::new(scale.format_final(v)).color(GREY))
                .unwrap_or_default(),
        );
        let mut fc = final_cell(course, s.id, period);
        fc.bg = Some(ACCENT_SOFT);
        row.push(fc);
        t.rows.push(row);
    }
    // Klassendurchschnitt
    let mut foot = vec![TCell::new(""), TCell::new("Klassendurchschnitt")];
    for a in course.scheme.areas.iter().filter(|a| shown(a)) {
        for c in a.columns.iter().filter(|c| c.half == half) {
            foot.push(TCell::new(
                calc::column_stats(course, c.id)
                    .0
                    .map(|v| scale.format_avg(v))
                    .unwrap_or_default(),
            ));
        }
        foot.push(TCell::new(
            mean(
                class
                    .students
                    .iter()
                    .filter_map(|s| calc::area_average(course, s.id, a.id, &avg_halves)),
            )
            .map(|v| scale.format_avg(v))
            .unwrap_or_default(),
        ));
    }
    if half == Half::H2 && matches!(course.scheme.year_mode, YearMode::Halbjahre { .. }) {
        foot.push(TCell::new(""));
    }
    foot.push(TCell::new(
        mean(
            class
                .students
                .iter()
                .filter_map(|s| calc::period_average(course, s.id, period)),
        )
        .map(|v| scale.format_avg(v))
        .unwrap_or_default(),
    ));
    foot.push(TCell::new(""));
    let n_final = class
        .students
        .iter()
        .filter(|s| course.final_grade(s.id, period).is_some())
        .count();
    foot.push(TCell::new(format!("{n_final}/{}", class.students.len())));
    t.footer = Some(foot);
    t
}

/// Notenliste eines Fachs (beide Halbjahre).
pub fn course_list(db: &Database, course_id: Id, header: &Header) -> Result<Vec<u8>, String> {
    let course = db.course(course_id).ok_or("Fach nicht gefunden")?;
    let class = db.class(course.class_id).ok_or("Klasse nicht gefunden")?;
    let m = Metrics::new();
    let mut tables: Vec<(String, Table)> = Vec::new();
    for half in [Half::H1, Half::H2] {
        let mut t = half_table(course, class, half);
        t.autosize(&m);
        let title = if half == Half::H1 {
            "1. Halbjahr – Halbjahresnote".to_string()
        } else {
            "2. Halbjahr – Schuljahresnote".to_string()
        };
        tables.push((title, t));
    }
    let widest = tables
        .iter()
        .map(|(_, t)| t.total_width())
        .fold(0.0, f32::max);
    let landscape = widest > A4_W - 2.0 * MARGIN;
    let title = format!("Notenliste {}", course.title());
    let subtitle = format!("{} · {}", context(db, class), course.scale.label());
    let mut l = Layout::new(&title, &subtitle, header, landscape);
    let year_mode = match course.scheme.year_mode {
        YearMode::Halbjahre { w1, w2 } => format!(
            "Schuljahresnote aus Halbjahresnote und 2. Halbjahr im Verhältnis {} : {}.",
            format_decimal(w1, 1).trim_end_matches(",0"),
            format_decimal(w2, 1).trim_end_matches(",0")
        ),
        YearMode::AlleNoten => "Schuljahresnote aus allen Einzelnoten des Schuljahres.".to_string(),
    };
    for (title, mut t) in tables {
        t.fit_to(l.content_w());
        l.heading(&title);
        l.table(&t);
    }
    l.paragraph(
        &format!("{} {}", legend(course.scale), year_mode),
        7.5,
        GREY,
    );

    // Begründungen pädagogischer Noten
    let peds: Vec<String> = class
        .students
        .iter()
        .flat_map(|s| [Period::Halbjahr, Period::Schuljahr].map(|p| (s, p)))
        .filter_map(|(s, p)| {
            let f = course.final_grade(s.id, p)?;
            f.is_pedagogical().then(|| {
                format!(
                    "{} – {}: {} (Vorschlag {}){}",
                    student_name(s),
                    p.label(),
                    course.scale.format_final(f.grade),
                    f.suggestion
                        .map(|v| course.scale.format_final(v))
                        .unwrap_or("–".into()),
                    if f.reason.is_empty() {
                        String::new()
                    } else {
                        format!(". Begründung: {}", f.reason)
                    }
                )
            })
        })
        .collect();
    if !peds.is_empty() {
        l.heading("Pädagogische Notenbildung");
        for p in peds {
            l.paragraph(&format!("• {p}"), 9.0, BLACK);
        }
    }
    l.signatures(&["Datum, Unterschrift Lehrkraft"]);
    l.finish()
}

// ---------------------------------------------------------------------------
// 2. Konferenzübersicht
// ---------------------------------------------------------------------------

/// Übersicht aller Fächer einer Klasse für die Zeugniskonferenz.
pub fn conference(
    db: &Database,
    class_id: Id,
    period: Period,
    header: &Header,
) -> Result<Vec<u8>, String> {
    let class = db.class(class_id).ok_or("Klasse nicht gefunden")?;
    let courses: Vec<&Course> = db.courses_of_class(class_id).collect();
    if courses.is_empty() {
        return Err("Diese Klasse hat noch keine Fächer.".into());
    }
    let mut cols = vec![
        TCol::new("Nr.", Align::Right).min(20.0),
        TCol::new("Name", Align::Left).min(110.0),
    ];
    for c in &courses {
        let t = if c.short.is_empty() {
            c.subject.clone()
        } else {
            c.short.clone()
        };
        cols.push(TCol::new(t, Align::Center).min(30.0));
    }
    let def_col = cols.len();
    cols.push(TCol::new("Defizite", Align::Center));
    let mut t = Table::new(cols);
    t.font = 10.0;
    t.shaded = vec![def_col];
    for (i, s) in class.students.iter().enumerate() {
        let mut row = vec![
            TCell::new(format!("{}", i + 1)).color(GREY),
            TCell::new(student_name(s)),
        ];
        let mut deficits = 0;
        for c in &courses {
            let cell = match c.final_grade(s.id, period) {
                Some(f) => {
                    let mut txt = c.scale.format_final(f.grade);
                    if f.is_pedagogical() {
                        txt.push_str(" P");
                    }
                    let cell = TCell::new(txt).bold();
                    if is_deficit(c.scale, f.grade) {
                        deficits += 1;
                        cell.color(RED).bg(RED_SOFT)
                    } else {
                        cell
                    }
                }
                None => match calc::suggestion(c, s.id, period) {
                    Some(v) => {
                        if is_deficit(c.scale, v) {
                            deficits += 1;
                        }
                        TCell::new(format!("({})", c.scale.format_final(v))).color(
                            if is_deficit(c.scale, v) {
                                RED
                            } else {
                                LIGHT_GREY
                            },
                        )
                    }
                    None => TCell::new("–").color(LIGHT_GREY),
                },
            };
            row.push(cell);
        }
        row.push(if deficits > 0 {
            TCell::new(format!("{deficits}")).bold().color(RED)
        } else {
            TCell::new("0").color(GREY)
        });
        t.rows.push(row);
    }
    let mut foot = vec![TCell::new(""), TCell::new("Durchschnitt")];
    for c in &courses {
        let vals = class.students.iter().filter_map(|s| {
            c.final_grade(s.id, period)
                .map(|f| f.grade)
                .or_else(|| calc::suggestion(c, s.id, period))
        });
        foot.push(TCell::new(
            mean(vals)
                .map(|v| c.scale.format_avg(v))
                .unwrap_or_default(),
        ));
    }
    foot.push(TCell::new(""));
    t.footer = Some(foot);

    let m = Metrics::new();
    let width = t.autosize(&m);
    let landscape = width > A4_W - 2.0 * MARGIN;
    let title = format!("Konferenzübersicht – {}", period.label());
    let mut l = Layout::new(&title, &context(db, class), header, landscape);
    t.fit_to(l.content_w());
    l.table(&t);
    let subjects: Vec<String> = courses
        .iter()
        .map(|c| {
            format!(
                "{} = {} ({})",
                if c.short.is_empty() {
                    &c.subject
                } else {
                    &c.short
                },
                c.subject,
                c.scale.label()
            )
        })
        .collect();
    l.paragraph(&format!("Fächer: {}", subjects.join(" · ")), 8.0, GREY);
    l.paragraph(
        "Fett = festgelegte Zeugnisnote · (3) = rechnerischer Vorschlag, noch nicht festgelegt · P = pädagogische Note · \
         Defizite: mangelhaft oder ungenügend (bei Punkten: unter 5 Punkte).",
        8.0,
        GREY,
    );
    l.signatures(&["Datum, Unterschrift Lehrkraft"]);
    l.finish()
}

// ---------------------------------------------------------------------------
// 3. Schülerbogen
// ---------------------------------------------------------------------------

/// Schülerbogen mit allen Noten; eine Seite (oder mehr) pro Schüler.
/// `student`: nur dieser Schüler, sonst die ganze Klasse.
/// `course`: nur dieses Fach, sonst alle Fächer der Klasse.
pub fn student_sheets(
    db: &Database,
    class_id: Id,
    student: Option<Id>,
    course: Option<Id>,
    header: &Header,
) -> Result<Vec<u8>, String> {
    let class = db.class(class_id).ok_or("Klasse nicht gefunden")?;
    let students: Vec<&Student> = class
        .students
        .iter()
        .filter(|s| student.is_none_or(|id| s.id == id))
        .collect();
    if students.is_empty() {
        return Err("Keine Schüler ausgewählt.".into());
    }
    let courses: Vec<&Course> = db
        .courses_of_class(class_id)
        .filter(|c| course.is_none_or(|id| c.id == id))
        .collect();
    let ctx = context(db, class);
    let mut l: Option<Layout> = None;
    for s in students {
        let subtitle = format!("{} · {}", student_name(s), ctx);
        let layout = match l.as_mut() {
            None => l.insert(Layout::new("Schülerbogen", &subtitle, header, false)),
            Some(layout) => {
                layout.subtitle = subtitle;
                layout.new_page();
                layout
            }
        };
        if courses.is_empty() {
            layout.paragraph(
                "Für diese Klasse sind noch keine Fächer angelegt.",
                10.0,
                GREY,
            );
        }
        for c in &courses {
            let scale = c.scale;
            layout.heading(&format!("{} · {}", c.title(), scale.label()));
            // Einzelleistungen
            let mut t = Table::new(vec![
                TCol::new("Leistungsbereich", Align::Left).min(110.0),
                TCol::new("Leistung", Align::Left).min(90.0),
                TCol::new("Halbjahr", Align::Center),
                TCol::new("Wertigkeit", Align::Center),
                TCol::new("Note", Align::Center).min(40.0),
                TCol::new("Bemerkung", Align::Left).min(120.0),
            ]);
            for a in &c.scheme.areas {
                for col in &a.columns {
                    let g = c.grade(s.id, col.id);
                    let eff = c.effective_entry(s.id, col.id);
                    let mut note = eff
                        .map(|(e, _)| entry_cell(scale, e))
                        .unwrap_or_else(|| TCell::new("–").color(LIGHT_GREY));
                    note.bold = eff.is_some();
                    let remark = g
                        .map(|g| {
                            let mut r = match g.entry {
                                Entry::Ungenuegend(_) | Entry::Fehlt => g.entry.describe(scale),
                                _ => String::new(),
                            };
                            if !g.comment.is_empty() {
                                if !r.is_empty() {
                                    r.push_str("; ");
                                }
                                r.push_str(&g.comment);
                            }
                            r
                        })
                        .unwrap_or_else(|| match (eff, col.oral) {
                            (Some((_, true)), Some(range)) => format!(
                                "aus {} Stundennoten ({})",
                                c.oral_in_range(s.id, range).count(),
                                range.label()
                            ),
                            _ => String::new(),
                        });
                    t.rows.push(vec![
                        TCell::new(&a.name),
                        TCell::new(&col.name),
                        TCell::new(if col.half == Half::H1 { "1." } else { "2." }),
                        TCell::new(format_decimal(col.weight, 1).trim_end_matches(",0")),
                        note,
                        TCell::new(remark).color(GREY),
                    ]);
                }
            }
            t.autosize(&layout.m);
            t.fit_to(layout.content_w());
            layout.table(&t);

            // Zusammenfassung
            let mut cols = vec![TCol::new("", Align::Left).min(110.0)];
            for a in &c.scheme.areas {
                cols.push(TCol::new(
                    format!("Ø {} ({:.0} %)", a.name, pct(a, &c.scheme)),
                    Align::Center,
                ));
            }
            cols.extend([
                TCol::new("Gesamt-Ø", Align::Center),
                TCol::new("Vorschlag", Align::Center),
                TCol::new("Zeugnisnote", Align::Center).min(50.0),
            ]);
            let mut sum = Table::new(cols);
            for (period, halves) in [
                (Period::Halbjahr, vec![Half::H1]),
                (
                    Period::Schuljahr,
                    if c.scheme.year_mode == YearMode::AlleNoten {
                        vec![Half::H1, Half::H2]
                    } else {
                        vec![Half::H2]
                    },
                ),
            ] {
                let mut row = vec![TCell::new(period.label()).bold()];
                for a in &c.scheme.areas {
                    row.push(avg_cell(scale, calc::area_average(c, s.id, a.id, &halves)));
                }
                row.push(avg_cell(scale, calc::period_average(c, s.id, period)).bold());
                row.push(
                    calc::suggestion(c, s.id, period)
                        .map(|v| TCell::new(scale.format_final(v)))
                        .unwrap_or_default(),
                );
                let mut fc = final_cell(c, s.id, period);
                fc.bg = Some(ACCENT_SOFT);
                row.push(fc);
                sum.rows.push(row);
            }
            sum.autosize(&layout.m);
            sum.fit_to(layout.content_w());
            layout.table(&sum);
            for period in [Period::Halbjahr, Period::Schuljahr] {
                if let Some(f) = c.final_grade(s.id, period).filter(|f| f.is_pedagogical()) {
                    layout.paragraph(
                        &format!(
                            "Pädagogische {}: {} statt rechnerisch {}{}",
                            period.label(),
                            scale.format_final_long(f.grade),
                            f.suggestion
                                .map(|v| scale.format_final(v))
                                .unwrap_or("–".into()),
                            if f.reason.is_empty() {
                                String::new()
                            } else {
                                format!(" – {}", f.reason)
                            }
                        ),
                        9.0,
                        ACCENT,
                    );
                }
            }
            layout.space(6.0);
        }
        layout.signatures(&[
            "Datum, Unterschrift Lehrkraft",
            "Kenntnisnahme Erziehungsberechtigte",
        ]);
    }
    l.ok_or_else(|| "Keine Daten".to_string())?.finish()
}

// ---------------------------------------------------------------------------
// 4. Notenspiegel
// ---------------------------------------------------------------------------

fn level_label(scale: ScaleKind, level: u8) -> String {
    let word = scale.word(f64::from(level));
    match scale {
        ScaleKind::Noten => format!("{level} – {word}"),
        ScaleKind::Punkte => {
            let range = match level {
                1 => "15–13",
                2 => "12–10",
                3 => "9–7",
                4 => "6–4",
                5 => "3–1",
                _ => "0",
            };
            format!("{word} ({range} P)")
        }
    }
}

/// Notenspiegel für eine einzelne Leistung (`column`) oder alle Leistungen
/// mit Noten eines Fachs.
pub fn grade_distribution(
    db: &Database,
    course_id: Id,
    column: Option<Id>,
    header: &Header,
) -> Result<Vec<u8>, String> {
    let course = db.course(course_id).ok_or("Fach nicht gefunden")?;
    let class = db.class(course.class_id).ok_or("Klasse nicht gefunden")?;
    let scale = course.scale;
    let cols: Vec<(&Area, &Column)> = course
        .scheme
        .areas
        .iter()
        .flat_map(|a| a.columns.iter().map(move |c| (a, c)))
        .filter(|(_, c)| column.is_none_or(|id| c.id == id))
        .filter(|(_, c)| !course.column_entries(c.id).is_empty())
        .collect();
    if cols.is_empty() {
        return Err("Für diese Auswahl sind noch keine Noten eingetragen.".into());
    }
    let title = match (column, cols.first()) {
        (Some(_), Some((_, c))) => format!("Notenspiegel {}", c.name),
        _ => "Notenspiegel".to_string(),
    };
    let subtitle = format!("{} · {}", course.title(), context(db, class));
    let mut l = Layout::new(&title, &subtitle, header, false);
    for (area, col) in cols {
        let dist = calc::distribution(course, col.id);
        let (avg, n) = calc::column_stats(course, col.id);
        let missing = class.students.len().saturating_sub(n);
        let fehlt = course
            .grades
            .iter()
            .filter(|g| g.column == col.id && g.entry == Entry::Fehlt)
            .count();
        let below = dist[4] + dist[5];
        l.ensure(200.0);
        l.heading(&format!(
            "{} · {} · {}",
            col.name,
            area.name,
            col.half.label()
        ));
        // Balkendiagramm
        let max = dist.iter().copied().max().unwrap_or(1).max(1) as f32;
        let label_w = 150.0;
        let bar_max = l.content_w() - label_w - 80.0;
        for (i, count) in dist.iter().enumerate() {
            let y = l.y;
            let level = i as u8 + 1;
            let color = if level >= 5 { RED } else { ACCENT };
            l.text(
                MARGIN,
                y + 11.0,
                &level_label(scale, level),
                10.0,
                false,
                BLACK,
            );
            l.rect(MARGIN + label_w, y + 2.0, bar_max, 12.0, STRIPE);
            if *count > 0 {
                l.rect(
                    MARGIN + label_w,
                    y + 2.0,
                    bar_max * *count as f32 / max,
                    12.0,
                    color,
                );
            }
            l.text(
                MARGIN + label_w + bar_max + 10.0,
                y + 11.5,
                &format!("{count}"),
                10.5,
                true,
                BLACK,
            );
            l.y += 18.0;
        }
        l.space(6.0);
        let mut info = format!(
            "Teilnehmer: {n} · Durchschnitt: {} · unter ausreichend: {below} ({:.0} %)",
            avg.map(|v| scale.format_avg(v)).unwrap_or("–".into()),
            if n > 0 {
                below as f64 / n as f64 * 100.0
            } else {
                0.0
            }
        );
        if fehlt > 0 || missing > fehlt {
            info.push_str(&format!(" · ohne Note: {}", missing));
        }
        if (col.weight - 1.0).abs() > 1e-9 {
            info.push_str(&format!(
                " · Wertigkeit ×{}",
                format_decimal(col.weight, 1).trim_end_matches(",0")
            ));
        }
        l.paragraph(&info, 9.5, GREY);
        l.space(8.0);
    }
    l.signatures(&[
        "Datum, Unterschrift Lehrkraft",
        "Kenntnisnahme Schulleitung",
    ]);
    l.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Header {
        Header {
            school: "Musterschule".into(),
            teacher: "Frau Beispiel".into(),
            logo: None,
        }
    }

    fn is_pdf(b: &[u8]) -> bool {
        b.starts_with(b"%PDF") && b.len() > 1000
    }

    #[test]
    fn alle_berichte() {
        let db = crate::demo::demo_database();
        let class = db.classes[0].id;
        let course = db.courses[0].id;
        let h = header();
        assert!(is_pdf(&course_list(&db, course, &h).unwrap()));
        assert!(is_pdf(
            &conference(&db, class, Period::Halbjahr, &h).unwrap()
        ));
        assert!(is_pdf(&student_sheets(&db, class, None, None, &h).unwrap()));
        assert!(is_pdf(&grade_distribution(&db, course, None, &h).unwrap()));
        // Punkte-Skala
        let c2 = db.courses[1].id;
        assert!(is_pdf(&course_list(&db, c2, &h).unwrap()));
        assert!(is_pdf(&grade_distribution(&db, c2, None, &h).unwrap()));
    }

    #[test]
    fn logo_png() {
        let png = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../gradecraft/assets/icon-256.png"
        ))
        .unwrap();
        validate_logo(&png).unwrap();
        let db = crate::demo::demo_database();
        let h = Header {
            logo: Some(png),
            ..header()
        };
        assert!(is_pdf(&course_list(&db, db.courses[0].id, &h).unwrap()));
        assert!(validate_logo(b"keinbild").is_err());
    }

    #[test]
    fn textbreite() {
        let m = Metrics::new();
        assert!(m.width("Mappenführung", 10.0, false) > m.width("KA 1", 10.0, false));
        assert!(m.width("Test", 10.0, true) >= m.width("Test", 10.0, false));
        assert_eq!(
            m.fit("Sehr langer Text", 10.0, false, 1000.0),
            "Sehr langer Text"
        );
        assert!(m.fit("Sehr langer Text", 10.0, false, 40.0).ends_with('…'));
    }
}
