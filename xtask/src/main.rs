// SPDX-License-Identifier: MIT OR Apache-2.0
//! Entwicklerwerkzeuge, Aufruf über `cargo xtask <befehl>`:
//!
//! - `fetch-pdfium [plattform …]` lädt die pdfium-Bibliothek nach `vendor/pdfium/<plattform>/`
//!   (ohne Angabe für dieses System; Plattformen: mac-arm64, mac-x64, win-x64, linux-x64 …)
//! - `demo-pdfs [ordner]` erzeugt Beispiel-Scans fiktiver Firmen (Standard: `demo/Eingang`)

mod demo;

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

/// pdfium-Version von github.com/bblanchon/pdfium-binaries. Muss zum Feature `pdfium_<nr>`
/// von pdfium-render in der Cargo.toml des Workspaces passen.
const PDFIUM_VERSION: &str = "7881";

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = workspace_root();
    match args.first().map(String::as_str) {
        Some("fetch-pdfium") => {
            let platforms: Vec<&str> = if args.len() > 1 {
                args[1..].iter().map(String::as_str).collect()
            } else {
                vec![current_platform()]
            };
            for p in platforms {
                fetch_pdfium(&root, p)?;
            }
            Ok(())
        }
        Some("demo-pdfs") => {
            let dir = args
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("demo/Eingang"));
            demo::write_all(&dir)
        }
        _ => {
            eprintln!("Aufruf: cargo xtask fetch-pdfium [plattform …] | demo-pdfs [ordner]");
            std::process::exit(2);
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask liegt im Workspace")
        .to_path_buf()
}

fn current_platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "mac-arm64",
        ("macos", _) => "mac-x64",
        ("windows", "aarch64") => "win-arm64",
        ("windows", _) => "win-x64",
        ("linux", "aarch64") => "linux-arm64",
        _ => "linux-x64",
    }
}

fn fetch_pdfium(root: &Path, platform: &str) -> anyhow::Result<()> {
    let dest = root.join("vendor/pdfium").join(platform);
    let marker = dest.join("VERSION.ablagefuchs");
    if std::fs::read_to_string(&marker).is_ok_and(|v| v.trim() == PDFIUM_VERSION) {
        println!(
            "pdfium {PDFIUM_VERSION} für {platform} ist schon da: {}",
            dest.display()
        );
        return Ok(());
    }
    let url = format!(
        "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F{PDFIUM_VERSION}/pdfium-{platform}.tgz"
    );
    println!("Lade {url}");
    // curl gibt es auf macOS, Windows 10+ und Linux; folgt der Weiterleitung von GitHub zuverlässig
    let archive = root.join(format!("vendor/pdfium-{platform}.tgz"));
    std::fs::create_dir_all(root.join("vendor"))?;
    let status = std::process::Command::new("curl")
        .args(["-fsSL", "--retry", "3", "-o"])
        .arg(&archive)
        .arg(&url)
        .status()
        .context("curl nicht gefunden")?;
    if !status.success() {
        bail!("Download fehlgeschlagen ({status}): {url}");
    }
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    std::fs::create_dir_all(&dest)?;
    let gz = flate2::read::GzDecoder::new(std::fs::File::open(&archive)?);
    tar::Archive::new(gz)
        .unpack(&dest)
        .context("Entpacken fehlgeschlagen")?;
    std::fs::remove_file(&archive)?;
    std::fs::write(&marker, PDFIUM_VERSION)?;
    println!("pdfium {PDFIUM_VERSION} für {platform} liegt in {}", dest.display());
    Ok(())
}
