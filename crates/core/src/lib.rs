// SPDX-License-Identifier: MIT OR Apache-2.0
//! Kern von Ablagefuchs: erkennt Datum, Unternehmen, Thema und Rechnung aus dem Text eines Dokuments
//! und baut daraus den Dateinamen.
//!
//! Bewusst ohne PDF- und GUI-Abhängigkeiten: Eingabe ist Text mit Positionen ([`PageText`]),
//! damit sich alles schnell und deterministisch testen lässt.

mod analyze;
pub mod company;
pub mod dates;
pub mod files;
pub mod invoice;
pub mod knowledge;
pub mod layout;
pub mod loose_name;
mod model;
pub mod naming;
pub mod topic;

pub use analyze::{Context, Field, Highlight, LearnInfo, Source, Suggestion, analyze};
pub use model::{PageText, Rect, TextSource, Word};
