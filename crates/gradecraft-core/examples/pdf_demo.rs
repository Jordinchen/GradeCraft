//! Erzeugt alle PDF-Berichte mit Beispieldaten: `cargo run --example pdf_demo -- <ordner>`
use gradecraft_core::{Period, demo, pdf};

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let db = demo::demo_database();
    let logo = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../gradecraft/assets/icon-256.png"
    ))
    .ok();
    let h = pdf::Header {
        school: "Berufskolleg Musterstadt".into(),
        teacher: "Frau Beispiel".into(),
        logo,
    };
    let class = db.classes[0].id;
    let course = db.courses[0].id;
    let out = [
        ("notenliste.pdf", pdf::course_list(&db, course, &h)),
        (
            "konferenz.pdf",
            pdf::conference(&db, class, Period::Halbjahr, &h),
        ),
        (
            "schuelerbogen.pdf",
            pdf::student_sheets(&db, class, Some(db.classes[0].students[3].id), None, &h),
        ),
        (
            "notenspiegel.pdf",
            pdf::grade_distribution(&db, course, None, &h),
        ),
    ];
    for (name, bytes) in out {
        std::fs::write(dir.join(name), bytes.unwrap()).unwrap();
    }
}
