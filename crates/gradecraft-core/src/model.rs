//! Datenmodell von GradeCraft.

use serde::{Deserialize, Serialize};

use crate::scale::ScaleKind;

pub type Id = u64;

pub const DB_FORMAT_VERSION: u32 = 1;

/// Gesamter Datenbestand einer GradeCraft-Datei.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Database {
    pub format_version: u32,
    pub next_id: Id,
    pub school_years: Vec<SchoolYear>,
    pub classes: Vec<SchoolClass>,
    pub courses: Vec<Course>,
    pub templates: Vec<Template>,
    #[serde(default)]
    pub settings: DbSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DbSettings {
    /// Anzahl aufbewahrter Sicherungskopien.
    pub backup_count: usize,
    /// Automatisch sperren nach so vielen Minuten Inaktivität (0 = nie).
    pub auto_lock_minutes: u32,
    /// Für den Kopf von PDF-Dokumenten.
    pub school_name: String,
    pub teacher_name: String,
    /// Schullogo (PNG oder JPEG), Base64 in der Datei.
    #[serde(with = "base64_opt")]
    pub logo: Option<Vec<u8>>,
}

impl Default for DbSettings {
    fn default() -> Self {
        Self {
            backup_count: 10,
            auto_lock_minutes: 15,
            school_name: String::new(),
            teacher_name: String::new(),
            logo: None,
        }
    }
}

/// Binärdaten kompakt als Base64 statt als Zahlenliste speichern.
mod base64_opt {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(b) => s.serialize_some(&STANDARD.encode(b)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        let s: Option<String> = Option::deserialize(d)?;
        s.map(|s| STANDARD.decode(s).map_err(serde::de::Error::custom))
            .transpose()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchoolYear {
    pub id: Id,
    /// z. B. „2026/27“
    pub name: String,
    /// Archivierte Schuljahre sind schreibgeschützt.
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchoolClass {
    pub id: Id,
    pub year_id: Id,
    /// z. B. „9b“ oder „IFK126“
    pub name: String,
    pub students: Vec<Student>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Gender {
    M,
    W,
    D,
    #[default]
    Ohne,
}

impl Gender {
    pub const ALL: [Gender; 4] = [Gender::M, Gender::W, Gender::D, Gender::Ohne];

    pub fn short(self) -> &'static str {
        match self {
            Gender::M => "m",
            Gender::W => "w",
            Gender::D => "d",
            Gender::Ohne => "–",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Gender::M => "männlich (m)",
            Gender::W => "weiblich (w)",
            Gender::D => "divers (d)",
            Gender::Ohne => "ohne Angabe",
        }
    }

    pub fn parse(s: &str) -> Gender {
        match s.trim().to_lowercase().as_str() {
            "m" | "männlich" | "maennlich" | "male" => Gender::M,
            "w" | "weiblich" | "f" | "female" => Gender::W,
            "d" | "divers" | "x" => Gender::D,
            _ => Gender::Ohne,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Student {
    pub id: Id,
    pub last_name: String,
    pub first_name: String,
    pub gender: Gender,
}

impl Student {
    pub fn display_name(&self) -> String {
        if self.first_name.is_empty() {
            self.last_name.clone()
        } else {
            format!("{}, {}", self.last_name, self.first_name)
        }
    }
}

/// Ein Fach in einer Klasse (Lerngruppe).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Course {
    pub id: Id,
    pub class_id: Id,
    /// z. B. „Deutsch“
    pub subject: String,
    /// z. B. „D“
    pub short: String,
    pub scale: ScaleKind,
    pub scheme: Scheme,
    pub grades: Vec<GradeRecord>,
    pub finals: Vec<FinalRecord>,
    /// Mündliche Noten aus der Stundenerfassung.
    #[serde(default)]
    pub oral: Vec<OralEntry>,
    /// Umrechnung der Kürzel ++ / + / o / − / −− in Noten.
    #[serde(default)]
    pub oral_scheme: OralScheme,
}

impl Course {
    pub fn title(&self) -> String {
        if self.short.is_empty() {
            self.subject.clone()
        } else {
            format!("{} ({})", self.subject, self.short)
        }
    }

    /// Note einer Zelle, wie sie zählt: manuelle Note, sonst automatisch aus
    /// Stundennoten (zweiter Wert `true`).
    pub fn effective_entry(&self, student: Id, column: Id) -> Option<(Entry, bool)> {
        if let Some(g) = self.grade(student, column) {
            return Some((g.entry, false));
        }
        let (_, col) = self.find_column(column)?;
        let range = col.oral?;
        self.oral_average(student, range)
            .map(|v| (Entry::Note(v), true))
    }

    /// Stundennoten eines Schülers im Zeitraum.
    pub fn oral_in_range(&self, student: Id, range: OralRange) -> impl Iterator<Item = &OralEntry> {
        self.oral
            .iter()
            .filter(move |o| o.student == student && range.contains(o.date))
    }

    /// Durchschnitt der Stundennoten im Zeitraum.
    pub fn oral_average(&self, student: Id, range: OralRange) -> Option<f64> {
        crate::calc::weighted_mean(
            self.oral_in_range(student, range)
                .map(|o| (self.oral_scheme.value(o.value), 1.0)),
        )
    }

    /// Stundennote eines Schülers an einem Tag (höchstens eine pro Tag).
    pub fn oral_on(&self, student: Id, date: chrono::NaiveDate) -> Option<&OralEntry> {
        self.oral
            .iter()
            .find(|o| o.student == student && o.date == date)
    }

    pub fn set_oral(&mut self, student: Id, date: chrono::NaiveDate, value: Option<OralValue>) {
        let pos = self
            .oral
            .iter()
            .position(|o| o.student == student && o.date == date);
        match (pos, value) {
            (Some(i), Some(v)) => self.oral[i].value = v,
            (Some(i), None) => {
                self.oral.remove(i);
            }
            (None, Some(v)) => self.oral.push(OralEntry {
                student,
                date,
                value: v,
                comment: String::new(),
            }),
            (None, None) => {}
        }
    }

    /// Alle zählenden Einträge einer Spalte (manuell und automatisch).
    pub fn column_entries(&self, column: Id) -> Vec<(Id, Entry)> {
        let mut out: Vec<(Id, Entry)> = self
            .grades
            .iter()
            .filter(|g| g.column == column)
            .map(|g| (g.student, g.entry))
            .collect();
        if let Some((_, col)) = self.find_column(column)
            && let Some(range) = col.oral
        {
            let mut students: Vec<Id> = self
                .oral
                .iter()
                .filter(|o| range.contains(o.date))
                .map(|o| o.student)
                .collect();
            students.sort_unstable();
            students.dedup();
            for s in students {
                if !out.iter().any(|(id, _)| *id == s)
                    && let Some(v) = self.oral_average(s, range)
                {
                    out.push((s, Entry::Note(v)));
                }
            }
        }
        out
    }

    pub fn grade(&self, student: Id, column: Id) -> Option<&GradeRecord> {
        self.grades
            .iter()
            .find(|g| g.student == student && g.column == column)
    }

    /// Setzt oder entfernt (bei `None`) eine Einzelnote.
    pub fn set_grade(&mut self, student: Id, column: Id, entry: Option<Entry>) {
        let pos = self
            .grades
            .iter()
            .position(|g| g.student == student && g.column == column);
        match (pos, entry) {
            (Some(i), Some(e)) => self.grades[i].entry = e,
            (Some(i), None) => {
                self.grades.remove(i);
            }
            (None, Some(e)) => self.grades.push(GradeRecord {
                student,
                column,
                entry: e,
                comment: String::new(),
            }),
            (None, None) => {}
        }
    }

    pub fn final_grade(&self, student: Id, period: Period) -> Option<&FinalRecord> {
        self.finals
            .iter()
            .find(|f| f.student == student && f.period == period)
    }

    pub fn set_final(&mut self, record: FinalRecord) {
        self.finals
            .retain(|f| !(f.student == record.student && f.period == record.period));
        self.finals.push(record);
    }

    pub fn clear_final(&mut self, student: Id, period: Period) {
        self.finals
            .retain(|f| !(f.student == student && f.period == period));
    }

    pub fn find_column(&self, column: Id) -> Option<(&Area, &Column)> {
        self.scheme
            .areas
            .iter()
            .find_map(|a| a.columns.iter().find(|c| c.id == column).map(|c| (a, c)))
    }
}

/// Aufbau der Leistungsbewertung eines Fachs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scheme {
    pub areas: Vec<Area>,
    pub year_mode: YearMode,
}

/// Leistungsbereich, z. B. „Schriftliche Arbeiten“ oder „Sonstige Leistungen“.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Area {
    pub id: Id,
    pub name: String,
    pub weight: f64,
    pub columns: Vec<Column>,
}

/// Eine einzelne Leistung, z. B. „KA 1“, „Referat“, „Mündlich Q1“.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub id: Id,
    pub name: String,
    pub weight: f64,
    pub half: Half,
    /// Wenn gesetzt, ergibt sich die Note automatisch aus den Stundennoten
    /// dieses Zeitraums (eine manuell eingetragene Note hat Vorrang).
    #[serde(default)]
    pub oral: Option<OralRange>,
}

/// Zeitraum, aus dem eine Spalte ihre Note aus Stundennoten bildet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OralRange {
    pub from: chrono::NaiveDate,
    pub to: chrono::NaiveDate,
}

impl OralRange {
    pub fn contains(&self, d: chrono::NaiveDate) -> bool {
        self.from <= d && d <= self.to
    }

    pub fn label(&self) -> String {
        format!(
            "{} – {}",
            self.from.format("%d.%m.%Y"),
            self.to.format("%d.%m.%Y")
        )
    }
}

/// Kürzel der Stundenerfassung.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OralMark {
    PlusPlus,
    Plus,
    Neutral,
    Minus,
    MinusMinus,
}

impl OralMark {
    pub const ALL: [OralMark; 5] = [
        OralMark::PlusPlus,
        OralMark::Plus,
        OralMark::Neutral,
        OralMark::Minus,
        OralMark::MinusMinus,
    ];

    pub fn label(self) -> &'static str {
        match self {
            OralMark::PlusPlus => "++",
            OralMark::Plus => "+",
            OralMark::Neutral => "o",
            OralMark::Minus => "\u{2212}",
            OralMark::MinusMinus => "\u{2212}\u{2212}",
        }
    }

    pub fn index(self) -> usize {
        OralMark::ALL.iter().position(|m| *m == self).unwrap()
    }
}

/// Inhalt einer Stundennote: Kürzel oder direkt eine Note.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum OralValue {
    Mark(OralMark),
    Note(f64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OralEntry {
    pub student: Id,
    pub date: chrono::NaiveDate,
    pub value: OralValue,
    #[serde(default)]
    pub comment: String,
}

/// Notenwerte für ++, +, o, −, −− (je Fach einstellbar).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OralScheme {
    pub values: [f64; 5],
}

impl Default for OralScheme {
    fn default() -> Self {
        OralScheme {
            values: [1.0, 2.0, 3.0, 4.0, 5.0],
        }
    }
}

impl OralScheme {
    pub fn for_scale(scale: ScaleKind) -> Self {
        match scale {
            ScaleKind::Noten => OralScheme::default(),
            ScaleKind::Punkte => OralScheme {
                values: [14.0, 11.0, 8.0, 5.0, 2.0],
            },
        }
    }

    pub fn value(&self, v: OralValue) -> f64 {
        match v {
            OralValue::Mark(m) => self.values[m.index()],
            OralValue::Note(n) => n,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Half {
    H1,
    H2,
}

impl Half {
    pub fn label(self) -> &'static str {
        match self {
            Half::H1 => "1. Halbjahr",
            Half::H2 => "2. Halbjahr",
        }
    }
}

/// Wie die Schuljahresnote gebildet wird.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum YearMode {
    /// Gewichtung der Halbjahresnote (1. HJ) und des Durchschnitts im 2. HJ.
    Halbjahre { w1: f64, w2: f64 },
    /// Alle Einzelnoten des Schuljahres fließen in die Leistungsbereiche ein.
    AlleNoten,
}

impl Default for YearMode {
    fn default() -> Self {
        YearMode::Halbjahre { w1: 1.0, w2: 1.0 }
    }
}

/// Zeitraum einer Zeugnisnote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Period {
    Halbjahr,
    Schuljahr,
}

impl Period {
    pub fn label(self) -> &'static str {
        match self {
            Period::Halbjahr => "Halbjahresnote",
            Period::Schuljahr => "Schuljahresnote",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UngenuegendGrund {
    /// Täuschungsversuch
    Taeuschung,
    /// unentschuldigte Nichtteilnahme
    Unentschuldigt,
}

/// Inhalt einer Notenzelle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Entry {
    /// Reguläre Note (auch „ungenügend wegen schlechter Leistung“).
    Note(f64),
    /// Ungenügend aus besonderem Grund – zählt wie die schlechteste Note.
    Ungenuegend(UngenuegendGrund),
    /// Fehlt (entschuldigt / nicht erbracht) – zählt nicht in den Schnitt.
    Fehlt,
}

impl Entry {
    /// Wert für die Durchschnittsberechnung (`None` = zählt nicht).
    pub fn value(self, scale: ScaleKind) -> Option<f64> {
        match self {
            Entry::Note(v) => Some(v),
            Entry::Ungenuegend(_) => Some(scale.worst()),
            Entry::Fehlt => None,
        }
    }

    /// Eingabe-Syntax: Note, „T“ (Täuschung), „U“ (unentschuldigt), „F“ oder „-“ (fehlt).
    pub fn parse(input: &str, scale: ScaleKind) -> Result<Option<Entry>, crate::scale::ParseError> {
        let t = input.trim();
        if t.is_empty() {
            return Ok(None);
        }
        let upper = t.to_uppercase();
        let base = scale.worst() as i64;
        let tt = upper.trim_start_matches(&base.to_string()).trim();
        match tt {
            "T" => return Ok(Some(Entry::Ungenuegend(UngenuegendGrund::Taeuschung))),
            "U" => return Ok(Some(Entry::Ungenuegend(UngenuegendGrund::Unentschuldigt))),
            _ => {}
        }
        match upper.as_str() {
            "F" | "-" | "–" | "−" | "K" => Ok(Some(Entry::Fehlt)),
            _ => scale.parse(t).map(|v| Some(Entry::Note(v))),
        }
    }

    pub fn format(self, scale: ScaleKind) -> String {
        match self {
            Entry::Note(v) => scale.format(v),
            Entry::Ungenuegend(UngenuegendGrund::Taeuschung) => {
                format!("{} T", scale.format(scale.worst()))
            }
            Entry::Ungenuegend(UngenuegendGrund::Unentschuldigt) => {
                format!("{} U", scale.format(scale.worst()))
            }
            Entry::Fehlt => "f".to_string(),
        }
    }

    pub fn describe(self, scale: ScaleKind) -> String {
        match self {
            Entry::Note(v) if v == scale.worst() => {
                "ungenügend wegen schlechter Leistung".to_string()
            }
            Entry::Note(v) => format!("{} – {}", scale.format(v), scale.word(v)),
            Entry::Ungenuegend(UngenuegendGrund::Taeuschung) => {
                "ungenügend wegen Täuschung".to_string()
            }
            Entry::Ungenuegend(UngenuegendGrund::Unentschuldigt) => {
                "ungenügend wegen unentschuldigter Nichtteilnahme".to_string()
            }
            Entry::Fehlt => "fehlt – zählt nicht zum Durchschnitt".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradeRecord {
    pub student: Id,
    pub column: Id,
    pub entry: Entry,
    #[serde(default)]
    pub comment: String,
}

/// Festgelegte Zeugnisnote (bestätigter Vorschlag oder pädagogische Note).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalRecord {
    pub student: Id,
    pub period: Period,
    pub grade: f64,
    /// Rechnerischer Vorschlag zum Zeitpunkt der Festlegung.
    pub suggestion: Option<f64>,
    /// Begründung (bei pädagogischer Notenbildung).
    pub reason: String,
}

impl FinalRecord {
    pub fn is_pedagogical(&self) -> bool {
        self.suggestion
            .is_none_or(|s| (s - self.grade).abs() > 1e-9)
    }
}

/// Vorlage für Leistungsbereiche.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub id: Id,
    pub name: String,
    pub scale: ScaleKind,
    pub scheme: Scheme,
}

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}

impl Database {
    pub fn new() -> Self {
        let mut db = Database {
            format_version: DB_FORMAT_VERSION,
            next_id: 1,
            school_years: Vec::new(),
            classes: Vec::new(),
            courses: Vec::new(),
            templates: Vec::new(),
            settings: DbSettings::default(),
        };
        let year = db.alloc_id();
        db.school_years.push(SchoolYear {
            id: year,
            name: current_school_year_name(),
            archived: false,
        });
        let scheme = db.default_scheme(4, 4);
        let id = db.alloc_id();
        db.templates.push(Template {
            id,
            name: "Standard 1:1 (2 KA je Halbjahr)".into(),
            scale: ScaleKind::Noten,
            scheme,
        });
        db
    }

    pub fn alloc_id(&mut self) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Standardschema: schriftlich 1 : sonstige 1.
    pub fn default_scheme(&mut self, written: usize, other: usize) -> Scheme {
        let mut areas = Vec::new();
        let a_id = self.alloc_id();
        let mut cols = Vec::new();
        for i in 0..written {
            let half = if i < written.div_ceil(2) {
                Half::H1
            } else {
                Half::H2
            };
            let id = self.alloc_id();
            cols.push(Column {
                id,
                name: format!("KA {}", i + 1),
                weight: 1.0,
                half,
                oral: None,
            });
        }
        areas.push(Area {
            id: a_id,
            name: "Schriftliche Arbeiten".into(),
            weight: 1.0,
            columns: cols,
        });
        let a_id = self.alloc_id();
        let mut cols = Vec::new();
        for i in 0..other {
            let half = if i < other.div_ceil(2) {
                Half::H1
            } else {
                Half::H2
            };
            let id = self.alloc_id();
            cols.push(Column {
                id,
                name: format!("SoMi {}", i + 1),
                weight: 1.0,
                half,
                oral: None,
            });
        }
        areas.push(Area {
            id: a_id,
            name: "Sonstige Leistungen".into(),
            weight: 1.0,
            columns: cols,
        });
        Scheme {
            areas,
            year_mode: YearMode::default(),
        }
    }

    /// Kopiert ein Schema und vergibt dabei neue IDs.
    pub fn clone_scheme(&mut self, scheme: &Scheme) -> Scheme {
        let areas = scheme
            .areas
            .iter()
            .map(|a| Area {
                id: self.alloc_id(),
                name: a.name.clone(),
                weight: a.weight,
                columns: a
                    .columns
                    .iter()
                    .map(|c| Column {
                        id: self.alloc_id(),
                        name: c.name.clone(),
                        weight: c.weight,
                        half: c.half,
                        oral: None,
                    })
                    .collect(),
            })
            .collect();
        Scheme {
            areas,
            year_mode: scheme.year_mode,
        }
    }

    pub fn year(&self, id: Id) -> Option<&SchoolYear> {
        self.school_years.iter().find(|y| y.id == id)
    }

    pub fn class(&self, id: Id) -> Option<&SchoolClass> {
        self.classes.iter().find(|c| c.id == id)
    }

    pub fn class_mut(&mut self, id: Id) -> Option<&mut SchoolClass> {
        self.classes.iter_mut().find(|c| c.id == id)
    }

    pub fn course(&self, id: Id) -> Option<&Course> {
        self.courses.iter().find(|c| c.id == id)
    }

    pub fn course_mut(&mut self, id: Id) -> Option<&mut Course> {
        self.courses.iter_mut().find(|c| c.id == id)
    }

    pub fn is_class_archived(&self, class_id: Id) -> bool {
        self.class(class_id)
            .and_then(|c| self.year(c.year_id))
            .is_some_and(|y| y.archived)
    }

    pub fn classes_of_year(&self, year: Id) -> impl Iterator<Item = &SchoolClass> {
        self.classes.iter().filter(move |c| c.year_id == year)
    }

    pub fn courses_of_class(&self, class: Id) -> impl Iterator<Item = &Course> {
        self.courses.iter().filter(move |c| c.class_id == class)
    }

    pub fn delete_class(&mut self, class: Id) {
        self.classes.retain(|c| c.id != class);
        self.courses.retain(|c| c.class_id != class);
    }

    pub fn delete_course(&mut self, course: Id) {
        self.courses.retain(|c| c.id != course);
    }

    pub fn delete_student(&mut self, class: Id, student: Id) {
        if let Some(c) = self.class_mut(class) {
            c.students.retain(|s| s.id != student);
        }
        for course in self.courses.iter_mut().filter(|c| c.class_id == class) {
            course.grades.retain(|g| g.student != student);
            course.finals.retain(|f| f.student != student);
            course.oral.retain(|o| o.student != student);
        }
    }

    /// Legt ein neues Schuljahr an und übernimmt die gewählten Klassen
    /// (mit neuem Namen, Schülern und optional den Fächern ohne Noten).
    pub fn new_school_year(
        &mut self,
        name: &str,
        carry: &[(Id, String)],
        with_courses: bool,
    ) -> Id {
        let year_id = self.alloc_id();
        self.school_years.push(SchoolYear {
            id: year_id,
            name: name.to_string(),
            archived: false,
        });
        for (old_class, new_name) in carry {
            let Some(old) = self.class(*old_class).cloned() else {
                continue;
            };
            let class_id = self.alloc_id();
            // Schüler behalten ihre IDs nicht – neue Klasse, neue IDs.
            let students = old
                .students
                .iter()
                .map(|s| Student {
                    id: self.alloc_id(),
                    ..s.clone()
                })
                .collect();
            self.classes.push(SchoolClass {
                id: class_id,
                year_id,
                name: new_name.clone(),
                students,
            });
            if with_courses {
                let courses: Vec<Course> = self.courses_of_class(*old_class).cloned().collect();
                for c in courses {
                    let scheme = self.clone_scheme(&c.scheme);
                    let id = self.alloc_id();
                    self.courses.push(Course {
                        id,
                        class_id,
                        subject: c.subject,
                        short: c.short,
                        scale: c.scale,
                        scheme,
                        grades: Vec::new(),
                        finals: Vec::new(),
                        oral: Vec::new(),
                        oral_scheme: Default::default(),
                    });
                }
            }
        }
        year_id
    }

    /// Repariert offensichtliche Inkonsistenzen (z. B. nach manueller Bearbeitung).
    pub fn normalize(&mut self) {
        let max = self.max_id();
        if self.next_id <= max {
            self.next_id = max + 1;
        }
    }

    fn max_id(&self) -> Id {
        let mut m = 0;
        m = m.max(self.school_years.iter().map(|y| y.id).max().unwrap_or(0));
        for c in &self.classes {
            m = m.max(c.id);
            m = m.max(c.students.iter().map(|s| s.id).max().unwrap_or(0));
        }
        for c in &self.courses {
            m = m.max(c.id);
            m = m.max(scheme_max_id(&c.scheme));
        }
        for t in &self.templates {
            m = m.max(t.id).max(scheme_max_id(&t.scheme));
        }
        m
    }
}

fn scheme_max_id(s: &Scheme) -> Id {
    s.areas
        .iter()
        .flat_map(|a| std::iter::once(a.id).chain(a.columns.iter().map(|c| c.id)))
        .max()
        .unwrap_or(0)
}

/// Schuljahr nach heutigem Datum (Wechsel zum 1. August).
pub fn current_school_year_name() -> String {
    use chrono::Datelike;
    let today = chrono::Local::now().date_naive();
    let start = if today.month() >= 8 {
        today.year()
    } else {
        today.year() - 1
    };
    format!("{}/{:02}", start, (start + 1) % 100)
}
