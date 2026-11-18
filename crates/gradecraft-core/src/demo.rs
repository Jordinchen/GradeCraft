//! Beispieldaten zum Ausprobieren (frei erfundene Namen).

use crate::model::*;
use crate::scale::ScaleKind;

const NAMES: &[(&str, &str, Gender)] = &[
    ("Albers", "Lena", Gender::W),
    ("Becker", "Jonas", Gender::M),
    ("Celik", "Elif", Gender::W),
    ("Dahlmann", "Noah", Gender::M),
    ("Engel", "Mia", Gender::W),
    ("Fischer", "Paul", Gender::M),
    ("Grabowski", "Sam", Gender::D),
    ("Hoffmann", "Emma", Gender::W),
    ("Ilić", "Luka", Gender::M),
    ("Jansen", "Hannah", Gender::W),
    ("Kaya", "Emir", Gender::M),
    ("Lorenz", "Sophie", Gender::W),
    ("Meier", "Ben", Gender::M),
    ("Nowak", "Lea", Gender::W),
    ("Okafor", "Daniel", Gender::M),
    ("Peters", "Marie", Gender::W),
    ("Quast", "Felix", Gender::M),
    ("Richter", "Clara", Gender::W),
    ("Schulz", "Leon", Gender::M),
    ("Teichmann", "Alex", Gender::Ohne),
    ("Usta", "Zeynep", Gender::W),
    ("Vogel", "Finn", Gender::M),
    ("Weber", "Ida", Gender::W),
    ("Yilmaz", "Can", Gender::M),
];

/// Einfacher deterministischer Zufallsgenerator.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f64) / ((1u64 << 31) as f64)
    }
}

pub fn demo_database() -> Database {
    let mut db = Database::new();
    let year = db.school_years[0].id;
    let mut rng = Lcg(42);

    // Klasse 9b, Deutsch
    let class_id = db.alloc_id();
    let students: Vec<Student> = NAMES
        .iter()
        .map(|(l, f, g)| Student {
            id: db.alloc_id(),
            last_name: l.to_string(),
            first_name: f.to_string(),
            gender: *g,
        })
        .collect();
    db.classes.push(SchoolClass {
        id: class_id,
        year_id: year,
        name: "9b".into(),
        students: students.clone(),
    });
    let scheme = db.default_scheme(4, 4);
    let course_id = db.alloc_id();
    let mut course = Course {
        id: course_id,
        class_id,
        subject: "Deutsch".into(),
        short: "D".into(),
        scale: ScaleKind::Noten,
        scheme,
        grades: vec![],
        finals: vec![],
        oral: vec![],
        oral_scheme: Default::default(),
    };
    let cols: Vec<(Id, Half)> = course
        .scheme
        .areas
        .iter()
        .flat_map(|a| a.columns.iter().map(|c| (c.id, c.half)))
        .collect();
    for (i, s) in students.iter().enumerate() {
        let level = 1.6 + (i as f64 * 0.37) % 3.4;
        for (j, (col, half)) in cols.iter().enumerate() {
            if *half == Half::H2 && j % 2 == 1 {
                continue; // 2. Halbjahr erst teilweise erfasst
            }
            let r = rng.next();
            let entry = if i == 6 && j == 1 {
                Entry::Ungenuegend(UngenuegendGrund::Taeuschung)
            } else if i == 15 && j == 4 {
                Entry::Ungenuegend(UngenuegendGrund::Unentschuldigt)
            } else if r < 0.03 {
                Entry::Fehlt
            } else {
                let v = (level + (rng.next() - 0.5) * 1.8).clamp(1.0, 6.0);
                let base = v.round();
                let t = ((v - base) * 3.0).round() / 3.0;
                Entry::Note((base + t).clamp(2.0 / 3.0, 6.0))
            };
            course.set_grade(s.id, *col, Some(entry));
        }
    }
    // einige Halbjahresnoten festlegen
    for s in students.iter().take(6) {
        if let Some(g) = crate::calc::suggestion(&course, s.id, Period::Halbjahr) {
            course.set_final(FinalRecord {
                student: s.id,
                period: Period::Halbjahr,
                grade: g,
                suggestion: Some(g),
                reason: String::new(),
            });
        }
    }
    if let Some(s) = students.get(3) {
        let sug = crate::calc::suggestion(&course, s.id, Period::Halbjahr);
        course.set_final(FinalRecord {
            student: s.id,
            period: Period::Halbjahr,
            grade: sug.map(|g| (g - 1.0).max(1.0)).unwrap_or(2.0),
            suggestion: sug,
            reason: "Deutliche Leistungssteigerung im Verlauf des Halbjahres".into(),
        });
    }
    // Stundenerfassung: SoMi 2 wird automatisch aus mündlichen Noten gebildet
    {
        use chrono::{Datelike, NaiveDate};
        let start: i32 = db.school_years[0].name[..4].parse().unwrap_or(2026);
        let from = NaiveDate::from_ymd_opt(start, 8, 1).unwrap();
        let to = NaiveDate::from_ymd_opt(start + 1, 1, 31).unwrap();
        let somi2 = course.scheme.areas[1].columns[1].id;
        course.scheme.areas[1].columns[1].oral = Some(OralRange { from, to });
        course.grades.retain(|g| g.column != somi2);
        let mut lesson = NaiveDate::from_ymd_opt(start, 9, 1).unwrap();
        for _ in 0..10 {
            for (i, s) in students.iter().enumerate() {
                if rng.next() < 0.45 {
                    let level = (i as f64 * 0.37) % 3.4;
                    let idx = ((level + (rng.next() - 0.5) * 2.0).round().clamp(0.0, 4.0)) as usize;
                    course.set_oral(s.id, lesson, Some(OralValue::Mark(OralMark::ALL[idx])));
                }
            }
            lesson += chrono::Duration::days(if lesson.weekday() == chrono::Weekday::Fri {
                4
            } else {
                7
            });
        }
    }
    db.courses.push(course);

    // Klasse IFK126, Punkte
    let class2 = db.alloc_id();
    let students2: Vec<Student> = NAMES
        .iter()
        .skip(4)
        .take(14)
        .map(|(l, f, g)| Student {
            id: db.alloc_id(),
            last_name: l.to_string(),
            first_name: f.to_string(),
            gender: *g,
        })
        .collect();
    db.classes.push(SchoolClass {
        id: class2,
        year_id: year,
        name: "IFK126".into(),
        students: students2.clone(),
    });
    let scheme = db.default_scheme(2, 2);
    let id = db.alloc_id();
    let mut c2 = Course {
        id,
        class_id: class2,
        subject: "Wirtschaftsinformatik".into(),
        short: "WI".into(),
        scale: ScaleKind::Punkte,
        scheme,
        grades: vec![],
        finals: vec![],
        oral: vec![],
        oral_scheme: Default::default(),
    };
    let cols: Vec<Id> = c2
        .scheme
        .areas
        .iter()
        .flat_map(|a| {
            a.columns
                .iter()
                .filter(|c| c.half == Half::H1)
                .map(|c| c.id)
        })
        .collect();
    for s in &students2 {
        for col in &cols {
            c2.set_grade(
                s.id,
                *col,
                Some(Entry::Note((rng.next() * 12.0 + 3.0).round())),
            );
        }
    }
    db.courses.push(c2);
    let id = db.alloc_id();
    let scheme = db.default_scheme(4, 4);
    db.courses.push(Course {
        id,
        class_id,
        subject: "Geschichte".into(),
        short: "GE".into(),
        scale: ScaleKind::Noten,
        scheme,
        grades: vec![],
        finals: vec![],
        oral: vec![],
        oral_scheme: Default::default(),
    });
    db
}

#[cfg(test)]
mod tests {
    #[test]
    fn demo_ist_konsistent() {
        let db = super::demo_database();
        assert_eq!(db.classes.len(), 2);
        assert!(
            db.courses
                .iter()
                .all(|c| c.grades.iter().all(|g| match g.entry {
                    crate::Entry::Note(v) => c.scale.parse(&c.scale.format(v)).is_ok(),
                    _ => true,
                }))
        );
    }
}
