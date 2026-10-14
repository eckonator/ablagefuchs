// SPDX-License-Identifier: MIT OR Apache-2.0
//! Wissen aus einem Archivordner lernen (rekursiv, mit allen Unterordnern). Läuft vollständig lokal:
//! Dateinamen werden mit der Vorlage zurückgelesen, dazu Kennungen und Briefkopf der ersten Seite.
//! Namen ohne Vorlagenschema (z. B. ohne `_-_`) werden frei gelesen; die Firma darin muss dann der Brief
//! bestätigen (siehe [`af_core::loose_name`]).
//!
//! Der [`LearnLog`] merkt sich, welche Dateien schon gelernt sind: Ein neuer Lauf überspringt sie (zählt also
//! nichts doppelt) und liest nur neue, geänderte und bisher nicht bestätigte Dateien.

use std::path::{Path, PathBuf};

use af_core::PageText;
use af_core::knowledge::{FoundId, Knowledge, find_ids, normalize_name};
use af_core::layout::Layout;
use af_core::logo::Fingerprint;
use af_core::loose_name::{
    GroupMember, LooseName, Mention, Miss, company_of_group, matches_any, mention, page_texts, sender_texts,
    subject_words,
};
use af_core::naming::{NameFields, Template};
use serde::{Deserialize, Serialize};

use crate::Preference;

/// Eine schon gelernte Datei: Größe und Änderungszeit (ändern sie sich, wird neu gelernt) und das Unternehmen
/// (wird es vergessen, darf die Datei wieder gelernt werden).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearnedFile {
    pub groesse: u64,
    /// Sekunden seit 1970
    pub geaendert: Option<u64>,
    pub unternehmen: String,
    /// Briefkopf-Fingerabdruck wurde schon übernommen (ältere Lernstände: nachtragen)
    #[serde(default)]
    pub briefkopf: bool,
}

/// Lernstand: welche Dateien schon ins Wissen eingeflossen sind. Liegt neben `wissen.json` (`lernstand.json`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LearnLog {
    pub dateien: std::collections::BTreeMap<PathBuf, LearnedFile>,
}

fn file_mark(path: &Path) -> Option<(u64, Option<u64>)> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    Some((meta.len(), modified))
}

impl LearnLog {
    pub fn load(path: &Path) -> std::io::Result<LearnLog> {
        match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).map_err(std::io::Error::other),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(LearnLog::default()),
            Err(e) => Err(e),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_string(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, path)
    }

    /// Schon gelernt und seitdem unverändert?
    pub fn is_learned(&self, path: &Path) -> bool {
        self.dateien
            .get(path)
            .is_some_and(|f| file_mark(path) == Some((f.groesse, f.geaendert)))
    }

    pub fn mark(&mut self, path: &Path, unternehmen: &str) {
        if let Some((groesse, geaendert)) = file_mark(path) {
            self.dateien.insert(
                path.to_path_buf(),
                LearnedFile {
                    groesse,
                    geaendert,
                    unternehmen: unternehmen.to_string(),
                    briefkopf: true,
                },
            );
        }
    }

    /// Ein Unternehmen wurde vergessen: seine Dateien dürfen wieder gelernt werden.
    pub fn forget_company(&mut self, names: &[&str]) {
        let names: Vec<String> = names.iter().map(|n| normalize_name(n)).collect();
        self.dateien
            .retain(|_, f| !names.contains(&normalize_name(&f.unternehmen)));
    }

    /// Ein Unternehmen heißt jetzt anders (umbenannt oder zusammengeführt).
    pub fn rename_company(&mut self, old: &[&str], new: &str) {
        let old: Vec<String> = old.iter().map(|n| normalize_name(n)).collect();
        for f in self.dateien.values_mut() {
            if old.contains(&normalize_name(&f.unternehmen)) {
                f.unternehmen = new.to_string();
            }
        }
    }

    pub fn len(&self) -> usize {
        self.dateien.len()
    }

    pub fn is_empty(&self) -> bool {
        self.dateien.is_empty()
    }
}

/// Liest die erste Seite direkt (ohne Zwischenspeicher), für die Kommandozeile.
/// `ocr`: auch Scans ohne Textebene lesen (langsam).
pub fn direct_reader(ocr: Option<Preference>) -> impl FnMut(&Path) -> anyhow::Result<Vec<PageText>> {
    move |path| match ocr {
        Some(pref) => crate::read_text(path, 1, pref).map(|(t, _)| t),
        None => af_pdf::extract_text(path, 1),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LearnStats {
    /// gefundene PDFs
    pub files: usize,
    /// daraus gelernt
    pub learned: usize,
    /// Name ohne verwertbare Wörter (nur Datum, Nummern)
    pub skipped_name: usize,
    /// freier Name, aber keine Wortfolge darin vom Brief als Firma bestätigt
    pub unconfirmed: usize,
    /// davon: kaum Text erkannt
    pub miss_no_text: usize,
    /// davon: Firma steht nur im Fließtext oder in der Anschrift
    pub miss_in_body: usize,
    /// davon: Firma steht nur ähnlich da (Lesefehler?)
    pub miss_similar: usize,
    /// davon: Firma steht gar nicht da (Logo als Bild, Abkürzung)
    pub miss_missing: usize,
    /// nicht lesbar
    pub errors: usize,
    /// schon in einem früheren Lauf gelernt und unverändert (übersprungen)
    pub already: usize,
    /// die nicht bestätigten Dateien, zum Durchgehen in der App
    pub unconfirmed_files: Vec<PathBuf>,
    /// davon gelernt, weil das Bild des Briefkopfs zu einem bekannten Unternehmen passt
    pub by_logo: usize,
    /// davon gelernt über Gruppen gleicher Briefköpfe mit derselben Firma im Dateinamen
    pub by_group: usize,
    /// bei schon gelernten Dateien nachgetragene Briefkopf-Fingerabdrücke
    pub logos_added: usize,
}

/// Fortschritt für die Anzeige.
pub struct Progress<'a> {
    pub done: usize,
    pub total: usize,
    pub current: &'a Path,
}

/// Alle PDFs unter `root`, rekursiv und sortiert. Versteckte Ordner (z. B. `.ablagefuchs`) bleiben außen vor,
/// symbolischen Links wird nicht gefolgt (keine Endlosschleifen).
pub fn collect_pdfs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.filter_map(Result::ok) {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(ft) = entry.file_type() else { continue };
            let path = entry.path();
            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Lernt aus allen benannten PDFs unter `root`, die laut `log` noch nicht gelernt sind. `read` liefert den Text
/// der ersten Seite (mit oder ohne Texterkennung, ggf. zwischengespeichert), `logo` den Fingerabdruck des
/// Briefkopfs. `progress` wird je Datei aufgerufen; gibt es `false` zurück, wird abgebrochen.
///
/// Erster Durchgang: Namen nach der Vorlage oder mit vom Brief bestätigter Firma. Zweiter Durchgang: Die übrigen
/// freien Namen, deren eindeutige Kennung (IBAN, USt-IdNr. …) inzwischen einem Unternehmen gehört, dessen Name
/// auch im Dateinamen steht (hilft bei Briefköpfen, die nur als Logo gedruckt sind). Dritter Durchgang: Der Brief
/// erwähnt irgendwo ein schon bestätigtes Unternehmen, das auch im Dateinamen steht. Davor: Der Briefkopf sieht aus
/// wie der eines bekannten Unternehmens, das auch im Dateinamen steht (Logo als Bild).
///
/// Schon gelernte Dateien aus älteren Lernständen bekommen ihren Briefkopf-Fingerabdruck nachgetragen.
pub fn learn_folder(
    root: &Path,
    template: &Template,
    knowledge: &mut Knowledge,
    log: &mut LearnLog,
    mut read: impl FnMut(&Path) -> anyhow::Result<Vec<PageText>>,
    mut logo: impl FnMut(&Path) -> Option<Fingerprint>,
    mut progress: impl FnMut(Progress<'_>) -> bool,
) -> LearnStats {
    let files = collect_pdfs(root);
    let mut stats = LearnStats {
        files: files.len(),
        ..LearnStats::default()
    };
    let mut pending: Vec<Pending> = Vec::new();
    for (i, path) in files.iter().enumerate() {
        if !progress(Progress {
            done: i,
            total: files.len(),
            current: path,
        }) {
            return stats;
        }
        if log.is_learned(path) {
            stats.already += 1;
            if let Some(entry) = log.dateien.get_mut(path)
                && !entry.briefkopf
            {
                if let Some(fp) = logo(path)
                    && knowledge.add_logo(&entry.unternehmen, fp)
                {
                    stats.logos_added += 1;
                }
                entry.briefkopf = true;
            }
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let by_template = template.parse_name(&name).filter(|f| !f.unternehmen.trim().is_empty());
        let named = match (by_template, LooseName::parse(&name)) {
            (Some(f), _) => Ok(f),
            (None, Some(l)) => Err(l),
            (None, None) => {
                stats.skipped_name += 1;
                continue;
            }
        };
        let pages = match read(path) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("{}: {e:#}", path.display());
                stats.errors += 1;
                continue;
            }
        };
        let layout = Layout::new(&pages);
        let ids = find_ids(&layout);
        let fp = logo(path);
        let fields = match named {
            Ok(f) => f,
            Err(loose) => {
                let texts = sender_texts(&layout);
                match loose.split(|n| matches_any(n, texts.iter().map(String::as_str))) {
                    Some(f) => f,
                    None => {
                        pending.push(Pending {
                            path: path.clone(),
                            loose,
                            ids,
                            texts: page_texts(&layout),
                            subject: subject_words(&layout),
                            logo: fp,
                        });
                        continue;
                    }
                }
            }
        };
        let company = learn_one(knowledge, fields, &layout, &ids, fp.as_ref());
        log.mark(path, &company);
        stats.learned += 1;
    }

    // Zweiter Durchgang: eindeutige Kennung eines bekannten Unternehmens, dessen Name im Dateinamen steht
    pending.retain(|p| {
        let Some(c) = knowledge
            .match_ids(&p.ids)
            .map(|m| knowledge.unternehmen[m.company()].clone())
        else {
            return true;
        };
        let Some((f, _)) = split_known(&p.loose, &c) else {
            return true;
        };
        knowledge.learn(&f, &p.ids, None, p.logo.as_ref(), None);
        log.mark(&p.path, &f.unternehmen);
        stats.learned += 1;
        false
    });

    // Briefkopf als Bild: sieht er aus wie der eines bekannten Unternehmens, das auch im Dateinamen steht
    pending.retain(|p| {
        let Some(c) = p
            .logo
            .as_ref()
            .and_then(|fp| knowledge.match_logo(fp))
            .map(|(i, _)| knowledge.unternehmen[i].clone())
        else {
            return true;
        };
        let Some((f, _)) = split_known(&p.loose, &c) else {
            return true;
        };
        knowledge.learn(&f, &p.ids, None, p.logo.as_ref(), None);
        log.mark(&p.path, &f.unternehmen);
        stats.learned += 1;
        stats.by_logo += 1;
        false
    });

    // Dritter Durchgang: Der Dateiname nennt ein Unternehmen, das andere Briefe schon bestätigt haben, und der Brief
    // erwähnt es irgendwo (auch im Fließtext, auch mit Lesefehlern). Themenwörter sind nie bestätigte
    // Unternehmen und können so nicht als Firma durchrutschen. Passen mehrere, wird nichts gelernt.
    let known = knowledge.unternehmen.clone();
    pending.retain(|p| {
        let candidates: Vec<NameFields> = known
            .iter()
            .filter_map(|c| {
                let (f, run) = split_known(&p.loose, c)?;
                // Im Brief steht der Name des Unternehmens, ein Alias oder die Wortfolge aus dem Dateinamen
                let in_letter = std::iter::once(&c.name)
                    .chain(c.aliase.iter())
                    .map(|n| normalize_name(n))
                    .chain(std::iter::once(run))
                    .any(|n| mention(&p.texts, &n) != Mention::No);
                in_letter.then_some(f)
            })
            .collect();
        let [f] = &candidates[..] else { return true };
        knowledge.learn(f, &p.ids, None, p.logo.as_ref(), None);
        log.mark(&p.path, &f.unternehmen);
        stats.learned += 1;
        false
    });

    // Gruppen gleicher Briefköpfe: Firmen, die ihren Namen nie als Text drucken, kennt das Wissen noch nicht.
    // Steht in fast allen Dateinamen einer Gruppe dieselbe Firma, belegen die Briefe einander.
    let groups = group_by_logo(&pending);
    let topic_words: std::collections::HashSet<String> = knowledge
        .unternehmen
        .iter()
        .flat_map(|c| c.themen.keys())
        .flat_map(|t| normalize_name(t).split(' ').map(String::from).collect::<Vec<_>>())
        .filter(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 4)
        .collect();
    // In wie vielen Gruppen (Einzeldateien zählen als eigene Gruppe) kommt eine Wortfolge vor?
    let runs_of: Vec<std::collections::HashSet<String>> = pending
        .iter()
        .map(|p| p.loose.candidate_runs().into_iter().collect())
        .collect();
    let mut learned_here = vec![false; pending.len()];
    for (g, group) in groups.iter().enumerate() {
        let members: Vec<GroupMember<'_>> = group
            .iter()
            .map(|&i| GroupMember {
                name: &pending[i].loose,
                subject: &pending[i].subject,
            })
            .collect();
        let elsewhere = |run: &str| {
            groups
                .iter()
                .enumerate()
                .filter(|&(h, other)| h != g && other.iter().any(|&i| runs_of[i].contains(run)))
                .count()
        };
        let Some(run) = company_of_group(&members, &topic_words, elsewhere) else {
            continue;
        };
        for &i in group {
            let p = &pending[i];
            let Some(f) = p.loose.split(|n| (n == run).then_some(false)) else {
                continue;
            };
            knowledge.learn(&f, &p.ids, None, p.logo.as_ref(), None);
            log.mark(&p.path, &f.unternehmen);
            stats.learned += 1;
            stats.by_group += 1;
            learned_here[i] = true;
        }
    }

    for (p, learned) in pending.into_iter().zip(learned_here) {
        if learned {
            continue;
        }
        stats.unconfirmed += 1;
        *match p.loose.miss_reason(&p.texts) {
            Miss::NoText => &mut stats.miss_no_text,
            Miss::InBody => &mut stats.miss_in_body,
            Miss::Similar => &mut stats.miss_similar,
            Miss::Missing => &mut stats.miss_missing,
        } += 1;
        stats.unconfirmed_files.push(p.path);
    }
    stats
}

/// Eine Datei, deren Firma der erste Durchgang nicht bestätigen konnte.
struct Pending {
    path: PathBuf,
    loose: LooseName,
    ids: Vec<FoundId>,
    /// Zeilen der ersten Seite in Vergleichsform
    texts: Vec<String>,
    /// Wörter des Betreffs in Vergleichsform
    subject: Vec<String>,
    logo: Option<Fingerprint>,
}

/// Gruppiert Dateien nach gleichem Briefkopf (Nummern in `pending`); Dateien ohne Briefkopf bilden eigene Gruppen.
fn group_by_logo(pending: &[Pending]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, p) in pending.iter().enumerate() {
        let joined = p.logo.as_ref().and_then(|fp| {
            groups.iter_mut().find(|g| {
                pending[g[0]]
                    .logo
                    .as_ref()
                    .is_some_and(|first| first.similarity(fp) >= af_core::logo::SAME)
            })
        });
        match joined {
            Some(g) => g.push(i),
            None => groups.push(vec![i]),
        }
    }
    groups
}

/// Teilt einen freien Namen mit einem bekannten Unternehmen (Name, Alias oder deren Anfang) und nimmt dessen
/// Schreibweise, auch wenn im Dateinamen nur ein Teil davon steht. Gibt auch die Wortfolge aus dem Dateinamen
/// zurück (Vergleichsform).
fn split_known(loose: &LooseName, c: &af_core::knowledge::Company) -> Option<(NameFields, String)> {
    let names: Vec<String> = std::iter::once(&c.name)
        .chain(c.aliase.iter())
        .map(|n| normalize_name(n))
        .collect();
    let mut f = loose.split(|n| matches_any(n, names.iter().map(String::as_str)))?;
    let run = normalize_name(&f.unternehmen);
    f.unternehmen = c.name.clone();
    Some((f, run))
}

/// Lernt eine Datei aus dem ersten Durchgang; gibt den Namen zurück, unter dem das Unternehmen geführt wird.
fn learn_one(
    knowledge: &mut Knowledge,
    mut fields: NameFields,
    layout: &Layout,
    ids: &[FoundId],
    logo: Option<&Fingerprint>,
) -> String {
    // Nur ein deutlich gestalteter Briefkopf taugt als Alias (sonst lernt man z. B. „Herrn“)
    let letterhead = af_core::company::detect(layout)
        .filter(|c| c.confidence >= 60)
        .map(|c| c.name);
    // Dateinamen kennen keine Umlaute („Sued“): stimmt der Briefkopf überein, dessen Schreibweise nehmen
    if let Some(l) = &letterhead
        && normalize_name(l) == normalize_name(&fields.unternehmen)
    {
        fields.unternehmen = l.clone();
    }
    knowledge.learn(&fields, ids, letterhead.as_deref(), logo, None);
    knowledge
        .find_company(&fields.unternehmen)
        .or_else(|| letterhead.as_deref().and_then(|l| knowledge.find_company(l)))
        .map_or(fields.unternehmen, |i| knowledge.unternehmen[i].name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learn_log_marks_and_forgets() {
        let dir = std::env::temp_dir().join(format!("af-lernstand-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("2025-01-01_Stadtwerke_Abschlag.pdf");
        std::fs::write(&file, b"%PDF-1.4").unwrap();

        let mut log = LearnLog::default();
        assert!(!log.is_learned(&file));
        log.mark(&file, "Stadtwerke");
        assert!(log.is_learned(&file));

        let saved = dir.join("lernstand.json");
        log.save(&saved).unwrap();
        let mut log = LearnLog::load(&saved).unwrap();
        assert!(log.is_learned(&file));

        // umbenannt: Datei bleibt gelernt, unter neuem Namen
        log.rename_company(&["Stadtwerke"], "Stadtwerke Musterstadt");
        assert_eq!(log.dateien[&file].unternehmen, "Stadtwerke Musterstadt");
        // geänderte Datei wird neu gelernt
        std::fs::write(&file, b"%PDF-1.4 geaendert").unwrap();
        assert!(!log.is_learned(&file));
        log.mark(&file, "Stadtwerke Musterstadt");
        // vergessenes Unternehmen: seine Dateien dürfen wieder gelernt werden
        log.forget_company(&["Stadtwerke Musterstadt GmbH"]);
        assert!(log.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
