//! UI-Tests der Notentabelle mit simulierten Tastatureingaben.

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gradecraft_core::store::Vault;
use gradecraft_core::{Entry, Period, UngenuegendGrund};

use crate::config::AppConfig;
use crate::session::{Session, View};

fn session() -> Session {
    let dir = std::env::temp_dir().join(format!("gc-ui-{}-{}", std::process::id(), rand_suffix()));
    let db = gradecraft_core::demo::demo_database();
    let vault = Vault::create(dir.join("t.gcdb"), "testtest", &db).unwrap();
    let mut s = Session::new(vault, db);
    let course = s.db.courses[0].id;
    s.open_view(View::Course(course));
    s
}

fn rand_suffix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

fn harness(s: Session) -> Harness<'static, Session> {
    let cfg = AppConfig::default();
    let mut fonts_ready = false;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(
            move |ui, s: &mut Session| {
                if !fonts_ready {
                    // Schriften werden erst im nächsten Frame aktiv
                    crate::theme::install_fonts(ui.ctx());
                    crate::theme::apply(ui.ctx(), crate::theme::ThemeChoice::Hell);
                    fonts_ready = true;
                    return;
                }
                s.course_view(ui, &cfg);
                s.show_dialog(ui.ctx(), &cfg);
            },
            s,
        );
    h.run();
    h
}

fn first_cols(s: &Session) -> (u64, u64, u64) {
    let c = &s.db.courses[0];
    (
        c.scheme.areas[0].columns[0].id,
        c.scheme.areas[0].columns[1].id,
        c.scheme.areas[1].columns[0].id,
    )
}

fn type_text(h: &mut Harness<'static, Session>, t: &str) {
    h.event(egui::Event::Text(t.to_string()));
    h.run();
}

#[test]
fn eingabe_mit_enter_und_tab() {
    let s = session();
    let (ka1, ka2, _) = first_cols(&s);
    let student0 = s.db.classes[0].students[0].id;
    let student1 = s.db.classes[0].students[1].id;
    let mut h = harness(s);

    // Zelle (0,0) auswählen und „2+“ tippen
    h.state_mut().grid.sel = Some((0, 0));
    h.run();
    type_text(&mut h, "2");
    assert!(
        h.state().grid.edit.is_some(),
        "Tippen startet die Bearbeitung"
    );
    type_text(&mut h, "+");
    h.key_press(egui::Key::Enter);
    h.run();
    let g = h.state().db.courses[0].grade(student0, ka1).unwrap().entry;
    assert!(
        matches!(g, Entry::Note(v) if (v - 5.0 / 3.0).abs() < 1e-9),
        "{g:?}"
    );
    assert_eq!(h.state().grid.sel, Some((1, 0)), "Enter springt nach unten");

    // „t“ + Tab → Täuschung, Sprung nach rechts
    type_text(&mut h, "t");
    h.key_press(egui::Key::Tab);
    h.run();
    let g = h.state().db.courses[0].grade(student1, ka1).unwrap().entry;
    assert_eq!(g, Entry::Ungenuegend(UngenuegendGrund::Taeuschung));
    assert_eq!(h.state().grid.sel, Some((1, 1)));

    // Entf löscht
    h.key_press(egui::Key::Delete);
    h.run();
    assert!(h.state().db.courses[0].grade(student1, ka2).is_none());

    // Pfeiltasten
    h.key_press(egui::Key::ArrowUp);
    h.run();
    assert_eq!(h.state().grid.sel, Some((0, 1)));

    // Ungültige Eingabe bleibt im Bearbeitungsmodus
    type_text(&mut h, "9");
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(h.state().grid.error.is_some());
    assert!(h.state().grid.edit.is_some());
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.state().grid.edit.is_none());

    // Rückgängig stellt die gelöschte Note wieder her
    assert!(h.state().undo.len() >= 3);
    let ctx = h.ctx.clone();
    h.state_mut().undo(&ctx);
    assert!(h.state().db.courses[0].grade(student1, ka2).is_some());
}

#[test]
fn vorschlag_bestaetigen_und_paedagogisch() {
    let s = session();
    let student = s.db.classes[0].students[10].id;
    let course = s.db.courses[0].id;
    let mut h = harness(s);
    let ctx = h.ctx.clone();
    h.state_mut().confirm_all(&ctx, course, Period::Halbjahr);
    h.run();
    let c = &h.state().db.courses[0];
    let n = h.state().db.classes[0].students.len();
    assert_eq!(
        c.finals
            .iter()
            .filter(|f| f.period == Period::Halbjahr)
            .count(),
        n
    );
    let f = c.final_grade(student, Period::Halbjahr).unwrap().clone();
    assert!(!f.is_pedagogical());

    // Pädagogischen Dialog öffnen und eine andere Note wählen
    let db = h.state().db.clone();
    h.state_mut().dialog = Some(crate::dialogs::Dialog::final_grade(
        &db,
        course,
        student,
        Period::Halbjahr,
    ));
    h.run();
    let target = if f.grade > 1.0 { f.grade - 1.0 } else { 2.0 };
    h.get_by_label(&format!("{}", target as i64)).click();
    h.run();
    h.get_by_label("Note festlegen").click();
    h.run();
    let f = h.state().db.courses[0]
        .final_grade(student, Period::Halbjahr)
        .unwrap()
        .clone();
    assert_eq!(f.grade, target);
    assert!(f.is_pedagogical());
    assert!(h.state().dialog.is_none());
}
