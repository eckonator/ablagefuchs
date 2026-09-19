# Ablagefuchs

**Rename scanned documents in seconds.** Ablagefuchs shows your scanned PDFs one by one, recognizes
date, company and topic, and suggests a file name like

```
2026-09-28_Stadtwerke_Musterstadt_-_Jahresabrechnung_Strom_Rechnung.pdf
```

Press Enter to accept, or fix a field and go on. A desktop app for macOS, Windows and Linux,
written in Rust. Works completely offline: no server, no database, no cloud.

> **Status: early development.** Nothing to download yet.

## Planned features

- Preview with highlighted findings; click a date or select text in the document to use it
- Recognition of date, sender, topic and invoices, with a confidence score and its source for every field
- Learns from your existing archive and from your corrections
- File name template you can configure (`{datum}_{unternehmen}_-_{thema}{rechnung?:_Rechnung}`)
- Batch mode for files recognized with high confidence, undo for every rename
- Writes PDF metadata (title, keywords, Finder tags) and an OCR text layer for full-text search
- Built-in OCR of each system: Apple Vision (macOS), Windows OCR, Tesseract (Linux)
- User interface in English and German

## Building

You need a current Rust toolchain ([rustup.rs](https://rustup.rs)). On Linux you also need the
X11/Wayland development packages, e.g. on Debian/Ubuntu:

```sh
sudo apt install libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev pkg-config
```

```sh
cargo xtask fetch-pdfium      # downloads the PDFium library into vendor/ (needed once)
cargo xtask demo-pdfs         # generates sample scans of fictional companies into demo/Eingang
cargo run -p ablagefuchs      # starts the app
cargo test --workspace        # runs the tests
```

## Architecture

| Part | Purpose |
|---|---|
| `crates/core` | Recognition, file name template, learned knowledge, journal. No PDF or GUI dependencies, fully tested. |
| `crates/pdf` | PDF access: rendering and text with positions via [PDFium](https://pdfium.googlesource.com/pdfium/), metadata via lopdf. |
| `crates/ocr` | Text recognition for scans without a text layer, using the OCR engine built into each system. |
| `cli` | `ablagefuchs-cli` for scripting and debugging. |
| `app` | The user interface (egui). |
| `xtask` | Developer tools (`cargo xtask …`). |

Code comments are in German; issues and contributions in English are welcome.

## License

Copyright © 2026 Markus Eckert

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

Releases will bundle [PDFium](https://pdfium.googlesource.com/pdfium/) (BSD-3-Clause / Apache-2.0,
binaries from [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)).
