// SPDX-License-Identifier: MIT OR Apache-2.0
//! Ablagefuchs auf der Kommandozeile: dieselbe Erkennung wie in der App, zum Testen, für Skripte
//! und für andere Programme wie lokale Assistenten (`--json`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use af_core::dates::DateKind;
use af_core::naming::{DEFAULT_TEMPLATE, NamingOptions, Template};
use af_core::topic::TopicSource;
use af_core::{Context, Source, Suggestion};
use anyhow::{Context as _, bail};
use serde::Serialize;

const USAGE: &str = "\
Aufruf:
  ablagefuchs-cli suggest [--json | --names] [--tesseract] [--wissen=DATEI] <datei.pdf | ordner> …
      Namensvorschlag mit Sicherheit und Herkunft je Feld (Scans ohne Text per Texterkennung)
  ablagefuchs-cli learn [--ocr] [--wissen=DATEI] <ordner>
      Wissen aus einem benannten Archiv lernen (rekursiv, alle Unterordner; nur lokal)
  ablagefuchs-cli text [--json] <datei.pdf>
      erkannter Text als Zeilen mit Zone und Schrift (Fehlersuche)
  ablagefuchs-cli logo <datei.pdf> …
      Ähnlichkeit der Briefköpfe als Matrix (nur Zahlen, zum Abstimmen des Logo-Vergleichs)
  ablagefuchs-cli pages <datei.pdf>
  ablagefuchs-cli --version";

/// Für die Erkennung reichen die ersten beiden Seiten.
const ANALYZE_PAGES: usize = 2;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Fehler: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> anyhow::Result<()> {
    let flags: Vec<&str> = args
        .iter()
        .skip(1)
        .filter(|a| a.starts_with("--"))
        .map(String::as_str)
        .collect();
    let paths: Vec<PathBuf> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    match args.first().map(String::as_str) {
        Some("--version" | "-V") => {
            println!("ablagefuchs-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("pages") if paths.len() == 1 => {
            println!("{}", af_pdf::page_count(&paths[0])?);
            Ok(())
        }
        Some("text") if paths.len() == 1 => text(&paths[0], flags.contains(&"--json")),
        Some("learn") if paths.len() == 1 => {
            let path = knowledge_path(&flags)?;
            let mut knowledge = af_core::knowledge::Knowledge::load(&path)?;
            let template = Template::parse(DEFAULT_TEMPLATE)?;
            let ocr = flags.contains(&"--ocr").then_some(af_ocr::Preference::Auto);
            // Lernstand neben der Wissensdatei: schon gelernte Dateien werden übersprungen
            let log_path = path.with_file_name("lernstand.json");
            let mut log = af_ocr::archive::LearnLog::load(&log_path)?;
            let stats = af_ocr::archive::learn_folder(
                &paths[0],
                &template,
                &mut knowledge,
                &mut log,
                af_ocr::archive::direct_reader(ocr),
                |p| af_ocr::letterhead_fingerprint(p).map(|h| h.fingerprint),
                |p| {
                    eprint!("\r{} / {}", p.done + 1, p.total);
                    true
                },
            );
            eprintln!();
            knowledge.save(&path)?;
            log.save(&log_path)?;
            println!(
                "{} PDFs, {} gelernt, {} schon bekannt, {} ohne Firma im Namen, {} Firma nicht im Brief bestätigt, {} nicht lesbar → {} Unternehmen in {}",
                stats.files,
                stats.learned,
                stats.already,
                stats.skipped_name,
                stats.unconfirmed,
                stats.errors,
                knowledge.unternehmen.len(),
                path.display()
            );
            if stats.by_logo + stats.by_group + stats.logos_added > 0 {
                println!(
                    "Briefkopf-Bild: {} über bekannte Logos gelernt, {} über Gruppen gleicher Briefköpfe, {} Fingerabdrücke nachgetragen",
                    stats.by_logo, stats.by_group, stats.logos_added
                );
            }
            if stats.unconfirmed > 0 {
                println!(
                    "Nicht bestätigt: {} ohne erkannten Text, {} Firma nur im Fließtext, {} ähnlich geschrieben, {} gar nicht gefunden",
                    stats.miss_no_text, stats.miss_in_body, stats.miss_similar, stats.miss_missing
                );
            }
            Ok(())
        }
        Some("logo") if !paths.is_empty() => {
            // Nummern statt Dateinamen: die Ausgabe lässt sich weitergeben, ohne Inhalte zu verraten
            let prints: Vec<_> = paths
                .iter()
                .map(|p| af_ocr::letterhead_fingerprint(p).map(|h| h.fingerprint))
                .collect();
            for (i, p) in paths.iter().enumerate() {
                eprintln!("{:>3}: {}", i + 1, p.display());
            }
            print!("    ");
            for j in 0..prints.len() {
                print!("{:>6}", j + 1);
            }
            println!();
            for (i, a) in prints.iter().enumerate() {
                print!("{:>3} ", i + 1);
                for b in &prints {
                    match (a, b) {
                        (Some(a), Some(b)) => print!("{:>6.2}", a.similarity(b)),
                        _ => print!("{:>6}", "-"),
                    }
                }
                println!();
            }
            Ok(())
        }
        Some("suggest") if !paths.is_empty() => {
            let mode = if flags.contains(&"--json") {
                Output::Json
            } else if flags.contains(&"--names") {
                Output::Names
            } else {
                Output::Human
            };
            let ocr = if flags.contains(&"--tesseract") {
                af_ocr::Preference::Tesseract
            } else {
                af_ocr::Preference::Auto
            };
            let knowledge = match flags.iter().any(|f| f.starts_with("--wissen=")) {
                true => Some(af_core::knowledge::Knowledge::load(&knowledge_path(&flags)?)?),
                false => None,
            };
            suggest(&paths, mode, ocr, knowledge.as_ref())
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}

#[derive(PartialEq)]
enum Output {
    Human,
    Names,
    Json,
}

#[derive(Serialize)]
struct FileSuggestion {
    file: PathBuf,
    name: String,
    /// schwächstes Feld, entscheidet über „Prüfen“ und Stapelmodus
    confidence: u8,
    suggestion: Suggestion,
}

fn suggest(
    paths: &[PathBuf],
    mode: Output,
    ocr_pref: af_ocr::Preference,
    knowledge: Option<&af_core::knowledge::Knowledge>,
) -> anyhow::Result<()> {
    let template = Template::parse(DEFAULT_TEMPLATE)?;
    let opts = NamingOptions::default();
    let mut results = Vec::new();
    for file in collect_pdfs(paths)? {
        let (pages, source) = af_ocr::read_text(&file, ANALYZE_PAGES, ocr_pref)?;
        let ocr_quality = match &source {
            af_core::TextSource::Ocr { mean_confidence, .. } => Some(*mean_confidence),
            af_core::TextSource::Embedded => None,
        };
        // Briefkopf-Bild nur, wenn es Wissen gibt, mit dem es verglichen werden kann
        let logo = knowledge.and(af_ocr::letterhead_fingerprint(&file));
        let ctx = Context {
            reference_date: modified_date(&file),
            ocr_quality,
            knowledge,
            logo: logo.as_ref(),
            created: af_pdf::created_date(&file),
        };
        let suggestion = af_core::analyze(&pages, &ctx);
        let name = template.file_name(&suggestion.fields(), &opts);
        let r = FileSuggestion {
            confidence: suggestion.min_confidence(),
            file,
            name,
            suggestion,
        };
        match mode {
            Output::Names if r.suggestion.is_complete() => println!("{}\t{}", file_name(&r.file), r.name),
            Output::Names => println!("{}\t-", file_name(&r.file)),
            Output::Human => print_human(&r),
            Output::Json => {}
        }
        results.push(r);
    }
    if mode == Output::Json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    }
    Ok(())
}

fn print_human(r: &FileSuggestion) {
    let s = &r.suggestion;
    println!("{}", file_name(&r.file));
    if s.is_complete() {
        println!("  → {}   (Sicherheit {} %)", r.name, r.confidence);
    } else {
        println!("  → kein vollständiger Vorschlag, bitte von Hand ausfüllen");
    }
    let datum = s
        .datum
        .value
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "–".into());
    let row = |label: &str, value: &str, conf: u8, src: &Source| {
        println!("    {label:<12} {value:<34} {conf:>3} %  {}", describe(src));
    };
    row("Datum", &datum, s.datum.confidence, &s.datum.source);
    row(
        "Unternehmen",
        &s.unternehmen.value,
        s.unternehmen.confidence,
        &s.unternehmen.source,
    );
    row("Thema", &s.thema.value, s.thema.confidence, &s.thema.source);
    row(
        "Rechnung",
        if s.rechnung.value { "ja" } else { "nein" },
        s.rechnung.confidence,
        &s.rechnung.source,
    );
    println!();
}

/// Herkunft als kurzer deutscher Satz (die App übersetzt das später über i18n).
fn describe(source: &Source) -> String {
    match source {
        Source::Nothing => "nichts gefunden".into(),
        Source::Date(kind) => match kind {
            DateKind::Document => "als Dokumentdatum ausgewiesen",
            DateKind::Unknown => "einziges/plausibelstes Datum",
            _ => "unsicher (kein Dokumentdatum gefunden)",
        }
        .into(),
        Source::Letterhead { sender_line: true } => "Briefkopf, bestätigt durch Absenderzeile".into(),
        Source::Letterhead { sender_line: false } => "Briefkopf (größte Schrift)".into(),
        Source::Topic(TopicSource::SubjectLine) => "aus der Betreffzeile".into(),
        Source::Topic(TopicSource::LineItem) => "aus der ersten Rechnungsposition".into(),
        Source::InvoiceSignals(s) if s.is_empty() => "keine Rechnungsmerkmale".into(),
        Source::InvoiceSignals(s) => format!("Merkmale: {}", s.join(", ")),
        Source::Knowledge {
            kennung: Some(k),
            dokumente,
        } => format!("gelernt über {k:?} ({dokumente} Dokumente)"),
        Source::Knowledge {
            kennung: None,
            dokumente,
        } => format!("gelernt über den Namen ({dokumente} Dokumente)"),
        Source::KnownTopic { anzahl } => format!("häufigstes Thema dieses Absenders ({anzahl}×)"),
        Source::Created { aus_pdf: true } => "kein Datum im Dokument: Erstellungsdatum der PDF".into(),
        Source::Created { aus_pdf: false } => "kein Datum im Dokument: Datum der Datei".into(),
        Source::Logo {
            aehnlichkeit,
            dokumente,
        } => format!("am Bild des Briefkopfs erkannt ({aehnlichkeit} % ähnlich, {dokumente} Dokumente)"),
    }
}

fn text(path: &Path, json: bool) -> anyhow::Result<()> {
    let (pages, _) = af_ocr::read_text(path, usize::MAX, af_ocr::Preference::Auto)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&pages)?);
        return Ok(());
    }
    let layout = af_core::layout::Layout::new(&pages);
    for l in &layout.lines {
        let bold = if l.bold { "fett" } else { "" };
        println!(
            "S{} {:<6} y={:>5.0} x={:>5.0} {:>4.1}pt {:<4} {}",
            l.page,
            format!("{:?}", l.zone),
            l.bbox.y,
            l.bbox.x,
            l.font_size,
            bold,
            l.text
        );
    }
    Ok(())
}

/// `--wissen=DATEI` oder die Wissensdatei der App im Einstellungsordner.
fn knowledge_path(flags: &[&str]) -> anyhow::Result<PathBuf> {
    if let Some(p) = flags.iter().find_map(|f| f.strip_prefix("--wissen=")) {
        return Ok(PathBuf::from(p));
    }
    let dir = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("kein Einstellungsordner"))?;
    Ok(dir.join("Ablagefuchs").join("wissen.json"))
}

/// Dateien und Ordner (nicht rekursiv) zu einer sortierten Liste von PDFs.
fn collect_pdfs(paths: &[PathBuf]) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            let mut files: Vec<PathBuf> = std::fs::read_dir(p)
                .with_context(|| format!("Ordner nicht lesbar: {}", p.display()))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|f| f.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
                .collect();
            files.sort();
            out.extend(files);
        } else if p.is_file() {
            out.push(p.clone());
        } else {
            bail!("Nicht gefunden: {}", p.display());
        }
    }
    Ok(out)
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Änderungsdatum der Datei als Bezugsdatum (meist der Scantag).
fn modified_date(p: &Path) -> Option<chrono::NaiveDate> {
    let secs = std::fs::metadata(p)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(chrono::DateTime::from_timestamp(i64::try_from(secs).ok()?, 0)?.date_naive())
}
