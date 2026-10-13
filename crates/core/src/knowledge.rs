// SPDX-License-Identifier: MIT OR Apache-2.0
//! Gelerntes Wissen (Bauplan §5.7): Unternehmen mit Schreibweise des Nutzers, Aliasen, eindeutigen Kennungen
//! (USt-IdNr., IBAN, Steuernummer, Domain) und typischen Themen. Wird nur lokal gespeichert (`wissen.json`)
//! und ausschließlich aus den Dateien des Nutzers aufgebaut: aus seinen Korrekturen und optional aus einem
//! Archivordner. Kein Netzwerk, keine KI.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::company::strip_legal_form;
use crate::layout::{Layout, Line};
use crate::model::Rect;
use crate::naming::NameFields;

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdKind {
    UstId,
    Iban,
    Steuernummer,
    Domain,
}

/// Eine Kennung im Dokument, mit Fundstelle.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FoundId {
    pub kind: IdKind,
    /// normalisiert (ohne Leerzeichen, Großbuchstaben bzw. Domain klein)
    pub value: String,
    pub page: u16,
    pub rect: Rect,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Company {
    /// Schreibweise des Nutzers, so wie sie im Dateinamen stehen soll
    pub name: String,
    /// andere Schreibweisen aus Briefköpfen („NetzWelt Kommunikation GmbH“)
    pub aliase: Vec<String>,
    pub kennungen: BTreeMap<IdKind, Vec<String>>,
    /// Thema → Anzahl
    pub themen: BTreeMap<String, u32>,
    pub rechnungen: u32,
    pub anzahl: u32,
}

impl Company {
    /// Themen nach Häufigkeit, häufigstes zuerst.
    pub fn topics(&self) -> Vec<&str> {
        let mut t: Vec<(&String, &u32)> = self.themen.iter().collect();
        t.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        t.into_iter().map(|(k, _)| k.as_str()).collect()
    }

    fn has_id(&self, kind: IdKind, value: &str) -> bool {
        self.kennungen.get(&kind).is_some_and(|v| v.iter().any(|x| x == value))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Knowledge {
    pub version: u32,
    pub unternehmen: Vec<Company>,
}

impl Default for Knowledge {
    fn default() -> Self {
        Knowledge {
            version: VERSION,
            unternehmen: Vec::new(),
        }
    }
}

/// Treffer im Wissen für ein Dokument.
#[derive(Debug, Clone, PartialEq)]
pub enum Match {
    /// über eindeutige Kennung(en) erkannt
    ById {
        company: usize,
        kind: IdKind,
        agreeing: usize,
        page: u16,
        rect: Rect,
    },
    /// über Namen oder Alias in der Briefkopfzeile
    ByName { company: usize, page: u16, rect: Rect },
}

impl Match {
    pub fn company(&self) -> usize {
        match self {
            Match::ById { company, .. } | Match::ByName { company, .. } => *company,
        }
    }
}

static UST_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bDE\s?\d{3}\s?\d{3}\s?\d{3}\b").unwrap());
static IBAN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b[A-Z]{2}\d{2}(?:\s?[A-Z0-9]{4}){3,7}(?:\s?[A-Z0-9]{1,3})?\b").unwrap());
static STEUERNUMMER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d{2,3}/\d{3}/\d{4,5}\b").unwrap());
static DOMAIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:www\.)?([a-z0-9][a-z0-9-]{1,40}\.(?:de|com|net|org|eu|info|at|ch|example))\b").unwrap()
});
/// Häufige Mailanbieter sind keine Kennung eines Absenders.
const MAIL_PROVIDERS: [&str; 10] = [
    "gmx.de",
    "web.de",
    "gmail.com",
    "t-online.de",
    "outlook.com",
    "yahoo.de",
    "icloud.com",
    "gmx.net",
    "posteo.de",
    "mailbox.org",
];

/// Prüfsumme nach ISO 13616 (mod 97).
pub fn iban_valid(iban: &str) -> bool {
    let s: String = iban.chars().filter(|c| !c.is_whitespace()).collect();
    if s.len() < 15 || s.len() > 34 || !s.is_ascii() {
        return false;
    }
    let rearranged = format!("{}{}", &s[4..], &s[..4]);
    let mut rem: u32 = 0;
    for c in rearranged.chars() {
        let v = match c {
            '0'..='9' => c as u32 - '0' as u32,
            'A'..='Z' => c as u32 - 'A' as u32 + 10,
            _ => return false,
        };
        rem = if v >= 10 {
            (rem * 100 + v) % 97
        } else {
            (rem * 10 + v) % 97
        };
    }
    rem == 1
}

/// Alle Kennungen im Dokument (erste beiden Seiten).
pub fn find_ids(layout: &Layout) -> Vec<FoundId> {
    let mut out: Vec<FoundId> = Vec::new();
    let mut push = |kind, value: String, line: &Line, range: std::ops::Range<usize>| {
        if !out.iter().any(|f| f.kind == kind && f.value == value) {
            out.push(FoundId {
                kind,
                value,
                page: line.page,
                rect: line.rect_of(range),
            });
        }
    };
    for line in layout.lines.iter().filter(|l| l.page <= 2) {
        for m in UST_ID.find_iter(&line.text) {
            push(IdKind::UstId, m.as_str().replace(' ', ""), line, m.range());
        }
        for m in IBAN.find_iter(&line.text) {
            let v: String = m.as_str().chars().filter(|c| !c.is_whitespace()).collect();
            if iban_valid(&v) {
                push(IdKind::Iban, v, line, m.range());
            }
        }
        for m in STEUERNUMMER.find_iter(&line.text) {
            push(IdKind::Steuernummer, m.as_str().to_string(), line, m.range());
        }
        for c in DOMAIN.captures_iter(&line.text) {
            let d = c[1].to_lowercase();
            if !MAIL_PROVIDERS.contains(&d.as_str()) {
                push(IdKind::Domain, d, line, c.get(0).unwrap().range());
            }
        }
    }
    out
}

/// Vergleichsform eines Firmennamens: klein, ohne Rechtsform, nur Buchstaben/Ziffern und einzelne Leerzeichen.
pub fn normalize_name(name: &str) -> String {
    let n = strip_legal_form(name).to_lowercase();
    let n = n
        .replace('ä', "ae")
        .replace('ö', "oe")
        .replace('ü', "ue")
        .replace('ß', "ss");
    n.chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

impl Knowledge {
    pub fn find_company(&self, name: &str) -> Option<usize> {
        let n = normalize_name(name);
        if n.is_empty() {
            return None;
        }
        self.unternehmen
            .iter()
            .position(|c| normalize_name(&c.name) == n || c.aliase.iter().any(|a| normalize_name(a) == n))
    }

    /// Wie viele Unternehmen diese Kennung tragen (nur eindeutige zählen beim Erkennen;
    /// so stört z. B. die eigene IBAN des Nutzers, die in vielen Briefen steht, nicht).
    fn id_owners(&self, kind: IdKind, value: &str) -> Vec<usize> {
        (0..self.unternehmen.len())
            .filter(|&i| self.unternehmen[i].has_id(kind, value))
            .collect()
    }

    /// Sucht das Unternehmen eines Dokuments: zuerst über eindeutige Kennungen, dann über den Namen im Briefkopf.
    pub fn match_document(&self, layout: &Layout, ids: &[FoundId]) -> Option<Match> {
        // Kennungen: Mehrheitsentscheid unter den eindeutigen
        let mut votes: BTreeMap<usize, (usize, &FoundId)> = BTreeMap::new();
        for id in ids {
            if let [owner] = self.id_owners(id.kind, &id.value)[..] {
                let e = votes.entry(owner).or_insert((0, id));
                e.0 += 1;
            }
        }
        if let Some((&company, &(agreeing, id))) = votes.iter().max_by_key(|(_, (n, _))| *n) {
            return Some(Match::ById {
                company,
                kind: id.kind,
                agreeing,
                page: id.page,
                rect: id.rect,
            });
        }
        // Name/Alias: in Briefkopf, Absenderzeile oder Fußzeile der ersten Seite (nicht im Fließtext,
        // wo andere Firmen erwähnt sein können)
        let mut best: Option<(f64, usize, &Line)> = None;
        for line in layout
            .lines
            .iter()
            .filter(|l| l.page == 1 && l.zone != crate::layout::Zone::Body)
        {
            let text = normalize_name(&line.text);
            if text.is_empty() {
                continue;
            }
            for (i, c) in self.unternehmen.iter().enumerate() {
                for candidate in std::iter::once(&c.name).chain(c.aliase.iter()) {
                    let cand = normalize_name(candidate);
                    if cand.len() < 4 {
                        continue;
                    }
                    let score = if text == cand || text.starts_with(&format!("{cand} ")) {
                        1.0
                    } else {
                        strsim::jaro_winkler(&text, &cand)
                    };
                    if score >= 0.94 && best.is_none_or(|(s, _, _)| score > s) {
                        best = Some((score, i, line));
                    }
                }
            }
        }
        best.map(|(_, company, line)| Match::ByName {
            company,
            page: line.page,
            rect: line.bbox,
        })
    }

    /// Lernt aus einem fertig benannten Dokument: `fields` sind die endgültigen Felder (vom Nutzer bestätigt
    /// oder aus einem Dateinamen im Archiv), `letterhead` der aus dem Briefkopf gelesene Name.
    /// `replaced`: Unternehmen, das die App vorgeschlagen und der Nutzer durch ein anderes ersetzt hat.
    /// Gibt `true` zurück, wenn sich etwas für die Erkennung geändert hat (neues Unternehmen, neue Kennung,
    /// neuer Alias, neue Schreibweise); bloßes Mitzählen von Themen zählt nicht.
    pub fn learn(
        &mut self,
        fields: &NameFields,
        ids: &[FoundId],
        letterhead: Option<&str>,
        replaced: Option<&str>,
    ) -> bool {
        let name = fields.unternehmen.trim();
        if name.is_empty() {
            return false;
        }
        let before = self.recognition_fingerprint();
        let idx = match self
            .find_company(name)
            .or_else(|| letterhead.and_then(|l| self.find_company(l)))
        {
            Some(i) => i,
            None => {
                self.unternehmen.push(Company {
                    name: name.to_string(),
                    ..Company::default()
                });
                self.unternehmen.len() - 1
            }
        };
        let replaced = replaced.and_then(|r| self.find_company(r)).filter(|&r| r != idx);
        for id in ids {
            // Hat der Nutzer einen falschen Vorschlag korrigiert, wandert die Kennung zum richtigen Unternehmen.
            // Sonst bekommt das Unternehmen sie zusätzlich; steht sie dann bei mehreren, ist sie keine
            // Absenderkennung (z. B. die eigene IBAN des Nutzers) und zählt beim Erkennen nicht mehr.
            if let Some(r) = replaced
                && let Some(v) = self.unternehmen[r].kennungen.get_mut(&id.kind)
            {
                v.retain(|x| x != &id.value);
            }
            let list = self.unternehmen[idx].kennungen.entry(id.kind).or_default();
            if !list.contains(&id.value) {
                list.push(id.value.clone());
            }
        }
        let c = &mut self.unternehmen[idx];
        if c.name != name && normalize_name(&c.name) == normalize_name(name) {
            // gleiche Firma, neue Schreibweise des Nutzers: die gilt
            c.name = name.to_string();
        }
        if let Some(l) = letterhead
            && normalize_name(l) != normalize_name(&c.name)
            && !c.aliase.iter().any(|a| normalize_name(a) == normalize_name(l))
            && !l.trim().is_empty()
        {
            c.aliase.push(l.trim().to_string());
        }
        let thema = fields.thema.trim();
        if !thema.is_empty() {
            *c.themen.entry(thema.to_string()).or_insert(0) += 1;
        }
        c.anzahl += 1;
        if fields.rechnung {
            c.rechnungen += 1;
        }
        self.recognition_fingerprint() != before
    }

    /// Alles, was die Zuordnung eines Dokuments beeinflusst (Namen, Aliase, Kennungen), als Vergleichswert.
    fn recognition_fingerprint(&self) -> String {
        self.unternehmen
            .iter()
            .map(|c| format!("{}|{:?}|{:?}", c.name, c.aliase, c.kennungen))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Lädt `wissen.json`; fehlt die Datei, gibt es leeres Wissen.
    pub fn load(path: &std::path::Path) -> std::io::Result<Knowledge> {
        match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).map_err(std::io::Error::other),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Knowledge::default()),
            Err(e) => Err(e),
        }
    }

    /// Speichert atomar (Zwischendatei, dann umbenennen).
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, path)
    }

    /// Alle bekannten Unternehmensnamen (für die Autovervollständigung).
    pub fn company_names(&self) -> Vec<String> {
        self.unternehmen.iter().map(|c| c.name.clone()).collect()
    }

    /// Alle bekannten Themen (für die Autovervollständigung).
    pub fn topic_names(&self) -> Vec<String> {
        let mut t: Vec<String> = self.unternehmen.iter().flat_map(|c| c.themen.keys().cloned()).collect();
        t.sort();
        t.dedup();
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PageText, Word};

    fn layout(lines: &[&str]) -> Layout {
        let mut words = Vec::new();
        for (i, text) in lines.iter().enumerate() {
            let mut x = 60.0;
            for w in text.split(' ') {
                let width = w.chars().count() as f32 * 5.0;
                words.push(Word {
                    text: w.into(),
                    bbox: Rect {
                        x,
                        y: 60.0 + i as f32 * 30.0,
                        w: width,
                        h: 10.0,
                    },
                    font_size: 10.0,
                    bold: false,
                });
                x += width + 3.0;
            }
        }
        Layout::new(&[PageText {
            page: 1,
            width: 595.0,
            height: 842.0,
            words,
        }])
    }

    fn fields(unternehmen: &str, thema: &str) -> NameFields {
        NameFields {
            datum: None,
            unternehmen: unternehmen.into(),
            thema: thema.into(),
            rechnung: true,
        }
    }

    #[test]
    fn iban_checksum() {
        assert!(iban_valid("DE89370400440532013000"));
        assert!(iban_valid("DE89 3704 0044 0532 0130 00"));
        assert!(!iban_valid("DE89370400440532013001"));
    }

    #[test]
    fn finds_identifiers() {
        let l = layout(&[
            "NetzWelt Kommunikation GmbH · USt-IdNr. DE 987 654 321 · netzwelt.example",
            "IBAN DE89 3704 0044 0532 0130 00 · Steuernummer 35/123/45678 · kontakt@gmx.de",
        ]);
        let ids: Vec<(IdKind, String)> = find_ids(&l).into_iter().map(|f| (f.kind, f.value)).collect();
        assert!(ids.contains(&(IdKind::UstId, "DE987654321".into())));
        assert!(ids.contains(&(IdKind::Iban, "DE89370400440532013000".into())));
        assert!(ids.contains(&(IdKind::Steuernummer, "35/123/45678".into())));
        assert!(ids.contains(&(IdKind::Domain, "netzwelt.example".into())));
        assert!(!ids.iter().any(|(_, v)| v == "gmx.de"));
    }

    #[test]
    fn learns_spelling_and_matches_by_id_and_name() {
        let mut k = Knowledge::default();
        let doc = layout(&["NetzWelt Kommunikation GmbH", "USt-IdNr. DE987654321"]);
        let ids = find_ids(&doc);
        k.learn(
            &fields("NetzWelt", "Mobilfunk September"),
            &ids,
            Some("NetzWelt Kommunikation"),
            None,
        );
        k.learn(
            &fields("NetzWelt", "Mobilfunk Oktober"),
            &ids,
            Some("NetzWelt Kommunikation"),
            None,
        );
        assert_eq!(k.unternehmen.len(), 1);
        assert_eq!(k.unternehmen[0].aliase, ["NetzWelt Kommunikation"]);
        assert_eq!(k.unternehmen[0].anzahl, 2);

        // Kennung im neuen Brief → erkannt, auch wenn der Briefkopf anders aussieht
        let new = layout(&["Ganz anderer Kopf", "USt-IdNr. DE 987654321"]);
        let m = k.match_document(&new, &find_ids(&new)).unwrap();
        assert!(matches!(
            m,
            Match::ById {
                company: 0,
                kind: IdKind::UstId,
                ..
            }
        ));

        // ohne Kennung über den Alias im Briefkopf
        let new = layout(&["NetzWelt Kommunikation GmbH", "Ihre Rechnung"]);
        assert!(matches!(
            k.match_document(&new, &find_ids(&new)),
            Some(Match::ByName { company: 0, .. })
        ));
        assert_eq!(k.find_company("netzwelt kommunikation gmbh"), Some(0));
    }

    #[test]
    fn shared_ids_are_ignored() {
        // Die eigene IBAN des Nutzers steht in Briefen verschiedener Firmen: keine Kennung
        let mut k = Knowledge::default();
        let own = layout(&["Abbuchung von IBAN DE89 3704 0044 0532 0130 00"]);
        let ids = find_ids(&own);
        k.learn(&fields("Stadtwerke", "Strom"), &ids, None, None);
        k.learn(&fields("NetzWelt", "Mobilfunk"), &ids, None, None);
        assert!(k.match_document(&own, &ids).is_none());
    }

    #[test]
    fn correction_moves_identifier() {
        let mut k = Knowledge::default();
        let doc = layout(&["USt-IdNr. DE123456789"]);
        let ids = find_ids(&doc);
        k.learn(&fields("Falsch GmbH", "x"), &ids, None, None);
        k.learn(&fields("Richtig", "x"), &ids, None, Some("Falsch GmbH"));
        let m = k.match_document(&doc, &ids).unwrap();
        assert_eq!(k.unternehmen[m.company()].name, "Richtig");
    }
}
