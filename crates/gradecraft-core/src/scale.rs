//! Notenskalen: Eingabe, Darstellung und Rundung.
//!
//! Intern wird jede Note als `f64` gespeichert:
//! * `Noten` (1–6): `2+` = 2 − 1/3, `2-` = 2 + 1/3, `2-3` = 2,5
//! * `Punkte` (0–15): ganze Punkte
//!
//! Neue Skalen werden als weitere Variante von [`ScaleKind`] ergänzt.

use serde::{Deserialize, Serialize};

const THIRD: f64 = 1.0 / 3.0;
const EPS: f64 = 1e-6;
/// Typografisches Minuszeichen für Tendenzen (Anzeige; Eingabe mit „-“).
pub const MINUS: char = '\u{2212}';
/// Halbgeviertstrich für Zwischennoten („2–3“).
pub const EN_DASH: char = '\u{2013}';

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ScaleKind {
    /// Noten 1 (sehr gut) bis 6 (ungenügend) mit Tendenzen.
    #[default]
    Noten,
    /// Punkte 15 (sehr gut plus) bis 0 (ungenügend).
    Punkte,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("Leere Eingabe")]
    Empty,
    #[error("„{0}“ ist keine gültige Note")]
    Invalid(String),
}

impl ScaleKind {
    pub const ALL: [ScaleKind; 2] = [ScaleKind::Noten, ScaleKind::Punkte];

    pub fn label(self) -> &'static str {
        match self {
            ScaleKind::Noten => "Noten 1–6",
            ScaleKind::Punkte => "Punkte 0–15",
        }
    }

    /// Schlechtester Wert der Skala (für „ungenügend“).
    pub fn worst(self) -> f64 {
        match self {
            ScaleKind::Noten => 6.0,
            ScaleKind::Punkte => 0.0,
        }
    }

    pub fn best(self) -> f64 {
        match self {
            ScaleKind::Noten => 1.0,
            ScaleKind::Punkte => 15.0,
        }
    }

    /// Mögliche Zeugnisnoten, beste zuerst.
    pub fn final_grades(self) -> Vec<f64> {
        match self {
            ScaleKind::Noten => (1..=6).map(f64::from).collect(),
            ScaleKind::Punkte => (0..=15).rev().map(f64::from).collect(),
        }
    }

    /// Wandelt eine Benutzereingabe in einen Notenwert um.
    pub fn parse(self, input: &str) -> Result<f64, ParseError> {
        // Typografische Zeichen (−, –) wie das normale Minus behandeln
        let s: String = input
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| if c == MINUS || c == EN_DASH { '-' } else { c })
            .collect();
        if s.is_empty() {
            return Err(ParseError::Empty);
        }
        let invalid = || ParseError::Invalid(input.trim().to_string());
        match self {
            ScaleKind::Noten => {
                // Zwischennote „2-3“
                if let Some((a, b)) = s.split_once('-')
                    && let (Ok(a), Ok(b)) = (a.parse::<u8>(), b.parse::<u8>())
                {
                    if (1..=5).contains(&a) && b == a + 1 {
                        return Ok(f64::from(a) + 0.5);
                    }
                    return Err(invalid());
                }
                let (num, tendency) = match s.chars().last() {
                    Some('+') => (&s[..s.len() - 1], -THIRD),
                    Some('-') | Some('–') => (s.trim_end_matches(['-', '–']), THIRD),
                    _ => (s.as_str(), 0.0),
                };
                let base: u8 = num.parse().map_err(|_| invalid())?;
                if !(1..=6).contains(&base) {
                    return Err(invalid());
                }
                let v = f64::from(base) + tendency;
                if !(1.0 - THIRD - EPS..=6.0 + EPS).contains(&v) {
                    return Err(invalid()); // „6-“ gibt es nicht
                }
                Ok(v)
            }
            ScaleKind::Punkte => {
                let s = s.trim_end_matches(['P', 'p']);
                let p: u8 = s.parse().map_err(|_| invalid())?;
                if p > 15 {
                    return Err(invalid());
                }
                Ok(f64::from(p))
            }
        }
    }

    /// Darstellung einer Einzelnote (so wie sie eingegeben wurde).
    pub fn format(self, v: f64) -> String {
        match self {
            ScaleKind::Noten => {
                let base = v.round();
                let diff = v - base;
                if (diff + THIRD).abs() < 1e-3 {
                    format!("{}+", base as i64)
                } else if (diff - THIRD).abs() < 1e-3 {
                    format!("{}{MINUS}", base as i64)
                } else if diff.abs() < 1e-3 {
                    format!("{}", base as i64)
                } else if ((v - v.floor()) - 0.5).abs() < 1e-3 {
                    format!("{}{EN_DASH}{}", v.floor() as i64, v.floor() as i64 + 1)
                } else {
                    format_decimal(v, 2)
                }
            }
            ScaleKind::Punkte => format!("{}", v.round() as i64),
        }
    }

    /// Darstellung eines Durchschnitts mit zwei Nachkommastellen.
    pub fn format_avg(self, v: f64) -> String {
        format_decimal(v, 2)
    }

    /// Rundet einen Durchschnitt kaufmännisch auf eine Zeugnisnote
    /// (Noten: 2,50 → 3; Punkte: 12,5 → 13).
    pub fn round_final(self, avg: f64) -> f64 {
        let r = (avg + 0.5 + 1e-9).floor();
        r.clamp(self.best().min(self.worst()), self.best().max(self.worst()))
    }

    /// Zeugnisnote als Text, z. B. „2 (gut)“ oder „11 P (gut)“.
    pub fn format_final_long(self, v: f64) -> String {
        match self {
            ScaleKind::Noten => format!("{} ({})", v as i64, self.word(v)),
            ScaleKind::Punkte => format!("{} P ({})", v as i64, self.word(v)),
        }
    }

    pub fn format_final(self, v: f64) -> String {
        format!("{}", v.round() as i64)
    }

    /// Notenbezeichnung in Worten.
    pub fn word(self, v: f64) -> &'static str {
        let note = match self {
            ScaleKind::Noten => v.round() as i64,
            ScaleKind::Punkte => match v.round() as i64 {
                13..=15 => 1,
                10..=12 => 2,
                7..=9 => 3,
                4..=6 => 4,
                1..=3 => 5,
                _ => 6,
            },
        };
        match note {
            1 => "sehr gut",
            2 => "gut",
            3 => "befriedigend",
            4 => "ausreichend",
            5 => "mangelhaft",
            _ => "ungenügend",
        }
    }

    /// Ordnet einen Wert grob einer Notenstufe 1–6 zu (für Farben).
    pub fn level(self, v: f64) -> u8 {
        let n = match self {
            ScaleKind::Noten => v.round(),
            ScaleKind::Punkte => match v.round() as i64 {
                13..=15 => 1.0,
                10..=12 => 2.0,
                7..=9 => 3.0,
                4..=6 => 4.0,
                1..=3 => 5.0,
                _ => 6.0,
            },
        };
        n.clamp(1.0, 6.0) as u8
    }

    pub fn input_hint(self) -> &'static str {
        match self {
            ScaleKind::Noten => "1–6, Tendenzen 2+ / 2-, Zwischennote 2-3",
            ScaleKind::Punkte => "0–15 Punkte",
        }
    }
}

/// Dezimalzahl mit deutschem Komma.
pub fn format_decimal(v: f64, digits: usize) -> String {
    format!("{v:.digits$}").replace('.', ",")
}

/// Liest eine Dezimalzahl mit Komma oder Punkt.
pub fn parse_decimal(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noten_parsen() {
        let n = ScaleKind::Noten;
        assert_eq!(n.parse("2").unwrap(), 2.0);
        assert!((n.parse("2+").unwrap() - 5.0 / 3.0).abs() < 1e-9);
        assert!((n.parse("2-").unwrap() - 7.0 / 3.0).abs() < 1e-9);
        assert_eq!(n.parse("2-3").unwrap(), 2.5);
        assert!(n.parse("6-").is_err());
        assert!(n.parse("7").is_err());
        assert!(n.parse("0").is_err());
        assert!(n.parse("2-4").is_err());
        assert!(n.parse("").is_err());
    }

    #[test]
    fn noten_formatieren() {
        let n = ScaleKind::Noten;
        for (input, shown) in [
            ("1", "1"),
            ("1+", "1+"),
            ("2-", "2\u{2212}"),
            ("3", "3"),
            ("5+", "5+"),
            ("6", "6"),
            ("2-3", "2\u{2013}3"),
        ] {
            assert_eq!(n.format(n.parse(input).unwrap()), shown);
            // Angezeigter Text muss wieder einlesbar sein (Bearbeiten mit F2)
            assert_eq!(n.parse(shown).unwrap(), n.parse(input).unwrap());
        }
    }

    #[test]
    fn punkte() {
        let p = ScaleKind::Punkte;
        assert_eq!(p.parse("13").unwrap(), 13.0);
        assert_eq!(p.parse("0").unwrap(), 0.0);
        assert!(p.parse("16").is_err());
        assert_eq!(p.round_final(12.5), 13.0);
        assert_eq!(p.word(11.0), "gut");
    }

    #[test]
    fn kaufmaennisch_runden() {
        let n = ScaleKind::Noten;
        assert_eq!(n.round_final(2.49), 2.0);
        assert_eq!(n.round_final(2.5), 3.0);
        // 2+ und 3- ergeben genau 2,5
        let avg = (n.parse("2+").unwrap() + n.parse("3-").unwrap()) / 2.0;
        assert!((avg - 2.5).abs() < 1e-9);
        assert_eq!(n.round_final(avg), 3.0);
    }
}
