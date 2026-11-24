//! Einfacher CSV-Import (Schülerlisten) und -Export (Notenlisten).
//!
//! Export: Semikolon-getrennt, UTF-8 mit BOM, Dezimalkomma – öffnet sich in
//! Excel/LibreOffice direkt korrekt.

use crate::calc;
use crate::model::{Course, Gender, Half, Period, SchoolClass};
use crate::scale::format_decimal;

/// Ein importierter Datensatz (noch ohne ID).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedStudent {
    pub last_name: String,
    pub first_name: String,
    pub gender: Gender,
}

fn split_line(line: &str, delim: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            c if c == delim && !quoted => out.push(std::mem::take(&mut cur).trim().to_string()),
            c => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

/// Liest Schüler aus CSV-Text oder aus der Zwischenablage (z. B. aus Excel kopiert).
///
/// Erkannt werden Trennzeichen `;`, Tab und `,`. Spalten werden über eine
/// Kopfzeile (Nachname/Vorname/Geschlecht) zugeordnet, sonst gilt die
/// Reihenfolge Nachname, Vorname, Geschlecht. Eine einzelne Spalte der Form
/// „Nachname, Vorname“ wird ebenfalls verstanden.
pub fn parse_students(text: &str) -> Vec<ImportedStudent> {
    let text = text.trim_start_matches('\u{feff}');
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let Some(first) = lines.first() else {
        return Vec::new();
    };
    let delim = [';', '\t', ',']
        .into_iter()
        .max_by_key(|d| first.matches(*d).count())
        .filter(|d| first.contains(*d))
        .unwrap_or(';');

    let header = split_line(first, delim);
    let find = |names: &[&str]| {
        header.iter().position(|h| {
            let h = h.to_lowercase();
            names.iter().any(|n| h.contains(n))
        })
    };
    let fn_ = find(&["vorname", "rufname"]);
    let ln = find(&["nachname", "familienname"]).or_else(|| {
        header
            .iter()
            .enumerate()
            .position(|(i, h)| Some(i) != fn_ && h.to_lowercase().contains("name"))
    });
    let gd = find(&["geschlecht", "gender"]);
    let has_header = ln.is_some() && (fn_.is_some() || gd.is_some());
    let (ln, fn_, gd) = if has_header {
        (ln, fn_, gd)
    } else {
        (Some(0), Some(1), Some(2))
    };

    lines
        .iter()
        .skip(usize::from(has_header))
        .filter_map(|l| {
            let cols = split_line(l, delim);
            let get = |i: Option<usize>| i.and_then(|i| cols.get(i)).cloned().unwrap_or_default();
            let mut last = get(ln);
            let mut first = get(fn_);
            if first.is_empty()
                && delim != ','
                && let Some((a, b)) = last.clone().split_once(',')
            {
                last = a.trim().to_string();
                first = b.trim().to_string();
            }
            (!last.is_empty()).then(|| ImportedStudent {
                last_name: last,
                first_name: first,
                gender: Gender::parse(&get(gd)),
            })
        })
        .collect()
}

fn esc(s: &str) -> String {
    // Schutz vor Formel-Einschleusung („CSV injection“): Excel und LibreOffice
    // werten Zellen, die mit = + - @ (oder Tab/CR) beginnen, als Formel aus.
    let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{s}")
    } else {
        s.to_string()
    };
    if s.contains([';', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

/// Exportiert die komplette Notenliste eines Fachs.
pub fn export_course(class: &SchoolClass, course: &Course) -> String {
    let scale = course.scale;
    let mut head = vec![
        "Nachname".to_string(),
        "Vorname".into(),
        "Geschlecht".into(),
    ];
    for half in [Half::H1, Half::H2] {
        for area in &course.scheme.areas {
            for col in area.columns.iter().filter(|c| c.half == half) {
                head.push(format!(
                    "{} – {} ({})",
                    area.name,
                    col.name,
                    if half == Half::H1 { "1. HJ" } else { "2. HJ" }
                ));
            }
            head.push(format!(
                "Ø {} ({})",
                area.name,
                if half == Half::H1 { "1. HJ" } else { "2. HJ" }
            ));
        }
        if half == Half::H1 {
            head.extend([
                "Ø Halbjahr".into(),
                "Vorschlag Halbjahr".into(),
                "Halbjahresnote".into(),
                "Begründung Halbjahr".into(),
            ]);
        }
    }
    head.extend([
        "Ø Schuljahr".into(),
        "Vorschlag Schuljahr".into(),
        "Schuljahresnote".into(),
        "Begründung Schuljahr".into(),
    ]);

    let mut out = String::from("\u{feff}");
    out.push_str(&head.iter().map(|h| esc(h)).collect::<Vec<_>>().join(";"));
    out.push_str("\r\n");

    let avg = |v: Option<f64>| v.map(|v| format_decimal(v, 2)).unwrap_or_default();
    for s in &class.students {
        let mut row = vec![
            s.last_name.clone(),
            s.first_name.clone(),
            s.gender.short().to_string(),
        ];
        for half in [Half::H1, Half::H2] {
            for area in &course.scheme.areas {
                for col in area.columns.iter().filter(|c| c.half == half) {
                    row.push(
                        course
                            .grade(s.id, col.id)
                            .map(|g| g.entry.format(scale))
                            .unwrap_or_default(),
                    );
                }
                row.push(avg(calc::area_average(course, s.id, area.id, &[half])));
            }
            if half == Half::H1 {
                push_period(&mut row, course, s.id, Period::Halbjahr);
            }
        }
        push_period(&mut row, course, s.id, Period::Schuljahr);
        out.push_str(&row.iter().map(|c| esc(c)).collect::<Vec<_>>().join(";"));
        out.push_str("\r\n");
    }
    out
}

fn push_period(row: &mut Vec<String>, course: &Course, student: crate::model::Id, period: Period) {
    let scale = course.scale;
    row.push(
        calc::period_average(course, student, period)
            .map(|v| format_decimal(v, 2))
            .unwrap_or_default(),
    );
    row.push(
        calc::suggestion(course, student, period)
            .map(|v| scale.format_final(v))
            .unwrap_or_default(),
    );
    let f = course.final_grade(student, period);
    row.push(f.map(|f| scale.format_final(f.grade)).unwrap_or_default());
    row.push(f.map(|f| f.reason.clone()).unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mit_kopfzeile() {
        let s = parse_students("Vorname;Nachname;Geschlecht\nAnna;Müller;w\nBen;Schmidt;m\n");
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].last_name, "Müller");
        assert_eq!(s[0].first_name, "Anna");
        assert_eq!(s[0].gender, Gender::W);
    }

    #[test]
    fn ohne_kopfzeile_tab() {
        let s = parse_students("Müller\tAnna\tw\nSchmidt\tBen\n");
        assert_eq!(s[1].first_name, "Ben");
        assert_eq!(s[1].gender, Gender::Ohne);
    }

    #[test]
    fn formeln_entschaerft() {
        assert_eq!(esc("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(esc("+49"), "'+49");
        assert_eq!(esc("-1"), "'-1");
        assert_eq!(esc("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(esc("Müller"), "Müller");
        assert_eq!(esc("2\u{2212}"), "2\u{2212}"); // Note mit typografischem Minus bleibt
    }

    #[test]
    fn eine_spalte() {
        let s = parse_students("Müller, Anna\nSchmidt, Ben\n");
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].last_name, "Müller");
        assert_eq!(s[0].first_name, "Anna");
    }
}
