// SPDX-License-Identifier: MIT OR Apache-2.0
//! Dateinamen aus einer Vorlage bauen, Feldwerte bereinigen und Namen zurückparsen (Bauplan §6).
//!
//! Vorlage: `{datum}_{unternehmen}_-_{thema}{rechnung?:_Rechnung}`. Platzhalter in `{…}`,
//! bedingter Text `{feld?:text}` erscheint nur, wenn das Feld gesetzt ist.

use std::fmt;

use chrono::{Datelike, NaiveDate};
use regex::Regex;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

pub const DEFAULT_TEMPLATE: &str = "{datum}_{unternehmen}_-_{thema}{rechnung?:_Rechnung}";

/// Höchstlänge des Dateinamens in Byte (inklusive `.pdf`), Puffer für lange NAS-Pfade unter Windows.
pub const MAX_NAME_BYTES: usize = 200;

/// Die Felder, aus denen ein Dateiname entsteht.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NameFields {
    pub datum: Option<NaiveDate>,
    pub unternehmen: String,
    pub thema: String,
    pub rechnung: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Spaces {
    Underscore,
    Dash,
    Remove,
    Keep,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamingOptions {
    /// ä → ae, ö → oe, ü → ue, ß → ss
    pub transliterate_umlauts: bool,
    pub spaces: Spaces,
    /// nur A–Z, 0–9, `_` und `-`; Akzente weg, `&` → und
    pub strict: bool,
}

impl Default for NamingOptions {
    fn default() -> Self {
        NamingOptions {
            transliterate_umlauts: true,
            spaces: Spaces::Underscore,
            strict: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Key {
    Datum,
    Jahr,
    Monat,
    Tag,
    Unternehmen,
    Thema,
    Rechnung,
}

impl Key {
    const ALL: [(&'static str, Key); 7] = [
        ("datum", Key::Datum),
        ("jahr", Key::Jahr),
        ("monat", Key::Monat),
        ("tag", Key::Tag),
        ("unternehmen", Key::Unternehmen),
        ("thema", Key::Thema),
        ("rechnung", Key::Rechnung),
    ];

    fn parse(name: &str) -> Result<Key, TemplateError> {
        let lower = name.to_lowercase();
        if let Some((_, k)) = Key::ALL.iter().find(|(n, _)| *n == lower) {
            return Ok(*k);
        }
        // Häufige Verwechslungen, auch englische Begriffe
        let suggestion = match lower.as_str() {
            "date" | "dat" => Some("datum"),
            "firma" | "company" | "absender" | "sender" => Some("unternehmen"),
            "topic" | "betreff" | "subject" | "titel" => Some("thema"),
            "invoice" | "rg" => Some("rechnung"),
            "year" => Some("jahr"),
            "month" => Some("monat"),
            "day" => Some("tag"),
            _ => None,
        };
        Err(TemplateError::UnknownPlaceholder {
            name: name.to_string(),
            suggestion,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Literal(String),
    Field(Key),
    /// `{feld?:text}`
    Cond(Key, String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    UnknownPlaceholder {
        name: String,
        suggestion: Option<&'static str>,
    },
    Unclosed,
    Empty,
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TemplateError::UnknownPlaceholder {
                name,
                suggestion: Some(s),
            } => {
                write!(f, "Unbekannter Platzhalter {{{name}}} – meintest du {{{s}}}?")
            }
            TemplateError::UnknownPlaceholder { name, suggestion: None } => {
                write!(f, "Unbekannter Platzhalter {{{name}}}")
            }
            TemplateError::Unclosed => write!(f, "Eine geschweifte Klammer {{ wird nicht geschlossen"),
            TemplateError::Empty => write!(f, "Die Vorlage ist leer"),
        }
    }
}

impl std::error::Error for TemplateError {}

/// Eine geprüfte Vorlage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    parts: Vec<Part>,
}

impl Template {
    pub fn parse(src: &str) -> Result<Template, TemplateError> {
        if src.trim().is_empty() {
            return Err(TemplateError::Empty);
        }
        let mut parts = Vec::new();
        let mut rest = src;
        while let Some(start) = rest.find('{') {
            if start > 0 {
                parts.push(Part::Literal(rest[..start].to_string()));
            }
            let end = rest[start..].find('}').ok_or(TemplateError::Unclosed)? + start;
            let inner = &rest[start + 1..end];
            parts.push(match inner.split_once("?:") {
                Some((key, text)) => Part::Cond(Key::parse(key.trim())?, text.to_string()),
                None => Part::Field(Key::parse(inner.trim())?),
            });
            rest = &rest[end + 1..];
        }
        if !rest.is_empty() {
            parts.push(Part::Literal(rest.to_string()));
        }
        Ok(Template { parts })
    }

    fn uses(&self, key: Key) -> bool {
        self.parts
            .iter()
            .any(|p| matches!(p, Part::Field(k) | Part::Cond(k, _) if *k == key))
    }

    /// Setzt die Felder ein (ohne Endung, ohne Längenbegrenzung).
    fn render_raw(&self, f: &NameFields, opts: &NamingOptions) -> String {
        // „Rechnung“ am Themenende nicht doppelt, wenn der Rechnungs-Zusatz ohnehin erscheint
        let thema = if f.rechnung && self.uses(Key::Rechnung) {
            strip_invoice_suffix(&f.thema)
        } else {
            f.thema.clone()
        };
        let date = f.datum;
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Literal(s) => out.push_str(s),
                Part::Field(k) => out.push_str(&match k {
                    Key::Datum => date.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default(),
                    Key::Jahr => date.map(|d| format!("{:04}", d.year())).unwrap_or_default(),
                    Key::Monat => date.map(|d| format!("{:02}", d.month())).unwrap_or_default(),
                    Key::Tag => date.map(|d| format!("{:02}", d.day())).unwrap_or_default(),
                    Key::Unternehmen => sanitize(&f.unternehmen, opts),
                    Key::Thema => sanitize(&thema, opts),
                    Key::Rechnung => {
                        if f.rechnung {
                            "Rechnung".into()
                        } else {
                            String::new()
                        }
                    }
                }),
                Part::Cond(k, text) => {
                    let set = match k {
                        Key::Datum | Key::Jahr | Key::Monat | Key::Tag => date.is_some(),
                        Key::Unternehmen => !f.unternehmen.trim().is_empty(),
                        Key::Thema => !thema.trim().is_empty(),
                        Key::Rechnung => f.rechnung,
                    };
                    if set {
                        out.push_str(text);
                    }
                }
            }
        }
        tidy(&out)
    }

    /// Fertiger Dateiname mit `.pdf`, auf [`MAX_NAME_BYTES`] gekürzt (zuerst am Thema, dann am Unternehmen).
    pub fn file_name(&self, f: &NameFields, opts: &NamingOptions) -> String {
        let mut fields = f.clone();
        let mut name = self.render_raw(&fields, opts);
        while name.len() + 4 > MAX_NAME_BYTES {
            let target = if !fields.thema.is_empty() {
                &mut fields.thema
            } else {
                &mut fields.unternehmen
            };
            if target.is_empty() {
                // Nur noch feste Teile: hart kürzen
                let mut cut = MAX_NAME_BYTES - 4;
                while !name.is_char_boundary(cut) {
                    cut -= 1;
                }
                name.truncate(cut);
                break;
            }
            target.pop();
            name = self.render_raw(&fields, opts);
        }
        if name.is_empty() {
            name = "Dokument".into();
        }
        if is_windows_reserved(&name) {
            name.push('_');
        }
        format!("{name}.pdf")
    }

    /// Liest Felder aus einem Dateinamen, der mit dieser Vorlage entstanden ist (für das Lernen aus dem Archiv).
    /// Unterstriche werden wieder zu Leerzeichen; Umlaute lassen sich nicht zurückgewinnen.
    pub fn parse_name(&self, file_name: &str) -> Option<NameFields> {
        let mut pattern = String::from("(?i)^");
        let mut keys = Vec::new();
        for (i, part) in self.parts.iter().enumerate() {
            match part {
                Part::Literal(s) => pattern.push_str(&regex::escape(s)),
                Part::Field(k) => {
                    let re = match k {
                        Key::Datum => r"\d{4}-\d{2}-\d{2}",
                        Key::Jahr => r"\d{4}",
                        Key::Monat | Key::Tag => r"\d{2}",
                        Key::Unternehmen | Key::Thema => ".+?",
                        Key::Rechnung => "Rechnung",
                    };
                    pattern.push_str(&format!("(?P<f{i}>{re})"));
                    keys.push((i, *k, false));
                }
                Part::Cond(k, text) => {
                    pattern.push_str(&format!("(?P<f{i}>{})?", regex::escape(text)));
                    keys.push((i, *k, true));
                }
            }
        }
        // Zähler bei Namensgleichheit (…_2.pdf) höchstens zweistellig, damit „…_2025“ beim Thema bleibt
        pattern.push_str(r"(?:_\d{1,2})?\.pdf$");
        let caps = Regex::new(&pattern).ok()?.captures(file_name)?;
        let mut f = NameFields::default();
        let (mut year, mut month, mut day) = (None, None, None);
        for (i, key, cond) in keys {
            let value = caps.name(&format!("f{i}")).map(|m| m.as_str());
            if cond {
                if key == Key::Rechnung {
                    f.rechnung |= value.is_some();
                }
                continue;
            }
            let Some(v) = value else { continue };
            match key {
                Key::Datum => f.datum = NaiveDate::parse_from_str(v, "%Y-%m-%d").ok(),
                Key::Jahr => year = v.parse().ok(),
                Key::Monat => month = v.parse().ok(),
                Key::Tag => day = v.parse().ok(),
                Key::Unternehmen => f.unternehmen = v.replace('_', " "),
                Key::Thema => f.thema = v.replace('_', " "),
                Key::Rechnung => f.rechnung = true,
            }
        }
        if f.datum.is_none()
            && let (Some(y), Some(m)) = (year, month)
        {
            f.datum = NaiveDate::from_ymd_opt(y, m, day.unwrap_or(1));
        }
        Some(f)
    }
}

/// Bei Namensgleichheit `_2`, `_3` … vor `.pdf` anhängen. `exists` prüft, ob ein Name schon vergeben ist.
pub fn dedupe(name: &str, exists: impl Fn(&str) -> bool) -> String {
    if !exists(name) {
        return name.to_string();
    }
    let stem = name.strip_suffix(".pdf").unwrap_or(name);
    (2..)
        .map(|n| format!("{stem}_{n}.pdf"))
        .find(|candidate| !exists(candidate))
        .expect("irgendwann ist ein Name frei")
}

/// Bereinigt einen Feldwert für den Dateinamen (Reihenfolge wie im Bauplan §6).
pub fn sanitize(value: &str, opts: &NamingOptions) -> String {
    let mut v: String = value.nfc().filter(|c| !c.is_control()).collect();
    v = v.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], " ");
    if opts.transliterate_umlauts {
        v = v
            .replace('ä', "ae")
            .replace('ö', "oe")
            .replace('ü', "ue")
            .replace('Ä', "Ae")
            .replace('Ö', "Oe")
            .replace('Ü', "Ue")
            .replace('ß', "ss");
    }
    if opts.strict {
        // Akzente entfernen (é → e), Umlaute aber stehen lassen, falls sie nicht umgeschrieben werden sollen
        v = v
            .chars()
            .flat_map(|c| {
                if is_umlaut(c) {
                    vec![c]
                } else {
                    c.nfd().filter(|d| !('\u{300}'..='\u{36f}').contains(d)).collect()
                }
            })
            .collect();
        v = v.replace('&', " und ").replace('+', " plus ");
        v = v
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ' || is_umlaut(c) {
                    c
                } else {
                    ' '
                }
            })
            .collect();
    }
    // `_-_` ist als Feldtrenner reserviert: Bindestriche im Wert ohne Leer-/Unterstriche drumherum
    let dash = Regex::new(r"[\s_]*-[\s_-]*").unwrap();
    v = dash.replace_all(&v, "-").into_owned();
    v = v.split_whitespace().collect::<Vec<_>>().join(" ");
    v = match opts.spaces {
        Spaces::Underscore => v.replace(' ', "_"),
        Spaces::Dash => v.replace(' ', "-"),
        Spaces::Remove => v.replace(' ', ""),
        Spaces::Keep => v,
    };
    let repeats = Regex::new(r"__+").unwrap();
    v = repeats.replace_all(&v, "_").into_owned();
    v.trim_matches(['_', '-', ' ']).to_string()
}

fn is_umlaut(c: char) -> bool {
    matches!(c, 'ä' | 'ö' | 'ü' | 'Ä' | 'Ö' | 'Ü' | 'ß')
}

/// Entfernt „Rechnung“ am Ende des Themas („Strom Rechnung“ → „Strom“).
pub fn strip_invoice_suffix(thema: &str) -> String {
    let re = Regex::new(r"(?i)[\s_-]*rechnung\s*$").unwrap();
    re.replace(thema, "").trim().to_string()
}

/// Leere Felder hinterlassen verwaiste Trenner: weg damit.
fn tidy(name: &str) -> String {
    let orphan = Regex::new(r"_-_(_|$)").unwrap();
    let mut s = orphan.replace_all(name, "$1").into_owned();
    s = s.strip_prefix("_-_").unwrap_or(&s).to_string();
    let repeats = Regex::new(r"__+").unwrap();
    s = repeats.replace_all(&s, "_").into_owned();
    s.trim_matches(['_', ' ']).to_string()
}

fn is_windows_reserved(name: &str) -> bool {
    let base = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && base.as_bytes()[3].is_ascii_digit()
            && base.as_bytes()[3] != b'0')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(datum: &str, unternehmen: &str, thema: &str, rechnung: bool) -> NameFields {
        NameFields {
            datum: NaiveDate::parse_from_str(datum, "%Y-%m-%d").ok(),
            unternehmen: unternehmen.into(),
            thema: thema.into(),
            rechnung,
        }
    }

    fn name(f: &NameFields) -> String {
        Template::parse(DEFAULT_TEMPLATE)
            .unwrap()
            .file_name(f, &NamingOptions::default())
    }

    #[test]
    fn examples_from_blueprint() {
        let cases = [
            (
                fields("2026-09-28", "Stadtwerke Musterstadt", "Jahresabrechnung Strom", true),
                "2026-09-28_Stadtwerke_Musterstadt_-_Jahresabrechnung_Strom_Rechnung.pdf",
            ),
            (
                fields("2026-09-15", "Krankenkasse Süd", "Beitragsbescheinigung 2025", false),
                "2026-09-15_Krankenkasse_Sued_-_Beitragsbescheinigung_2025.pdf",
            ),
            (
                fields(
                    "2026-09-30",
                    "Zahnarztpraxis Dr. Weber",
                    "Professionelle Zahnreinigung",
                    true,
                ),
                "2026-09-30_Zahnarztpraxis_Dr_Weber_-_Professionelle_Zahnreinigung_Rechnung.pdf",
            ),
            (
                fields("2026-09-30", "Müller - Partner", "Strom Rechnung", true),
                "2026-09-30_Mueller-Partner_-_Strom_Rechnung.pdf",
            ),
            (
                fields("2026-09-30", "Café & Co.", "", false),
                "2026-09-30_Cafe_und_Co.pdf",
            ),
        ];
        for (f, expected) in cases {
            assert_eq!(name(&f), expected);
        }
    }

    #[test]
    fn options_keep_umlauts_and_spaces() {
        let opts = NamingOptions {
            transliterate_umlauts: false,
            spaces: Spaces::Keep,
            strict: true,
        };
        assert_eq!(sanitize("Krankenkasse Süd", &opts), "Krankenkasse Süd");
        let opts = NamingOptions {
            transliterate_umlauts: true,
            spaces: Spaces::Dash,
            strict: false,
        };
        assert_eq!(sanitize("Grüße (Test)", &opts), "Gruesse-(Test)");
    }

    #[test]
    fn template_errors() {
        assert_eq!(
            Template::parse("{datum}_{firma}"),
            Err(TemplateError::UnknownPlaceholder {
                name: "firma".into(),
                suggestion: Some("unternehmen")
            })
        );
        assert_eq!(Template::parse("{datum"), Err(TemplateError::Unclosed));
        assert_eq!(Template::parse("  "), Err(TemplateError::Empty));
    }

    #[test]
    fn parse_back() {
        let t = Template::parse(DEFAULT_TEMPLATE).unwrap();
        for f in [
            fields("2026-09-28", "Stadtwerke Musterstadt", "Jahresabrechnung Strom", true),
            fields("2026-09-15", "Krankenkasse Sued", "Beitragsbescheinigung 2025", false),
            fields("2026-10-01", "Mueller-Partner", "Mobilfunk September", true),
        ] {
            let n = t.file_name(&f, &NamingOptions::default());
            assert_eq!(t.parse_name(&n), Some(f.clone()), "{n}");
            let numbered = n.replace(".pdf", "_2.pdf");
            assert_eq!(t.parse_name(&numbered), Some(f), "{numbered}");
        }
        assert_eq!(t.parse_name("scan_20261002_0001.pdf"), None);
    }

    #[test]
    fn empty_fields_and_limits() {
        assert_eq!(
            name(&fields("2026-01-02", "Firma", "", true)),
            "2026-01-02_Firma_Rechnung.pdf"
        );
        assert_eq!(name(&fields("", "Firma", "Thema", false)), "Firma_-_Thema.pdf");
        let long = name(&fields("2026-01-02", "Firma", &"Sehr langes Thema ".repeat(30), false));
        assert!(long.len() <= MAX_NAME_BYTES && long.ends_with(".pdf") && long.starts_with("2026-01-02_Firma_-_"));
        let t = Template::parse("{unternehmen}").unwrap();
        assert_eq!(
            t.file_name(&fields("", "CON", "", false), &NamingOptions::default()),
            "CON_.pdf"
        );
    }

    #[test]
    fn dedupe_appends_counter() {
        let taken = ["a.pdf", "a_2.pdf"];
        assert_eq!(dedupe("a.pdf", |n| taken.contains(&n)), "a_3.pdf");
        assert_eq!(dedupe("b.pdf", |n| taken.contains(&n)), "b.pdf");
    }
}
