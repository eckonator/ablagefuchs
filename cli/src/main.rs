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
  ablagefuchs-cli suggest [--json | --names] [--tesseract] <datei.pdf | ordner> …
      Namensvorschlag mit Sicherheit und Herkunft je Feld (Scans ohne Text per Texterkennung)
  ablagefuchs-cli text [--json] <datei.pdf>
      erkannter Text als Zeilen mit Zone und Schrift (Fehlersuche)
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
            suggest(&paths, mode, ocr)
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

fn suggest(paths: &[PathBuf], mode: Output, ocr_pref: af_ocr::Preference) -> anyhow::Result<()> {
    let template = Template::parse(DEFAULT_TEMPLATE)?;
    let opts = NamingOptions::default();
    let mut results = Vec::new();
    for file in collect_pdfs(paths)? {
        let (pages, source) = af_ocr::read_text(&file, ANALYZE_PAGES, ocr_pref)?;
        let ocr_quality = match &source {
            af_core::TextSource::Ocr { mean_confidence, .. } => Some(*mean_confidence),
            af_core::TextSource::Embedded => None,
        };
        let ctx = Context {
            reference_date: modified_date(&file),
            ocr_quality,
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
