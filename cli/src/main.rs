// SPDX-License-Identifier: MIT OR Apache-2.0
//! Ablagefuchs auf der Kommandozeile. Die Befehle (`suggest`, `rename`, `learn`, `undo`, `text`)
//! folgen ab Meilenstein M1; bisher nur ein Selbsttest, ob pdfium gefunden wird.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version" | "-V") => {
            println!("ablagefuchs-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("pages") if args.len() == 2 => match af_pdf::page_count(std::path::Path::new(&args[1])) {
            Ok(n) => {
                println!("{n}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("Fehler: {e:#}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("Aufruf: ablagefuchs-cli --version | pages <datei.pdf>");
            ExitCode::from(2)
        }
    }
}
