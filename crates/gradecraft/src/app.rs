//! Hauptanwendung: Startbildschirm (Öffnen/Anlegen/Entsperren) und Arbeitsbereich.

use std::path::PathBuf;

use eframe::egui::{self, RichText, vec2};
use egui_phosphor::regular as ic;
use gradecraft_core::Database;
use gradecraft_core::store::{FILE_EXTENSION, Vault};

use crate::config::{self, AppConfig};
use crate::dialogs::{Dialog, password_strength};
use crate::session::{Session, SessionEvent, View};
use crate::theme::{self, Palette};
use crate::widgets::{primary_button, secondary_button};

#[derive(PartialEq)]
enum StartMode {
    Open,
    Create,
}

#[derive(Clone, Copy, PartialEq)]
enum Location {
    Standard,
    Portable,
    Custom,
}

struct StartState {
    mode: StartMode,
    selected: Option<PathBuf>,
    password: String,
    error: Option<String>,
    new_name: String,
    location: Location,
    custom_path: Option<PathBuf>,
    pw1: String,
    pw2: String,
    focus_pw: bool,
    with_demo: bool,
}

impl StartState {
    fn new(cfg: &AppConfig, selected: Option<PathBuf>) -> Self {
        let selected = selected.or_else(|| cfg.recent_files.iter().find(|p| p.exists()).cloned());
        StartState {
            mode: if selected.is_some() {
                StartMode::Open
            } else {
                StartMode::Create
            },
            focus_pw: selected.is_some(),
            selected,
            password: String::new(),
            error: None,
            new_name: "Notenbuch".into(),
            location: if config::portable_dir().is_some() {
                Location::Portable
            } else {
                Location::Standard
            },
            custom_path: None,
            pw1: String::new(),
            pw2: String::new(),
            with_demo: false,
        }
    }
}

enum Screen {
    Start(StartState),
    Main(Box<Session>),
}

pub struct GradeCraftApp {
    cfg: AppConfig,
    screen: Screen,
    applied_zoom: f32,
    update: crate::updater::UpdateState,
    update_dismissed: bool,
    /// Ergebnis auch anzeigen, wenn kein Update vorliegt (manuelle Suche)
    update_manual: bool,
    #[cfg(debug_assertions)]
    frames: u32,
}

impl GradeCraftApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let cfg = AppConfig::load();
        theme::install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx, cfg.theme);
        let start = StartState::new(&cfg, None);
        #[allow(unused_mut)]
        let mut app = GradeCraftApp {
            cfg,
            screen: Screen::Start(start),
            applied_zoom: 0.0,
            update: crate::updater::UpdateState::Idle,
            update_dismissed: false,
            update_manual: false,
            #[cfg(debug_assertions)]
            frames: 0,
        };
        // Datei aus der Kommandozeile (Doppelklick auf .gcdb im Explorer)
        if let Some(path) = std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .filter(|p| p.is_file())
        {
            app.screen = Screen::Start(StartState::new(&app.cfg, Some(path)));
        }
        if app.cfg.check_updates && std::env::var_os("GRADECRAFT_DEV").is_none() {
            app.update = crate::updater::check_async(cc.egui_ctx.clone());
        }
        #[cfg(debug_assertions)]
        app.dev_setup(&cc.egui_ctx);
        app
    }

    /// Hinweisleiste für verfügbare Updates.
    fn update_banner(&mut self, ui: &mut egui::Ui) {
        use crate::updater::{UpdateState, download_async, run_installer};
        let ctx = ui.ctx().clone();
        let p = Palette::get(&ctx);
        if let Some(installer) = self.update.poll() {
            // Vor der Installation speichern und beenden
            if let Screen::Main(s) = &mut self.screen {
                s.save(&ctx);
            }
            match run_installer(&installer) {
                Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Err(e) => self.update = UpdateState::Failed(e),
            }
        }
        let (text, color, info) = match &self.update {
            UpdateState::Available(info) if !self.update_dismissed => (
                format!(
                    "{} GradeCraft {} ist verfügbar (installiert: {}).",
                    ic::ARROW_CIRCLE_UP,
                    info.version,
                    crate::updater::CURRENT_VERSION
                ),
                p.accent_text,
                Some(info.clone()),
            ),
            UpdateState::Checking(_) if self.update_manual => (
                format!("{} Suche nach Updates …", ic::ARROW_CIRCLE_UP),
                p.text_weak,
                None,
            ),
            UpdateState::Downloading(_) => (
                format!("{} Update wird heruntergeladen …", ic::DOWNLOAD_SIMPLE),
                p.accent_text,
                None,
            ),
            UpdateState::Failed(e) if !self.update_dismissed && self.update_manual => {
                (format!("{} Update-Prüfung: {e}", ic::WARNING), p.warn, None)
            }
            UpdateState::UpToDate if !self.update_dismissed && self.update_manual => (
                format!(
                    "{} GradeCraft ist auf dem neuesten Stand.",
                    ic::CHECK_CIRCLE
                ),
                p.good,
                None,
            ),
            _ => return,
        };
        egui::Panel::top("update-banner")
            .frame(
                egui::Frame::new()
                    .fill(p.accent_soft)
                    .inner_margin(egui::Margin::symmetric(12, 6)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(text).color(color).strong());
                    if let Some(info) = &info {
                        if !info.notes.trim().is_empty() {
                            ui.label("ⓘ")
                                .on_hover_text(info.notes.chars().take(1200).collect::<String>());
                        }
                        match &info.setup_url {
                            Some(url) if cfg!(windows) => {
                                if primary_button(ui, "Jetzt aktualisieren").clicked() {
                                    if let Screen::Main(s) = &mut self.screen {
                                        s.save(&ctx);
                                    }
                                    self.update_manual = true;
                                    self.update = download_async(ctx.clone(), url.clone());
                                }
                            }
                            _ => {
                                if primary_button(ui, "Download-Seite öffnen").clicked() {
                                    ctx.open_url(egui::OpenUrl::new_tab(&info.page_url));
                                }
                            }
                        }
                    }
                    if !matches!(self.update, UpdateState::Downloading(_))
                        && ui.button("Ausblenden").clicked()
                    {
                        self.update_dismissed = true;
                    }
                });
            });
    }

    /// Nur für Entwicklung: `GRADECRAFT_SHOT=datei.ppm` speichert nach einigen Frames einen Screenshot und beendet.
    #[cfg(debug_assertions)]
    fn dev_screenshot(&mut self, ctx: &egui::Context) {
        let Ok(path) = std::env::var("GRADECRAFT_SHOT") else {
            return;
        };
        self.frames += 1;
        ctx.request_repaint();
        if self.frames == 40 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let mut out = format!("P6\n{} {}\n255\n", img.size[0], img.size[1]).into_bytes();
            for px in &img.pixels {
                out.extend_from_slice(&[px.r(), px.g(), px.b()]);
            }
            let _ = std::fs::write(&path, out);
            std::process::exit(0);
        }
    }

    /// Nur für Entwicklung/Screenshots: `GRADECRAFT_DEV="view=course;tab=h1;theme=dark"`.
    #[cfg(debug_assertions)]
    fn dev_setup(&mut self, ctx: &egui::Context) {
        use crate::session::{BackstagePage, CourseTab, RibbonTab};
        let Ok(spec) = std::env::var("GRADECRAFT_DEV") else {
            return;
        };
        let opts: std::collections::HashMap<&str, &str> = spec
            .split(';')
            .filter_map(|kv| kv.split_once('='))
            .collect();
        if let Some(t) = opts.get("theme") {
            self.cfg.theme = if *t == "dark" {
                theme::ThemeChoice::Dunkel
            } else {
                theme::ThemeChoice::Hell
            };
            theme::apply(ctx, self.cfg.theme);
        }
        // Ohne Fenstermanager ist das Fenster nie „aktiv“ – Sichtschutz nur auf Wunsch
        self.cfg.privacy_on_blur = opts.contains_key("privacy");
        if opts.get("view") == Some(&"start") {
            return;
        }
        let path = std::env::temp_dir()
            .join("gradecraft-dev")
            .join("Demo-Notenbuch.gcdb");
        let _ = std::fs::remove_file(&path);
        let db = gradecraft_core::demo::demo_database();
        let vault = Vault::create(&path, "demo1234", &db).expect("dev vault");
        let mut s = Session::new(vault, db);
        let class = s.db.classes[0].id;
        let course = s.db.courses[0].id;
        match opts.get("view").copied().unwrap_or("course") {
            "class" => s.open_view(View::Class(class)),
            "welcome" => {}
            "backstage" => s.backstage = Some(BackstagePage::Info),
            "years" => s.backstage = Some(BackstagePage::Years),
            "options" => s.backstage = Some(BackstagePage::Settings),
            _ => s.open_view(View::Course(course)),
        }
        match opts.get("tab").copied() {
            Some("h2") => s.course_tab = CourseTab::H2,
            Some("schema") => s.course_tab = CourseTab::Schema,
            Some("oral") => s.course_tab = CourseTab::Oral,
            _ => {}
        }
        match opts.get("ribbon").copied() {
            Some("klasse") => s.tab = RibbonTab::Klasse,
            Some("bewertung") => s.tab = RibbonTab::Bewertung,
            Some("ansicht") => s.tab = RibbonTab::Ansicht,
            Some("berichte") => s.tab = RibbonTab::Berichte,
            _ => {}
        }
        if opts.get("dialog") == Some(&"final") {
            let student = s.db.classes[0].students[3].id;
            s.dialog = Some(Dialog::final_grade(
                &s.db,
                course,
                student,
                gradecraft_core::Period::Halbjahr,
            ));
        }
        if opts.get("dialog") == Some(&"import") {
            s.dialog = Some(Dialog::Import {
                class,
                text: "Nachname;Vorname;Geschlecht\nMüller;Anna;w\nSchmidt;Ben;m".into(),
            });
        }
        if opts.get("variant") == Some(&"praxis") {
            let id = s.db.alloc_id();
            let c = s.db.course_mut(course).unwrap();
            for a in &mut c.scheme.areas {
                a.weight = 2.0;
                a.columns.retain(|c| c.half == gradecraft_core::Half::H1);
            }
            c.scheme.areas.push(gradecraft_core::Area {
                id,
                name: "Praxis".into(),
                weight: 1.0,
                columns: vec![],
            });
            let cid = s.db.alloc_id();
            s.db.course_mut(course).unwrap().scheme.areas[2]
                .columns
                .push(gradecraft_core::Column {
                    id: cid,
                    name: "Projekt".into(),
                    weight: 1.0,
                    half: gradecraft_core::Half::H2,
                    oral: None,
                });
        }
        if opts.contains_key("dist") {
            let col = s.db.courses[0].scheme.areas[0].columns[0].id;
            s.open_distribution(course, col);
        }
        if opts.contains_key("embed") {
            ctx.set_embed_viewports(true);
        }
        if opts.contains_key("review") {
            let st = s.db.classes[0].students[3].id;
            s.open_review(class, st, Some(course));
        }
        if opts.contains_key("hide") {
            s.privacy_hidden = true;
        }
        if let Some(sel) = opts.get("sel") {
            let (r, c) = sel.split_once(',').unwrap_or(("0", "0"));
            s.grid.sel = Some((r.parse().unwrap_or(0), c.parse().unwrap_or(0)));
        }
        self.screen = Screen::Main(Box::new(s));
    }

    fn open_session(&mut self, ctx: &egui::Context, vault: Vault, db: Database) {
        let path = vault.path().to_path_buf();
        // Sicherung der Datei, so wie sie vor dieser Sitzung war
        if let Err(e) = vault.backup(db.settings.backup_count) {
            eprintln!("Sicherung fehlgeschlagen: {e}");
        }
        self.cfg.add_recent(&path);
        let mut s = Session::new(vault, db);
        if s.db.classes_of_year(s.year).next().is_none() {
            s.view = View::Welcome;
        }
        s.notify(ctx, "Datei entsperrt", false);
        self.screen = Screen::Main(Box::new(s));
    }

    fn start_screen(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let p = Palette::get(&ctx);
        let Screen::Start(st) = &mut self.screen else {
            return;
        };
        let mut opened: Option<(Vault, Database)> = None;

        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.canvas)).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(((ui.available_height() - 560.0) / 2.0).max(24.0));
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(ic::GRADUATION_CAP).size(56.0).color(p.accent_text));
                    ui.label(RichText::new("GradeCraft").font(theme::bold(34.0)).color(p.accent_text));
                    ui.label(RichText::new("Notenverwaltung – lokal & verschlüsselt").color(p.text_weak));
                    ui.add_space(20.0);
                });
                let w = 760.0_f32.min(ui.available_width() - 32.0);
                ui.horizontal(|ui| {
                    ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                    egui::Frame::new()
                        .fill(p.sheet)
                        .stroke(egui::Stroke::new(1.0, p.border))
                        .corner_radius(egui::CornerRadius::same(10))
                        .inner_margin(egui::Margin::same(24))
                        .show(ui, |ui| {
                          ui.vertical(|ui| {
                            ui.set_width(w - 48.0);
                            ui.horizontal(|ui| {
                                if crate::grid::pill(ui, &format!("{} Datei öffnen", ic::LOCK_OPEN), st.mode == StartMode::Open).clicked() {
                                    st.mode = StartMode::Open;
                                    st.error = None;
                                }
                                if crate::grid::pill(ui, &format!("{} Neue Notendatei", ic::FILE_PLUS), st.mode == StartMode::Create).clicked() {
                                    st.mode = StartMode::Create;
                                    st.error = None;
                                }
                            });
                            ui.add_space(14.0);
                            match st.mode {
                                StartMode::Open => open_form(ui, &self.cfg, st, &mut opened),
                                StartMode::Create => create_form(ui, st, &mut opened),
                            }
                            if let Some(e) = &st.error {
                                ui.add_space(8.0);
                                ui.label(RichText::new(format!("{} {e}", ic::WARNING_CIRCLE)).color(p.bad));
                            }
                          });
                        });
                });
                ui.add_space(14.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(format!("{} Alle Daten bleiben auf diesem Gerät. Verschlüsselung: AES-256 · Argon2id", ic::SHIELD_CHECK))
                            .size(12.0)
                            .color(p.text_weak),
                    );
                });
            });
        });

        if let Some((vault, db)) = opened {
            self.open_session(&ctx, vault, db);
        }
    }
}

fn open_form(
    ui: &mut egui::Ui,
    cfg: &AppConfig,
    st: &mut StartState,
    opened: &mut Option<(Vault, Database)>,
) {
    let p = Palette::get(ui.ctx());
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        ui.label(RichText::new("Zuletzt verwendet").strong());
        ui.add_space(4.0);
        let recent: Vec<&PathBuf> = cfg.recent_files.iter().filter(|p| p.exists()).collect();
        if recent.is_empty() {
            ui.label(RichText::new("Keine zuletzt verwendeten Dateien.").color(p.text_weak));
        }
        for path in recent {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let sel = st.selected.as_ref() == Some(path);
            let resp = ui.add(
                egui::Button::new(RichText::new(format!("{} {name}", ic::FILE_LOCK)).strong())
                    .selected(sel)
                    .min_size(vec2(ui.available_width(), 30.0)),
            );
            ui.label(
                RichText::new(
                    path.parent()
                        .map(|d| d.display().to_string())
                        .unwrap_or_default(),
                )
                .size(11.0)
                .color(p.text_weak),
            );
            if resp.clicked() {
                st.selected = Some(path.clone());
                st.error = None;
                st.focus_pw = true;
            }
        }
        ui.add_space(8.0);
        if secondary_button(ui, &format!("{} Andere Datei öffnen …", ic::FOLDER_OPEN)).clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("GradeCraft-Datei", &[FILE_EXTENSION])
                .set_directory(config::default_data_dir())
                .pick_file()
        {
            st.selected = Some(path);
            st.error = None;
            st.focus_pw = true;
        }

        let ui = &mut cols[1];
        match st.selected.clone() {
            None => {
                ui.label(RichText::new("Wähle links eine Datei aus.").color(p.text_weak));
            }
            Some(path) => {
                ui.label(RichText::new(format!("{} Entsperren", ic::LOCK)).font(theme::bold(16.0)));
                ui.label(
                    RichText::new(
                        path.file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default(),
                    )
                    .color(p.text_weak),
                );
                ui.add_space(8.0);
                ui.label("Passwort");
                let r = ui.add(
                    egui::TextEdit::singleline(&mut st.password)
                        .password(true)
                        .desired_width(f32::INFINITY),
                );
                if std::mem::take(&mut st.focus_pw) {
                    r.request_focus();
                }
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.add_space(8.0);
                if primary_button(ui, &format!("{}  Entsperren", ic::LOCK_OPEN)).clicked() || enter
                {
                    match Vault::open(&path, &st.password) {
                        Ok(v) => {
                            st.password.clear();
                            *opened = Some(v);
                        }
                        Err(e) => {
                            st.error = Some(e.to_string());
                            st.password.clear();
                            st.focus_pw = true;
                        }
                    }
                }
            }
        }
    });
}

fn create_form(ui: &mut egui::Ui, st: &mut StartState, opened: &mut Option<(Vault, Database)>) {
    let p = Palette::get(ui.ctx());
    egui::Grid::new("create")
        .num_columns(2)
        .spacing([14.0, 10.0])
        .show(ui, |ui| {
            ui.label("Dateiname");
            ui.add(egui::TextEdit::singleline(&mut st.new_name).desired_width(260.0));
            ui.end_row();
            ui.label("Speicherort");
            ui.vertical(|ui| {
                ui.radio_value(
                    &mut st.location,
                    Location::Standard,
                    format!("Standardordner ({})", config::default_data_dir().display()),
                );
                let portable = std::env::current_exe()
                    .ok()
                    .and_then(|e| e.parent().map(|d| d.join("Daten")));
                if let Some(pd) = &portable {
                    ui.radio_value(
                        &mut st.location,
                        Location::Portable,
                        format!("Portabel neben dem Programm ({})", pd.display()),
                    );
                }
                ui.horizontal(|ui| {
                    ui.radio_value(
                        &mut st.location,
                        Location::Custom,
                        "Anderer Ort (z. B. USB-Stick)",
                    );
                    if st.location == Location::Custom
                        && ui.button(format!("{} Wählen …", ic::FOLDER_OPEN)).clicked()
                    {
                        st.custom_path = rfd::FileDialog::new().pick_folder();
                    }
                });
                if st.location == Location::Custom {
                    ui.label(
                        RichText::new(
                            st.custom_path
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or("Kein Ordner gewählt".into()),
                        )
                        .size(12.0)
                        .color(p.text_weak),
                    );
                }
            });
            ui.end_row();
            ui.label("Passwort");
            ui.vertical(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut st.pw1)
                        .password(true)
                        .desired_width(260.0),
                );
                password_strength(ui, &st.pw1);
            });
            ui.end_row();
            ui.label("Wiederholen");
            ui.add(
                egui::TextEdit::singleline(&mut st.pw2)
                    .password(true)
                    .desired_width(260.0),
            );
            ui.end_row();
        });
    ui.add_space(6.0);
    ui.label(
        RichText::new(format!("{} Das Passwort kann nicht wiederhergestellt werden. Ohne Passwort sind die Noten verloren – bitte sicher notieren.", ic::WARNING))
            .size(12.0)
            .color(p.warn),
    );
    ui.checkbox(
        &mut st.with_demo,
        "Mit Beispieldaten füllen (zum Ausprobieren)",
    );
    ui.add_space(10.0);
    if primary_button(ui, &format!("{}  Notendatei anlegen", ic::FILE_PLUS)).clicked() {
        let dir = match st.location {
            Location::Standard => Some(config::default_data_dir()),
            Location::Portable => std::env::current_exe()
                .ok()
                .and_then(|e| e.parent().map(|d| d.join("Daten"))),
            Location::Custom => st.custom_path.clone(),
        };
        let name = st
            .new_name
            .trim()
            .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        st.error = if name.is_empty() {
            Some("Bitte einen Dateinamen angeben.".into())
        } else if dir.is_none() {
            Some("Bitte einen Speicherort wählen.".into())
        } else if st.pw1.chars().count() < 8 {
            Some("Das Passwort muss mindestens 8 Zeichen haben.".into())
        } else if st.pw1 != st.pw2 {
            Some("Die Passwörter stimmen nicht überein.".into())
        } else {
            None
        };
        if st.error.is_none() {
            let path = dir.unwrap().join(format!("{name}.{FILE_EXTENSION}"));
            if path.exists() {
                st.error = Some(format!("Die Datei {} existiert bereits.", path.display()));
            } else {
                let db = if st.with_demo {
                    gradecraft_core::demo::demo_database()
                } else {
                    Database::new()
                };
                match Vault::create(&path, &st.pw1, &db) {
                    Ok(v) => {
                        st.pw1.clear();
                        st.pw2.clear();
                        *opened = Some((v, db));
                    }
                    Err(e) => st.error = Some(e.to_string()),
                }
            }
        }
    }
}

impl eframe::App for GradeCraftApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        #[cfg(debug_assertions)]
        self.dev_screenshot(&ctx);
        if (self.applied_zoom - self.cfg.zoom).abs() > 1e-3 {
            ctx.set_zoom_factor(self.cfg.zoom);
            self.applied_zoom = self.cfg.zoom;
        }
        let p = Palette::get(&ctx);

        // Manuelle Update-Suche aus dem Menü
        if let Screen::Main(s) = &mut self.screen
            && std::mem::take(&mut s.request_update_check)
        {
            self.update_dismissed = false;
            self.update_manual = true;
            self.update = crate::updater::check_async(ctx.clone());
        }
        if matches!(self.update, crate::updater::UpdateState::Checking(_)) {
            self.update.poll();
        }
        self.update_banner(ui);

        if let Screen::Start(_) = self.screen {
            if ctx.input(|i| i.viewport().close_requested()) {
                return;
            }
            self.start_screen(ui);
            return;
        }

        let Screen::Main(session) = &mut self.screen else {
            return;
        };
        let s: &mut Session = session;
        let cfg = &mut self.cfg;

        // Fenster schließen: speichern bzw. nachfragen
        if ctx.input(|i| i.viewport().close_requested()) && !s.close_after_save && s.dirty {
            if cfg.autosave {
                s.save(&ctx);
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                s.dialog = Some(Dialog::UnsavedOnExit);
            }
        }
        if s.close_after_save {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        s.tick(&ctx, cfg);
        let title = format!(
            "{} – GradeCraft",
            s.vault
                .path()
                .file_stem()
                .map(|x| x.to_string_lossy().to_string())
                .unwrap_or_default()
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        egui::Panel::top("titlebar")
            .frame(
                egui::Frame::new()
                    .fill(p.chrome)
                    .inner_margin(egui::Margin::symmetric(4, 2)),
            )
            .show_separator_line(false)
            .show(ui, |ui| s.title_bar(ui, cfg));
        let tabs_bottom = egui::Panel::top("tabs")
            .frame(egui::Frame::new().fill(p.chrome))
            .show_separator_line(false)
            .show(ui, |ui| s.tab_bar(ui))
            .response
            .rect
            .bottom();

        if s.backstage.is_some() {
            s.backstage(ui, cfg);
        } else {
            egui::Panel::top("ribbon")
                .frame(
                    egui::Frame::new()
                        .fill(p.chrome)
                        .inner_margin(egui::Margin {
                            left: 8,
                            right: 8,
                            top: 0,
                            bottom: 6,
                        }),
                )
                .show_separator_line(false)
                .show(ui, |ui| {
                    egui::Frame::new()
                        .fill(p.ribbon)
                        .stroke(egui::Stroke::new(1.0, p.border))
                        .inner_margin(egui::Margin {
                            left: 6,
                            right: 6,
                            top: 6,
                            bottom: 2,
                        })
                        .corner_radius(egui::CornerRadius::same(8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            s.ribbon(ui, cfg)
                        });
                });
            egui::Panel::bottom("status")
                .frame(
                    egui::Frame::new()
                        .fill(p.chrome)
                        .inner_margin(egui::Margin::symmetric(4, 2)),
                )
                .show(ui, |ui| s.status_bar(ui, cfg));
            if cfg.show_nav {
                egui::Panel::left("nav")
                    .default_size(270.0)
                    .size_range(200.0..=420.0)
                    .resizable(true)
                    .frame(
                        egui::Frame::new()
                            .fill(p.chrome)
                            .inner_margin(egui::Margin {
                                left: 12,
                                right: 10,
                                top: 0,
                                bottom: 8,
                            }),
                    )
                    .show(ui, |ui| s.navigation(ui));
            }
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(p.canvas)
                        .inner_margin(egui::Margin::same(18)),
                )
                .show(ui, |ui| match s.view {
                    View::Welcome => s.welcome_view(ui),
                    View::Class(_) => s.class_view(ui, cfg),
                    View::Course(_) => s.course_view(ui, cfg),
                });
        }

        s.distribution_windows(&ctx);
        s.review_window(&ctx, cfg);
        s.show_dialog(&ctx, cfg);

        // Sichtschutz: Noten verdecken, wenn das Fenster nicht aktiv ist oder auf Wunsch
        let unfocused = cfg.privacy_on_blur && ctx.input(|i| i.viewport().focused) == Some(false);
        if unfocused || s.privacy_hidden {
            privacy_overlay(&ctx, tabs_bottom, s, unfocused);
        }

        match std::mem::replace(&mut s.event, SessionEvent::None) {
            SessionEvent::None => {}
            ev => {
                if s.dirty {
                    s.save(&ctx);
                }
                let path = s.vault.path().to_path_buf();
                let selected = matches!(ev, SessionEvent::Lock).then_some(path);
                // Session (inkl. Schlüssel) wird hier verworfen
                self.screen = Screen::Start(StartState::new(&self.cfg, selected));
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Screen::Main(s) = &mut self.screen
            && s.dirty
            && (self.cfg.autosave || s.close_after_save)
        {
            let _ = s.vault.save(&s.db);
        }
        self.cfg.save();
    }
}

/// Deckt alles unterhalb der Registerkarten ab (über Dialogen und Fenstern).
fn privacy_overlay(ctx: &egui::Context, top: f32, s: &mut Session, unfocused: bool) {
    let p = Palette::get(ctx);
    let screen = ctx.content_rect();
    let rect = egui::Rect::from_min_max(egui::pos2(screen.left(), top), screen.max);
    egui::Area::new(egui::Id::new("privacy-overlay"))
        .order(egui::Order::Tooltip)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(rect.size(), egui::Sense::click());
            ui.painter().rect_filled(r, 0.0, p.canvas);
            let c = r.center();
            ui.painter().text(
                c - egui::vec2(0.0, 40.0),
                egui::Align2::CENTER_CENTER,
                ic::EYE_SLASH,
                egui::FontId::proportional(56.0),
                p.accent_text,
            );
            ui.painter().text(
                c + egui::vec2(0.0, 10.0),
                egui::Align2::CENTER_CENTER,
                "Noten ausgeblendet",
                theme::bold(22.0),
                p.text,
            );
            let hint = if unfocused {
                "GradeCraft ist nicht das aktive Fenster. Zum Anzeigen ins Fenster klicken."
            } else {
                "Zum Anzeigen klicken oder F9 drücken."
            };
            ui.painter().text(
                c + egui::vec2(0.0, 40.0),
                egui::Align2::CENTER_CENTER,
                hint,
                egui::FontId::proportional(14.0),
                p.text_weak,
            );
            if resp.clicked() && !unfocused {
                s.privacy_hidden = false;
            }
        });
}
