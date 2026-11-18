//! Berechnung von Durchschnitten und Notenvorschlägen.

use crate::model::{Course, Half, Id, Period, YearMode};

/// Gewichteter Mittelwert; Paare mit Gewicht ≤ 0 werden ignoriert.
pub fn weighted_mean(values: impl IntoIterator<Item = (f64, f64)>) -> Option<f64> {
    let (sum, wsum) = values
        .into_iter()
        .filter(|(_, w)| *w > 0.0)
        .fold((0.0, 0.0), |(s, ws), (v, w)| (s + v * w, ws + w));
    (wsum > 0.0).then(|| sum / wsum)
}

/// Durchschnitt eines Leistungsbereichs über die angegebenen Halbjahre.
pub fn area_average(course: &Course, student: Id, area: Id, halves: &[Half]) -> Option<f64> {
    let area = course.scheme.areas.iter().find(|a| a.id == area)?;
    weighted_mean(
        area.columns
            .iter()
            .filter(|c| halves.contains(&c.half))
            .filter_map(|c| {
                course
                    .grade(student, c.id)
                    .and_then(|g| g.entry.value(course.scale))
                    .map(|v| (v, c.weight))
            }),
    )
}

/// Gewichteter Durchschnitt aller Leistungsbereiche.
/// Bereiche ohne Noten werden bei der Gewichtung nicht berücksichtigt.
pub fn combined_average(course: &Course, student: Id, halves: &[Half]) -> Option<f64> {
    weighted_mean(
        course
            .scheme
            .areas
            .iter()
            .filter_map(|a| area_average(course, student, a.id, halves).map(|v| (v, a.weight))),
    )
}

/// Ungerundeter Durchschnitt für eine Zeugnisnote.
pub fn period_average(course: &Course, student: Id, period: Period) -> Option<f64> {
    match period {
        Period::Halbjahr => combined_average(course, student, &[Half::H1]),
        Period::Schuljahr => match course.scheme.year_mode {
            YearMode::AlleNoten => combined_average(course, student, &[Half::H1, Half::H2]),
            YearMode::Halbjahre { w1, w2 } => {
                // Festgelegte Halbjahresnote hat Vorrang vor dem rechnerischen Schnitt.
                let h1 = course
                    .final_grade(student, Period::Halbjahr)
                    .map(|f| f.grade)
                    .or_else(|| combined_average(course, student, &[Half::H1]));
                let h2 = combined_average(course, student, &[Half::H2]);
                weighted_mean(h1.map(|v| (v, w1)).into_iter().chain(h2.map(|v| (v, w2))))
            }
        },
    }
}

/// Gerundeter Notenvorschlag für eine Zeugnisnote.
pub fn suggestion(course: &Course, student: Id, period: Period) -> Option<f64> {
    period_average(course, student, period).map(|a| course.scale.round_final(a))
}

/// Klassenstatistik einer Spalte: (Durchschnitt, Anzahl gezählter Noten).
pub fn column_stats(course: &Course, column: Id) -> (Option<f64>, usize) {
    let vals: Vec<f64> = course
        .grades
        .iter()
        .filter(|g| g.column == column)
        .filter_map(|g| g.entry.value(course.scale))
        .collect();
    let n = vals.len();
    (weighted_mean(vals.into_iter().map(|v| (v, 1.0))), n)
}

/// Notenspiegel einer Spalte: Anzahl je Notenstufe 1–6.
pub fn distribution(course: &Course, column: Id) -> [usize; 6] {
    let mut d = [0; 6];
    for g in course.grades.iter().filter(|g| g.column == column) {
        if let Some(v) = g.entry.value(course.scale) {
            d[(course.scale.level(v) - 1) as usize] += 1;
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::scale::ScaleKind;

    fn course() -> (Database, Id) {
        let mut db = Database::new();
        let scheme = db.default_scheme(4, 4);
        let id = db.alloc_id();
        db.courses.push(Course {
            id,
            class_id: 0,
            subject: "Deutsch".into(),
            short: "D".into(),
            scale: ScaleKind::Noten,
            scheme,
            grades: vec![],
            finals: vec![],
        });
        (db, id)
    }

    #[test]
    fn halbjahr_1_zu_1() {
        let (mut db, cid) = course();
        let c = db.course_mut(cid).unwrap();
        let ka: Vec<Id> = c.scheme.areas[0].columns.iter().map(|c| c.id).collect();
        let so: Vec<Id> = c.scheme.areas[1].columns.iter().map(|c| c.id).collect();
        let s = 99;
        c.set_grade(s, ka[0], Some(Entry::Note(2.0)));
        c.set_grade(s, ka[1], Some(Entry::Note(3.0)));
        c.set_grade(s, so[0], Some(Entry::Note(1.0)));
        c.set_grade(s, so[1], Some(Entry::Fehlt));
        assert_eq!(
            area_average(c, s, c.scheme.areas[0].id, &[Half::H1]),
            Some(2.5)
        );
        assert_eq!(
            area_average(c, s, c.scheme.areas[1].id, &[Half::H1]),
            Some(1.0)
        );
        assert_eq!(period_average(c, s, Period::Halbjahr), Some(1.75));
        assert_eq!(suggestion(c, s, Period::Halbjahr), Some(2.0));
    }

    #[test]
    fn ungenuegend_zaehlt_als_6() {
        let (mut db, cid) = course();
        let c = db.course_mut(cid).unwrap();
        let ka: Vec<Id> = c.scheme.areas[0].columns.iter().map(|c| c.id).collect();
        c.set_grade(1, ka[0], Some(Entry::Note(2.0)));
        c.set_grade(
            1,
            ka[1],
            Some(Entry::Ungenuegend(UngenuegendGrund::Taeuschung)),
        );
        assert_eq!(
            area_average(c, 1, c.scheme.areas[0].id, &[Half::H1]),
            Some(4.0)
        );
    }

    #[test]
    fn schuljahr_modi() {
        let (mut db, cid) = course();
        let c = db.course_mut(cid).unwrap();
        let ka: Vec<Id> = c.scheme.areas[0].columns.iter().map(|c| c.id).collect();
        // H1: KA1 = 2, KA2 = 2 ; H2: KA3 = 4
        c.set_grade(1, ka[0], Some(Entry::Note(2.0)));
        c.set_grade(1, ka[1], Some(Entry::Note(2.0)));
        c.set_grade(1, ka[2], Some(Entry::Note(4.0)));
        c.scheme.year_mode = YearMode::Halbjahre { w1: 1.0, w2: 1.0 };
        assert_eq!(period_average(c, 1, Period::Schuljahr), Some(3.0));
        c.scheme.year_mode = YearMode::AlleNoten;
        assert!((period_average(c, 1, Period::Schuljahr).unwrap() - 8.0 / 3.0).abs() < 1e-9);
        // Pädagogische Halbjahresnote fließt ein
        c.scheme.year_mode = YearMode::Halbjahre { w1: 1.0, w2: 1.0 };
        c.set_final(FinalRecord {
            student: 1,
            period: Period::Halbjahr,
            grade: 1.0,
            suggestion: Some(2.0),
            reason: "x".into(),
        });
        assert_eq!(period_average(c, 1, Period::Schuljahr), Some(2.5));
    }

    #[test]
    fn gewichtung() {
        assert_eq!(weighted_mean([(1.0, 2.0), (4.0, 1.0)]), Some(2.0));
        assert_eq!(weighted_mean([(1.0, 0.0)]), None);
    }
}
