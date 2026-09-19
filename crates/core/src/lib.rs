// SPDX-License-Identifier: MIT OR Apache-2.0
//! Kern von Ablagefuchs: erkennt Datum, Unternehmen, Thema und Rechnung aus dem Text eines Dokuments
//! und baut daraus den Dateinamen.
//!
//! Bewusst ohne PDF- und GUI-Abhängigkeiten: Eingabe ist Text mit Positionen ([`PageText`]),
//! damit sich alles schnell und deterministisch testen lässt.

mod model;

pub use model::{PageText, Rect, TextSource, Word};
