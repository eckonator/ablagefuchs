// SPDX-License-Identifier: MIT OR Apache-2.0
//! Gelerntes Wissen: Unternehmen mit Schreibweise des Nutzers, Aliasen, eindeutigen Kennungen
//! (USt-IdNr., IBAN, Steuernummer, Domain) und typischen Themen. Wird nur lokal gespeichert (`wissen.json`)
//! und ausschließlich aus den Dateien des Nutzers aufgebaut: aus seinen Korrekturen und optional aus einem
//! Archivordner. Kein Netzwerk, keine KI.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::company::strip_legal_form;
use crate::layout::{Layout, Line};
use crate::logo::{self, Fingerprint};
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
    /// Fingerabdrücke des Briefkopfs (für Absender, deren Name nur als Bild gedruckt ist)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub logos: Vec<Fingerprint>,
    /// Nie als mögliches Doppel vorschlagen (z. B. die Bank: Kontoauszüge nennen viele andere Firmen)
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub nie_doppel: bool,
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
    /// Paare, die der Nutzer als verschiedene Unternehmen bestätigt hat (keine Doppel-Vorschläge mehr)
    pub keine_doppel: Vec<(String, String)>,
}

impl Default for Knowledge {
    fn default() -> Self {
        Knowledge {
            version: VERSION,
            unternehmen: Vec::new(),
            keine_doppel: Vec::new(),
        }
    }
}

/// Warum zwei Unternehmen vermutlich dasselbe sind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// ein Name ist der Anfang des anderen oder sehr ähnlich geschrieben
    Name,
    /// genau diese beiden teilen sich eine USt-IdNr. oder Steuernummer
    Kennung(IdKind),
    /// gleicher Briefkopf
    Briefkopf,
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
    // (Kennung, steht im Fließtext)
    let mut out: Vec<(FoundId, bool)> = Vec::new();
    let mut push = |kind, value: String, line: &Line, range: std::ops::Range<usize>| {
        if !out.iter().any(|(f, _)| f.kind == kind && f.value == value) {
            out.push((
                FoundId {
                    kind,
                    value,
                    page: line.page,
                    rect: line.rect_of(range),
                },
                // Folgeseiten sind bei Listen voller Buchungen, auch oben und unten
                line.zone == crate::layout::Zone::Body || line.page > 1,
            ));
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
    // Listen (Kontoauszug, Sammelrechnung) nennen im Text die Kennungen vieler anderer Firmen: dann zählen nur
    // Briefkopf und Fußzeile, sonst hingen fremde IBANs am Absender
    let in_body = out.iter().filter(|(_, body)| *body).count();
    out.into_iter()
        .filter(|(_, body)| !body || in_body < LIST_IDS)
        .map(|(f, _)| f)
        .collect()
}

/// Ab so vielen Kennungen im Fließtext gilt ein Dokument als Liste fremder Kennungen.
const LIST_IDS: usize = 3;

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

/// Übernimmt Aliase, Kennungen, Themen und Zähler von `c` in `own`; `count` verknüpft die Zähler
/// (Maximum beim Import, Summe beim Zusammenführen).
fn absorb(own: &mut Company, c: Company, count: impl Fn(u32, u32) -> u32) {
    for a in std::iter::once(c.name).chain(c.aliase) {
        let n = normalize_name(&a);
        if n != normalize_name(&own.name) && !own.aliase.iter().any(|x| normalize_name(x) == n) {
            own.aliase.push(a);
        }
    }
    for (kind, values) in c.kennungen {
        let list = own.kennungen.entry(kind).or_default();
        for v in values {
            if !list.contains(&v) {
                list.push(v);
            }
        }
    }
    for (thema, n) in c.themen {
        let e = own.themen.entry(thema).or_insert(0);
        *e = count(*e, n);
    }
    for fp in c.logos {
        logo::add_to(&mut own.logos, fp);
    }
    own.nie_doppel |= c.nie_doppel;
    own.rechnungen = count(own.rechnungen, c.rechnungen);
    own.anzahl = count(own.anzahl, c.anzahl);
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

    /// Unternehmen über den Fingerabdruck des Briefkopfs: das ähnlichste, wenn es über [`logo::SAME`] liegt
    /// und deutlich vor dem nächsten Unternehmen. Gibt (Nummer, Ähnlichkeit) zurück.
    pub fn match_logo(&self, fp: &Fingerprint) -> Option<(usize, f32)> {
        let mut scores: Vec<(usize, f32)> = self
            .unternehmen
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                c.logos
                    .iter()
                    .map(|f| f.similarity(fp))
                    .max_by(f32::total_cmp)
                    .map(|s| (i, s))
            })
            .collect();
        scores.sort_by(|a, b| b.1.total_cmp(&a.1));
        let &(best, score) = scores.first()?;
        let second = scores.get(1).map_or(f32::MIN, |s| s.1);
        (score >= logo::SAME && score - second >= logo::MARGIN).then_some((best, score))
    }

    /// Briefkopf-Fingerabdruck zu einem Unternehmen (Name oder Alias) nachtragen, ohne etwas mitzuzählen.
    pub fn add_logo(&mut self, name: &str, fp: Fingerprint) -> bool {
        match self.find_company(name) {
            Some(i) => logo::add_to(&mut self.unternehmen[i].logos, fp),
            None => false,
        }
    }

    /// Unternehmen über eindeutige Kennungen (Mehrheitsentscheid unter den eindeutigen).
    pub fn match_ids(&self, ids: &[FoundId]) -> Option<Match> {
        let mut votes: BTreeMap<usize, (usize, &FoundId)> = BTreeMap::new();
        for id in ids {
            if let [owner] = self.id_owners(id.kind, &id.value)[..] {
                let e = votes.entry(owner).or_insert((0, id));
                e.0 += 1;
            }
        }
        votes
            .iter()
            .max_by_key(|(_, (n, _))| *n)
            .map(|(&company, &(agreeing, id))| Match::ById {
                company,
                kind: id.kind,
                agreeing,
                page: id.page,
                rect: id.rect,
            })
    }

    /// Sucht das Unternehmen eines Dokuments: zuerst über eindeutige Kennungen, dann über den Namen im Briefkopf.
    pub fn match_document(&self, layout: &Layout, ids: &[FoundId]) -> Option<Match> {
        if let Some(m) = self.match_ids(ids) {
            return Some(m);
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
        logo: Option<&Fingerprint>,
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
        if let Some(fp) = logo {
            // Briefkopf gehört zum richtigen Unternehmen, nicht (mehr) zum falsch vorgeschlagenen
            if let Some(r) = replaced {
                self.unternehmen[r].logos.retain(|f| f.similarity(fp) < logo::SAME);
            }
            logo::add_to(&mut self.unternehmen[idx].logos, fp.clone());
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
            .map(|c| format!("{}|{:?}|{:?}|{}", c.name, c.aliase, c.kennungen, c.logos.len()))
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

    /// Führt Wissen von einem anderen Gerät hinzu. Gleiche Unternehmen (über Name oder Alias) werden vereint:
    /// Aliase und Kennungen zusammen, Zähler als Maximum (so zählt ein zweiter Import nichts doppelt).
    /// Gibt die Zahl der neu hinzugekommenen Unternehmen zurück.
    pub fn merge(&mut self, other: Knowledge) -> usize {
        let mut added = 0;
        for c in other.unternehmen {
            let found = std::iter::once(&c.name)
                .chain(c.aliase.iter())
                .find_map(|n| self.find_company(n));
            match found {
                Some(i) => absorb(&mut self.unternehmen[i], c, u32::max),
                None => {
                    self.unternehmen.push(c);
                    added += 1;
                }
            }
        }
        for pair in other.keine_doppel {
            if !self.keine_doppel.contains(&pair) {
                self.keine_doppel.push(pair);
            }
        }
        added
    }

    /// Neuer Name für ein Unternehmen (der alte bleibt als Alias, damit Briefe mit ihm weiter erkannt werden).
    /// Heißt schon ein anderes Unternehmen so, passiert nichts und dessen Nummer kommt zurück
    /// (dann ist Zusammenführen gemeint).
    pub fn rename_company(&mut self, i: usize, new_name: &str) -> Result<(), usize> {
        let new_name = new_name.trim();
        if new_name.is_empty() || i >= self.unternehmen.len() {
            return Ok(());
        }
        if let Some(j) = self.find_company(new_name).filter(|&j| j != i) {
            return Err(j);
        }
        let c = &mut self.unternehmen[i];
        let old = std::mem::replace(&mut c.name, new_name.to_string());
        let n = normalize_name(new_name);
        c.aliase.retain(|a| normalize_name(a) != n);
        if normalize_name(&old) != n && !c.aliase.iter().any(|a| normalize_name(a) == normalize_name(&old)) {
            c.aliase.push(old);
        }
        Ok(())
    }

    /// Führt Unternehmen `from` in `into` zusammen (Name von `into` bleibt, `from` wird Alias; Zähler addieren sich).
    pub fn merge_companies(&mut self, from: usize, into: usize) {
        if from == into || from >= self.unternehmen.len() || into >= self.unternehmen.len() {
            return;
        }
        let c = self.unternehmen.remove(from);
        let into = if from < into { into - 1 } else { into };
        absorb(&mut self.unternehmen[into], c, |a, b| a + b);
    }

    /// Paare, die vermutlich dasselbe Unternehmen sind: ein Name ist der Anfang des anderen
    /// („Stadtwerke“ / „Stadtwerke Musterstadt“), sehr ähnliche Schreibweise, oder genau diese beiden teilen
    /// sich eine Kennung. Als „kein Doppel“ markierte Paare fehlen. Häufigere Unternehmen zuerst.
    pub fn likely_duplicates(&self) -> Vec<(usize, usize, Why)> {
        let names: Vec<Vec<String>> = self
            .unternehmen
            .iter()
            .map(|c| {
                std::iter::once(&c.name)
                    .chain(c.aliase.iter())
                    .map(|n| normalize_name(n))
                    .collect()
            })
            .collect();
        let similar = |a: &str, b: &str| {
            let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
            (short.len() >= 4 && long.starts_with(&format!("{short} ")))
                || (short.len() >= 6 && strsim::jaro_winkler(a, b) >= 0.93)
        };
        let mut out = Vec::new();
        for i in 0..self.unternehmen.len() {
            for j in i + 1..self.unternehmen.len() {
                let (a, b) = (&self.unternehmen[i], &self.unternehmen[j]);
                if a.nie_doppel || b.nie_doppel || self.is_dismissed(&a.name, &b.name) {
                    continue;
                }
                let by_name = names[i].iter().any(|x| names[j].iter().any(|y| similar(x, y)));
                // Nur Kennungen, die eine Firma wirklich ausweisen: Eine IBAN kann die eigene des Nutzers sein
                // (Kontoauszug und Lastschrift-Brief), eine Domain bloß erwähnt
                let by_id = a
                    .kennungen
                    .iter()
                    .filter(|(kind, _)| matches!(kind, IdKind::UstId | IdKind::Steuernummer))
                    .find_map(|(kind, values)| {
                        values
                            .iter()
                            .any(|v| b.has_id(*kind, v) && self.id_owners(*kind, v).len() == 2)
                            .then_some(*kind)
                    });
                let by_logo = a
                    .logos
                    .iter()
                    .any(|x| b.logos.iter().any(|y| x.similarity(y) >= logo::SAME));
                let why = if by_name {
                    Some(Why::Name)
                } else if let Some(kind) = by_id {
                    Some(Why::Kennung(kind))
                } else {
                    by_logo.then_some(Why::Briefkopf)
                };
                if let Some(why) = why {
                    out.push((i, j, why));
                }
            }
        }
        out.sort_by_key(|&(i, j, _)| std::cmp::Reverse(self.unternehmen[i].anzahl + self.unternehmen[j].anzahl));
        out
    }

    /// Der Nutzer sagt: diese beiden sind verschiedene Unternehmen.
    pub fn dismiss_duplicate(&mut self, a: &str, b: &str) {
        if !self.is_dismissed(a, b) {
            self.keine_doppel.push((a.to_string(), b.to_string()));
        }
    }

    fn is_dismissed(&self, a: &str, b: &str) -> bool {
        let (a, b) = (normalize_name(a), normalize_name(b));
        self.keine_doppel.iter().any(|(x, y)| {
            let (x, y) = (normalize_name(x), normalize_name(y));
            (x == a && y == b) || (x == b && y == a)
        })
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
            None,
        );
        k.learn(
            &fields("NetzWelt", "Mobilfunk Oktober"),
            &ids,
            Some("NetzWelt Kommunikation"),
            None,
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
        k.learn(&fields("Stadtwerke", "Strom"), &ids, None, None, None);
        k.learn(&fields("NetzWelt", "Mobilfunk"), &ids, None, None, None);
        assert!(k.match_document(&own, &ids).is_none());
    }

    #[test]
    fn correction_moves_identifier() {
        let mut k = Knowledge::default();
        let doc = layout(&["USt-IdNr. DE123456789"]);
        let ids = find_ids(&doc);
        k.learn(&fields("Falsch GmbH", "x"), &ids, None, None, None);
        k.learn(&fields("Richtig", "x"), &ids, None, None, Some("Falsch GmbH"));
        let m = k.match_document(&doc, &ids).unwrap();
        assert_eq!(k.unternehmen[m.company()].name, "Richtig");
    }

    #[test]
    fn merge_from_other_device() {
        let company = |name: &str, alias: &[&str], ust: &str, thema: &str, n: u32| Company {
            name: name.into(),
            aliase: alias.iter().map(|a| a.to_string()).collect(),
            kennungen: BTreeMap::from([(IdKind::UstId, vec![ust.to_string()])]),
            themen: BTreeMap::from([(thema.to_string(), n)]),
            anzahl: n,
            ..Company::default()
        };
        let mut here = Knowledge {
            unternehmen: vec![company("NetzWelt", &[], "DE987654321", "Mobilfunk", 3)],
            ..Knowledge::default()
        };
        let there = Knowledge {
            unternehmen: vec![
                company("Netzwelt", &["NetzWelt Kommunikation"], "DE111111111", "Mobilfunk", 5),
                company("Stadtwerke Musterstadt", &[], "DE123456789", "Abschlag", 2),
            ],
            ..Knowledge::default()
        };
        assert_eq!(here.merge(there.clone()), 1);
        assert_eq!(here.unternehmen.len(), 2);
        let n = &here.unternehmen[0];
        assert_eq!(n.name, "NetzWelt", "eigene Schreibweise bleibt");
        assert_eq!(n.aliase, ["NetzWelt Kommunikation"]);
        assert_eq!(n.kennungen[&IdKind::UstId], ["DE987654321", "DE111111111"]);
        assert_eq!(n.themen["Mobilfunk"], 5);
        // zweiter Import ändert nichts mehr
        let before = here.clone();
        assert_eq!(here.merge(there), 0);
        assert_eq!(here, before);
    }

    fn pairs(k: &Knowledge) -> Vec<(usize, usize)> {
        k.likely_duplicates().iter().map(|d| (d.0, d.1)).collect()
    }

    #[test]
    fn own_iban_is_no_duplicate_evidence() {
        // Die eigene IBAN des Nutzers steht im Kontoauszug der Bank und im Lastschrift-Brief einer Firma
        let company = |name: &str| Company {
            name: name.into(),
            kennungen: BTreeMap::from([(IdKind::Iban, vec!["DE89370400440532013000".to_string()])]),
            anzahl: 1,
            ..Company::default()
        };
        let k = Knowledge {
            unternehmen: vec![company("Sparkasse Musterstadt"), company("Fitnessclub Nord")],
            ..Knowledge::default()
        };
        assert!(k.likely_duplicates().is_empty());
    }

    #[test]
    fn rename_merge_and_duplicates() {
        let company = |name: &str, ust: &str, n: u32| Company {
            name: name.into(),
            kennungen: BTreeMap::from([(IdKind::UstId, vec![ust.to_string()])]),
            themen: BTreeMap::from([("Abschlag".to_string(), n)]),
            anzahl: n,
            ..Company::default()
        };
        let mut k = Knowledge {
            unternehmen: vec![
                company("Stadtwerke", "DE1", 3),
                company("Stadtwerke Musterstadt", "DE2", 10),
                company("Telekom", "DE3", 4),
                company("Deutsche Telekom", "DE4", 1),
                company("Autohaus Becker", "DE5", 2),
            ],
            ..Knowledge::default()
        };
        // Präfix am Wortende: „Stadtwerke“ und „Telekom“ ja, „Deutsche Telekom“ nur über eine geteilte Kennung
        assert_eq!(pairs(&k), vec![(0, 1)]);
        k.unternehmen[3]
            .kennungen
            .get_mut(&IdKind::UstId)
            .unwrap()
            .push("DE3".into());
        assert_eq!(pairs(&k), vec![(0, 1), (2, 3)]);
        assert_eq!(k.likely_duplicates()[1].2, Why::Kennung(IdKind::UstId));
        k.dismiss_duplicate("Deutsche Telekom", "Telekom");
        assert_eq!(pairs(&k), vec![(0, 1)]);

        k.merge_companies(0, 1);
        let s = &k.unternehmen[0];
        assert_eq!(s.name, "Stadtwerke Musterstadt");
        assert_eq!(s.aliase, ["Stadtwerke"]);
        assert_eq!(s.anzahl, 13);
        assert_eq!(s.themen["Abschlag"], 13);
        assert_eq!(s.kennungen[&IdKind::UstId], ["DE2", "DE1"]);

        // jetzt: 0 Stadtwerke Musterstadt, 1 Telekom, 2 Deutsche Telekom, 3 Autohaus Becker
        assert_eq!(k.rename_company(2, "Autohaus Becker"), Err(3));
        assert_eq!(k.rename_company(2, "Autohaus Becker GmbH & Co. KG"), Err(3));
        assert_eq!(k.rename_company(2, "Telekom Deutschland"), Ok(()));
        assert_eq!(k.unternehmen[2].name, "Telekom Deutschland");
        assert_eq!(k.unternehmen[2].aliase, ["Deutsche Telekom"]);
        assert_eq!(
            k.find_company("Deutsche Telekom"),
            Some(2),
            "alter Name bleibt als Alias"
        );
    }

    /// Fingerabdruck aus einem Muster: `bits` wählt, welche Rasterzellen dunkel sind.
    fn logo_print(bits: impl Fn(usize) -> bool) -> Fingerprint {
        Fingerprint {
            raster: (0..logo::COLS * logo::ROWS)
                .map(|i| if bits(i) { 'f' } else { '0' })
                .collect(),
            seitenverhaeltnis: 4.0,
        }
    }

    #[test]
    fn recognized_by_logo_only_when_clear() {
        let stripes = logo_print(|i| (i / logo::COLS).is_multiple_of(3));
        let checker = logo_print(|i| (i % logo::COLS / 4 + i / logo::COLS / 3).is_multiple_of(2));
        let mut k = Knowledge::default();
        k.learn(&fields("Bildlogo AG", "Brief"), &[], None, Some(&stripes), None);
        k.learn(&fields("Andere", "Brief"), &[], None, Some(&checker), None);
        assert_eq!(k.match_logo(&stripes).map(|m| m.0), Some(0));
        assert_eq!(k.match_logo(&checker).map(|m| m.0), Some(1));
        // gleiches Logo bei zwei Unternehmen: unklar, kein Treffer – und ein Doppel-Vorschlag
        k.add_logo("Andere", stripes.clone());
        assert_eq!(k.match_logo(&stripes), None);
        assert!(k.likely_duplicates().contains(&(0, 1, Why::Briefkopf)));
        // Korrektur: der Nutzer sagt, dieses Logo gehört zu „Bildlogo AG“ → beim falschen entfernt
        k.learn(
            &fields("Bildlogo AG", "Brief"),
            &[],
            None,
            Some(&stripes),
            Some("Andere"),
        );
        assert_eq!(k.match_logo(&stripes).map(|m| m.0), Some(0));
        // fast gleiche Varianten werden nicht doppelt gespeichert
        assert_eq!(k.unternehmen[0].logos.len(), 1);
    }

    #[test]
    fn statements_do_not_hand_out_foreign_ids() {
        // Brief: IBAN im Text („bitte überweisen Sie auf …“) zählt
        let letter = layout(&[
            "Stadtwerke Musterstadt",
            "",
            "",
            "",
            "",
            "Bitte überweisen Sie auf DE89 3704 0044 0532 0130 00",
        ]);
        assert_eq!(find_ids(&letter).len(), 1);
        // Kontoauszug: viele fremde IBANs in den Buchungen, die eigene Kennung in der Kopfzeile
        let statement = layout(&[
            "Sparkasse Musterstadt USt-IdNr. DE111222333",
            "",
            "",
            "",
            "",
            "Lastschrift DE89 3704 0044 0532 0130 00",
            "Gutschrift DE02 1203 0000 0000 2020 51",
            "Lastschrift DE77 1001 0010 0123 4567 89",
        ]);
        let ids = find_ids(&statement);
        assert_eq!(ids.len(), 1, "{ids:?}");
        assert_eq!(ids[0].kind, IdKind::UstId);
    }
}
