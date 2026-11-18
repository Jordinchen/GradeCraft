//! GradeCraft – Kernbibliothek: Datenmodell, Notenberechnung, verschlüsselte Speicherung.

pub mod calc;
pub mod csv;
pub mod demo;
pub mod model;
pub mod scale;
pub mod store;

pub use model::*;
pub use scale::ScaleKind;
