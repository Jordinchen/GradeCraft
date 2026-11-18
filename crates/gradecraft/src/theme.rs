//! Farben und Stil (Office-ähnlich, Akzent #880088).

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke, Theme,
};
use serde::{Deserialize, Serialize};

pub const ACCENT: Color32 = Color32::from_rgb(0x88, 0x00, 0x88);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeChoice {
    #[default]
    System,
    Hell,
    Dunkel,
}

impl ThemeChoice {
    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::System => "Wie System",
            ThemeChoice::Hell => "Hell",
            ThemeChoice::Dunkel => "Dunkel",
        }
    }
}

/// Farbpalette, abhängig vom aktuellen Hell/Dunkel-Modus.
#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// Titelleiste, Registerkarten
    pub chrome: Color32,
    /// Ribbon-Fläche
    pub ribbon: Color32,
    /// Arbeitsfläche hinter dem „Blatt“
    pub canvas: Color32,
    /// Blatt / Tabelle
    pub sheet: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_weak: Color32,
    /// Akzent für Text/Icons (im Dunkelmodus aufgehellt)
    pub accent_text: Color32,
    /// Hintergrund für Auswahl und Hover
    pub accent_soft: Color32,
    pub header: Color32,
    pub stripe: Color32,
    pub avg_bg: Color32,
    pub final_bg: Color32,
    pub bad: Color32,
    pub good: Color32,
    pub warn: Color32,
}

impl Palette {
    pub fn get(ctx: &egui::Context) -> Self {
        if ctx.global_style().visuals.dark_mode {
            Self::dark()
        } else {
            Self::light()
        }
    }

    pub fn light() -> Self {
        Self {
            dark: false,
            chrome: Color32::from_rgb(0xF3, 0xF2, 0xF1),
            ribbon: Color32::from_rgb(0xFB, 0xFA, 0xF9),
            canvas: Color32::from_rgb(0xE8, 0xE6, 0xE4),
            sheet: Color32::WHITE,
            border: Color32::from_rgb(0xD6, 0xD3, 0xD0),
            text: Color32::from_rgb(0x20, 0x1F, 0x1E),
            text_weak: Color32::from_rgb(0x60, 0x5E, 0x5C),
            accent_text: ACCENT,
            accent_soft: Color32::from_rgb(0xF4, 0xE3, 0xF4),
            header: Color32::from_rgb(0xF6, 0xF1, 0xF6),
            stripe: Color32::from_rgb(0xFA, 0xF9, 0xFA),
            avg_bg: Color32::from_rgb(0xF3, 0xF0, 0xF5),
            final_bg: Color32::from_rgb(0xF0, 0xE2, 0xF0),
            bad: Color32::from_rgb(0xC4, 0x2B, 0x1C),
            good: Color32::from_rgb(0x10, 0x7C, 0x10),
            warn: Color32::from_rgb(0xB0, 0x6A, 0x00),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            chrome: Color32::from_rgb(0x1F, 0x1F, 0x1F),
            ribbon: Color32::from_rgb(0x29, 0x28, 0x29),
            canvas: Color32::from_rgb(0x16, 0x16, 0x16),
            sheet: Color32::from_rgb(0x24, 0x23, 0x24),
            border: Color32::from_rgb(0x3E, 0x3C, 0x3E),
            text: Color32::from_rgb(0xEE, 0xEC, 0xEE),
            text_weak: Color32::from_rgb(0xA8, 0xA4, 0xA8),
            accent_text: Color32::from_rgb(0xE0, 0x8A, 0xE0),
            accent_soft: Color32::from_rgb(0x48, 0x22, 0x48),
            header: Color32::from_rgb(0x30, 0x2A, 0x30),
            stripe: Color32::from_rgb(0x28, 0x27, 0x28),
            avg_bg: Color32::from_rgb(0x2E, 0x2A, 0x31),
            final_bg: Color32::from_rgb(0x3A, 0x26, 0x3A),
            bad: Color32::from_rgb(0xFF, 0x7B, 0x6E),
            good: Color32::from_rgb(0x6C, 0xCB, 0x5F),
            warn: Color32::from_rgb(0xF0, 0xB0, 0x40),
        }
    }

    /// Textfarbe für eine Notenstufe 1–6.
    pub fn grade_color(&self, level: u8) -> Color32 {
        match level {
            5 | 6 => self.bad,
            _ => self.text,
        }
    }
}

fn visuals_for(p: Palette) -> egui::Visuals {
    let mut v = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = p.chrome;
    v.window_fill = p.sheet;
    v.extreme_bg_color = if p.dark {
        Color32::from_rgb(0x1A, 0x1A, 0x1A)
    } else {
        Color32::WHITE
    };
    v.faint_bg_color = p.stripe;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    v.selection.bg_fill = if p.dark {
        Color32::from_rgb(0x6A, 0x1E, 0x6A)
    } else {
        Color32::from_rgb(0xE6, 0xC4, 0xE6)
    };
    v.selection.stroke = Stroke::new(1.0, p.accent_text);
    v.hyperlink_color = p.accent_text;
    v.override_text_color = Some(p.text);
    let r = CornerRadius::same(4);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.bg_fill = if p.dark {
        Color32::from_rgb(0x33, 0x32, 0x33)
    } else {
        Color32::WHITE
    };
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.hovered.weak_bg_fill = p.accent_soft;
    v.widgets.hovered.bg_fill = p.accent_soft;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent_text.gamma_multiply(0.6));
    v.widgets.active.weak_bg_fill = p.accent_soft;
    v.widgets.active.bg_fill = p.accent_soft;
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent_text);
    v.widgets.open.weak_bg_fill = p.accent_soft;
    v.text_cursor.stroke = Stroke::new(2.0, p.accent_text);
    v
}

pub fn apply(ctx: &egui::Context, choice: ThemeChoice) {
    ctx.set_visuals_of(Theme::Light, visuals_for(Palette::light()));
    ctx.set_visuals_of(Theme::Dark, visuals_for(Palette::dark()));
    for theme in [Theme::Light, Theme::Dark] {
        ctx.style_mut_of(theme, |s| {
            s.spacing.item_spacing = egui::vec2(6.0, 4.0);
            s.spacing.button_padding = egui::vec2(8.0, 3.0);
            s.spacing.interact_size.y = 24.0;
            s.text_styles
                .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
            s.text_styles
                .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
            s.text_styles
                .insert(egui::TextStyle::Small, egui::FontId::proportional(11.5));
            s.text_styles
                .insert(egui::TextStyle::Heading, egui::FontId::proportional(22.0));
        });
    }
    ctx.set_theme(match choice {
        ThemeChoice::System => egui::ThemePreference::System,
        ThemeChoice::Hell => egui::ThemePreference::Light,
        ThemeChoice::Dunkel => egui::ThemePreference::Dark,
    });
}

/// Schriften: Segoe UI unter Windows (falls vorhanden), sonst egui-Standard,
/// plus Phosphor-Icons.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let candidates: &[(&str, &str)] = &[
        ("segoe", "C:\\Windows\\Fonts\\segoeui.ttf"),
        ("segoe_bold", "C:\\Windows\\Fonts\\segoeuib.ttf"),
        ("system", "/System/Library/Fonts/SFNS.ttf"),
        ("system", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
    ];
    let mut regular_loaded = false;
    for (name, path) in candidates {
        if regular_loaded && !name.ends_with("_bold") {
            continue;
        }
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert((*name).to_string(), FontData::from_owned(bytes).into());
            if name.ends_with("_bold") {
                fonts
                    .families
                    .insert(FontFamily::Name("bold".into()), vec![(*name).to_string()]);
            } else {
                fonts
                    .families
                    .entry(FontFamily::Proportional)
                    .or_default()
                    .insert(0, (*name).to_string());
                regular_loaded = true;
            }
        }
    }
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    // „bold“-Familie muss immer existieren; Fallback auf normale Schrift
    let prop = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let bold = fonts
        .families
        .entry(FontFamily::Name("bold".into()))
        .or_default();
    bold.extend(prop);
    ctx.set_fonts(fonts);
}

pub fn bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, FontFamily::Name("bold".into()))
}
