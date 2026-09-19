// SPDX-License-Identifier: MIT OR Apache-2.0
//! PDF-Zugriff für Ablagefuchs. Rendern und Text mit Positionen über pdfium,
//! Metadaten schreiben über lopdf (ab M6).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, anyhow};
use pdfium_render::prelude::*;

static PDFIUM: OnceLock<Pdfium> = OnceLock::new();

/// Gemeinsame pdfium-Instanz. Beim ersten Aufruf wird die Bibliothek gesucht und geladen.
pub fn pdfium() -> anyhow::Result<&'static Pdfium> {
    if let Some(p) = PDFIUM.get() {
        return Ok(p);
    }
    let bindings = bind()?;
    Ok(PDFIUM.get_or_init(|| Pdfium::new(bindings)))
}

/// Anzahl der Seiten eines PDFs.
pub fn page_count(path: &Path) -> anyhow::Result<usize> {
    let doc = pdfium()?
        .load_pdf_from_file(path, None)
        .with_context(|| format!("PDF lässt sich nicht öffnen: {}", path.display()))?;
    Ok(usize::try_from(doc.pages().len()).unwrap_or(0))
}

/// Orte, an denen nach der pdfium-Bibliothek gesucht wird, in dieser Reihenfolge:
/// `AF_PDFIUM` (Datei oder Ordner), neben dem Programm, `../Frameworks` im macOS-App-Bundle,
/// `vendor/pdfium/<plattform>` im Quellbaum (von `cargo xtask fetch-pdfium`).
/// Danach versucht [`bind`] noch die Systembibliothek.
pub fn search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(p) = std::env::var_os("AF_PDFIUM").map(PathBuf::from) {
        paths.push(if p.is_dir() {
            Pdfium::pdfium_platform_library_name_at_path(&p)
        } else {
            p
        });
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        paths.push(Pdfium::pdfium_platform_library_name_at_path(&dir));
        paths.push(Pdfium::pdfium_platform_library_name_at_path(&dir.join("../Frameworks")));
    }
    let vendor = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/pdfium")
        .join(platform());
    for sub in ["lib", "bin"] {
        paths.push(Pdfium::pdfium_platform_library_name_at_path(&vendor.join(sub)));
    }
    paths
}

fn bind() -> anyhow::Result<Box<dyn PdfiumLibraryBindings>> {
    for path in search_paths() {
        if path.is_file() {
            log::debug!("Lade pdfium: {}", path.display());
            return Pdfium::bind_to_library(&path)
                .map_err(|e| anyhow!("pdfium aus {} nicht ladbar: {e}", path.display()));
        }
    }
    Pdfium::bind_to_system_library()
        .map_err(|e| anyhow!("pdfium nicht gefunden ({e}). Für die Entwicklung: `cargo xtask fetch-pdfium` ausführen."))
}

/// Plattformkennung wie bei den pdfium-Binärpaketen (`mac-arm64`, `win-x64`, …).
pub fn platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "mac-arm64",
        ("macos", _) => "mac-x64",
        ("windows", "aarch64") => "win-arm64",
        ("windows", _) => "win-x64",
        ("linux", "aarch64") => "linux-arm64",
        _ => "linux-x64",
    }
}
