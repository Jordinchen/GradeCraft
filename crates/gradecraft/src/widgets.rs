//! Ribbon-Bausteine und kleine Hilfs-Widgets.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Rect, Response, Sense, Stroke, Ui, vec2,
};

use crate::theme::{self, Palette};

/// Große Ribbon-Schaltfläche: Icon oben, Beschriftung darunter.
pub fn big_button(ui: &mut Ui, icon: &str, label: &str, enabled: bool) -> Response {
    let p = Palette::get(ui.ctx());
    let galley = ui
        .painter()
        .layout(label.to_string(), FontId::proportional(12.5), p.text, 86.0);
    let width = galley.size().x.max(46.0) + 14.0;
    let (rect, resp) = ui.allocate_exact_size(
        vec2(width, 70.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if ui.is_rect_visible(rect) {
        let hovered = enabled && resp.hovered();
        if hovered {
            ui.painter().rect(
                rect,
                CornerRadius::same(4),
                p.accent_soft,
                Stroke::new(1.0, p.accent_text.gamma_multiply(0.35)),
                egui::StrokeKind::Inside,
            );
        }
        let alpha = if enabled { 1.0 } else { 0.35 };
        ui.painter().text(
            rect.center_top() + vec2(0.0, 20.0),
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(28.0),
            p.accent_text.gamma_multiply(alpha),
        );
        let text_pos = egui::pos2(rect.center().x - galley.size().x / 2.0, rect.top() + 40.0);
        ui.painter().galley_with_override_text_color(
            text_pos,
            galley,
            p.text.gamma_multiply(alpha),
        );
    }
    resp
}

/// Kleine Ribbon-Schaltfläche: Icon links, Text rechts.
pub fn small_button(ui: &mut Ui, icon: &str, label: &str, enabled: bool) -> Response {
    let p = Palette::get(ui.ctx());
    let font = FontId::proportional(13.0);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, p.text);
    let (rect, resp) = ui.allocate_exact_size(
        vec2(galley.size().x + 32.0, 22.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if ui.is_rect_visible(rect) {
        if enabled && resp.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), p.accent_soft);
        }
        let alpha = if enabled { 1.0 } else { 0.35 };
        ui.painter().text(
            rect.left_center() + vec2(13.0, 0.0),
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(16.0),
            p.accent_text.gamma_multiply(alpha),
        );
        ui.painter().galley_with_override_text_color(
            egui::pos2(rect.left() + 26.0, rect.center().y - galley.size().y / 2.0),
            galley,
            p.text.gamma_multiply(alpha),
        );
    }
    resp
}

/// Umschaltbare große Schaltfläche (z. B. Halbjahr-Auswahl).
pub fn big_toggle(ui: &mut Ui, icon: &str, label: &str, selected: bool) -> Response {
    let p = Palette::get(ui.ctx());
    let before = ui.cursor().min;
    let resp = big_button(ui, icon, label, true);
    if selected {
        let rect = Rect::from_min_size(before, resp.rect.size());
        ui.painter().rect(
            rect,
            CornerRadius::same(4),
            Color32::TRANSPARENT,
            Stroke::new(1.5, p.accent_text),
            egui::StrokeKind::Inside,
        );
        ui.painter().rect_filled(
            Rect::from_min_size(
                egui::pos2(rect.left() + 8.0, rect.bottom() - 3.0),
                vec2(rect.width() - 16.0, 2.5),
            ),
            CornerRadius::same(1),
            p.accent_text,
        );
    }
    resp
}

/// Ribbon-Gruppe mit Beschriftung unten und Trennlinie rechts.
pub fn group(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    let p = Palette::get(ui.ctx());
    let top = ui.cursor().min;
    let row = ui
        .vertical(|ui| {
            let r = ui
                .horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.set_min_height(72.0);
                    add(ui);
                })
                .response
                .rect;
            ui.add_space(18.0);
            r
        })
        .inner;
    let caption =
        ui.painter()
            .layout_no_wrap(title.to_string(), FontId::proportional(11.5), p.text_weak);
    let width = row.width().max(caption.size().x + 12.0);
    let rect = Rect::from_min_size(top, vec2(width, 92.0));
    ui.painter().galley(
        egui::pos2(
            rect.center().x - caption.size().x / 2.0,
            rect.bottom() - caption.size().y - 2.0,
        ),
        caption,
        p.text_weak,
    );
    // Platz reservieren (falls Beschriftung breiter als Inhalt)
    if width > row.width() {
        ui.add_space(width - row.width());
    }
    ui.add_space(6.0);
    let x = ui.cursor().min.x;
    ui.painter().line_segment(
        [
            egui::pos2(x, rect.top() + 4.0),
            egui::pos2(x, rect.bottom() - 4.0),
        ],
        Stroke::new(1.0, p.border),
    );
    ui.add_space(8.0);
}

/// Kleine Schaltflächen untereinander stapeln (wie im Office-Ribbon).
pub fn stack(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        add(ui);
    });
}

/// Primäre Schaltfläche in Akzentfarbe.
pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    let btn = egui::Button::new(egui::RichText::new(text).color(Color32::WHITE).strong())
        .fill(theme::ACCENT)
        .corner_radius(CornerRadius::same(4))
        .min_size(vec2(0.0, 28.0));
    ui.add(btn)
}

pub fn secondary_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(egui::Button::new(text).min_size(vec2(0.0, 28.0)))
}

/// Überschrift mit Akzentfarbe.
pub fn section_title(ui: &mut Ui, text: &str) {
    let p = Palette::get(ui.ctx());
    ui.label(
        egui::RichText::new(text)
            .font(theme::bold(16.0))
            .color(p.accent_text),
    );
}

/// Karte (abgerundeter Rahmen auf Blattfarbe).
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::get(ui.ctx());
    egui::Frame::new()
        .fill(p.sheet)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::same(14))
        .show(ui, add)
        .inner
}

/// Zahl mit Komma als Eingabefeld (Gewichtungen).
pub fn weight_edit(ui: &mut Ui, value: &mut f64) -> Response {
    ui.add(
        egui::DragValue::new(value)
            .range(0.0..=100.0)
            .speed(0.05)
            .max_decimals(2)
            .custom_formatter(|v, _| fmt_weight(v))
            .custom_parser(gradecraft_core::scale::parse_decimal),
    )
}

/// Gewicht ohne überflüssige Nachkommastellen: 1 → „1“, 1.5 → „1,5“.
pub fn fmt_weight(v: f64) -> String {
    if v.fract().abs() < 1e-9 {
        format!("{}", v as i64)
    } else {
        gradecraft_core::scale::format_decimal(v, 2)
            .trim_end_matches('0')
            .to_string()
    }
}
