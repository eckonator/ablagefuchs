# Ablagefuchs – Bauplan

> Open-Source-Desktop-App (macOS zuerst, dazu Windows und Linux), die gescannte PDFs sichtet,
> Datum, Unternehmen und Thema erkennt und sie nach einem frei einstellbaren Schema umbenennt.
> Stand: 2026-10-13 · GUI-Vorlage: [`mockup.html`](mockup.html)

---

## 1. Ziel und Abgrenzung

**Problem:** Der Scanner legt Dateien wie `scan_20261002_0001.pdf` im Eingangsordner auf dem NAS ab.
Jede Datei muss geöffnet, gelesen und von Hand umbenannt werden, z. B. in
`2026-09-28_Stadtwerke_Musterstadt_-_Jahresabrechnung_Strom_Rechnung.pdf`.

**Lösung:** Ablagefuchs zeigt Datei für Datei eine Vorschau mit fertigem Namensvorschlag.
Man bestätigt mit `↵` oder korrigiert. Dateien, bei denen die Erkennung sehr sicher ist, gehen
im Stapelmodus ohne Prüfung durch.

**Grundsätze**

| Grundsatz | Bedeutung |
|---|---|
| Dateisystem ist die Wahrheit | Keine Datenbank, kein Import. Ordner wählen und loslegen (wie bei FadeWerk DJ). Gelerntes Wissen ist nur Zusatz und lässt sich jederzeit neu aufbauen. |
| Offline und privat | Standardmäßig keine Netzwerkzugriffe, keine Telemetrie. KI ist aus und läuft, wenn eingeschaltet, nur lokal (Ollama). |
| Nichts geht verloren | Jede Änderung wird protokolliert und lässt sich rückgängig machen. Schreibvorgänge laufen über eine Zwischendatei und werden dann atomar umbenannt. |
| Tastatur zuerst | Jeder Arbeitsschritt geht ohne Maus. |
| Nachvollziehbar | Zu jedem Vorschlag sieht man, wie sicher er ist und woher er kommt (Briefkopf, USt-IdNr., gelernt …). |

**Nicht Ziel (vorerst):** Dokumentenverwaltung mit Suche/Datenbank (dafür gibt es Paperless-ngx),
Server-Betrieb, Mobil-Apps, Scanner-Ansteuerung.

---

## 2. Festgelegte Entscheidungen

| Thema | Entscheidung |
|---|---|
| Name | **Ablagefuchs** überall (Fenster, `Ablagefuchs.app`, `Ablagefuchs.exe`, Einstellungsordner). Ohne Großbuchstaben: `ablagefuchs`. |
| Kennungen | Bundle-ID `de.eckonator.ablagefuchs`, Repo `github.com/eckonator/ablagefuchs`, interne Crates `af-*`, Entwicklerschalter `AF_*` |
| Lizenz | **MIT OR Apache-2.0** (Dual-Lizenz, Rust-üblich). Abhängigkeiten dürfen nur MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0 oder Unicode sein, **kein GPL/AGPL** (also kein MuPDF). Geprüft wird automatisch mit `cargo-deny`. |
| GUI | **egui/eframe** (reines Rust), wie FadeWerk DJ |
| Grafik | wgpu, unter **Windows nur DirectX 12** (kein Vulkan), automatischer Ausweichweg auf OpenGL. Erkenntnis aus FadeWerk DJ, siehe §7.8. |
| Layout | 3 Spalten: Dateiliste · Vorschau · Formular. Die Dateiliste lässt sich mit `⌘L` ausblenden. |
| Dateiname | Vorlage `{datum}_{unternehmen}_-_{thema}{rechnung?:_Rechnung}` |
| Zeichen | Leerzeichen → `_`, Umlaute → ae/oe/ue/ss, nur `A–Z a–z 0–9 _ -`. `_-_` trennt nur Unternehmen und Thema (siehe §6). |
| Rechnung | Zusatz `_Rechnung` am Ende. Endet das Thema schon auf „Rechnung“, wird es nicht doppelt angehängt. |
| Überspringen | `⌘↓` springt zur nächsten Datei, die übersprungene bleibt offen. `esc` verlässt nur das Eingabefeld. |
| Ablage | Standard: nur umbenennen. Verschieben nach `Archiv/{jahr}/{unternehmen}` lässt sich zuschalten. |
| Stapelmodus | übernimmt nur, wenn jedes Feld mindestens 90 % Sicherheit hat (einstellbar) |
| KI | Standard aus. Lokal über Ollama zuschaltbar. Cloud erst später, nur auf ausdrücklichen Wunsch (Opt-in). |
| Sprache | Oberfläche **komplett** auf Deutsch und Englisch. Umschaltbar in den Einstellungen (Seite „Allgemein“), wirkt sofort ohne Neustart. Standard ist die Systemsprache, sonst Englisch. Code-Kommentare und interne Doku auf Deutsch, README auf Englisch. |
| Über | Eigene Seite „Über“ in den Einstellungen: Version, Autor, Lizenz (MIT OR Apache-2.0) mit eingebetteten Lizenztexten, Fremdbestandteile, Quellcode-Link, Haftungsausschluss (siehe §7.6) |

### Namensprüfung (2026-10-13)

Ergebnis für „Ablagefuchs“:
- **GitHub:** 0 Repositories
- **crates.io:** frei
- **Domains:** `ablagefuchs.de` und `.com` nicht registriert
- **Websuche:** kein Produkt und keine Firma gefunden

Aussortierte Kandidaten:
- *BelegWerk:* Buchhaltungsbüro
- *Briefwerk:* aktives Unternehmen
- *Ablagio:* bestehendes Aufräum-Produkt
- *Stapelfix, Taufpate, Nomina, Titulus, Kladde:* belegt

**Noch zu tun vor der Veröffentlichung:**
- [ ] Markenrecherche beim DPMA (DPMAregister) und bei der EUIPO (TMview), Klassen 9 und 42
- [ ] Domains `ablagefuchs.de` / `.com` sichern (optional, z. B. für eine Projektseite)

---

## 3. Gesamtarchitektur

```
                         ┌──────────────────────────────────────────────┐
                         │ app (egui)                                   │
                         │  UI-Module zeichnen und geben Aktionen zurück│
                         │  app.rs = Logik, Zustand, Threads            │
                         └───────┬───────────────┬──────────────┬───────┘
                                 │               │              │
          ┌──────────────────────▼──┐  ┌─────────▼────────┐  ┌──▼────────────────┐
          │ af-core                 │  │ af-pdf           │  │ af-ocr            │
          │ Erkennung, Vorlage,     │  │ pdfium: Render,  │  │ Vision (macOS)    │
          │ Bereinigung, Wissen,    │  │ Text + Positionen│  │ Windows.Media.Ocr │
          │ Journal, Dateioperation │  │ lopdf: Metadaten │  │ Tesseract (Linux) │
          │ (keine PDF-/GUI-Abh.)   │  │ Textebene        │  │                   │
          └─────────────────────────┘  └──────────────────┘  └───────────────────┘
                     ▲                          ▲                    ▲
                     └──────────── cli (ablagefuchs-cli) ────────────┘
                                   af-ai (optional, Ollama)
```

### Workspace

```
ablagefuchs/
├── Cargo.toml                 # [workspace], gemeinsame Versionen
├── crates/
│   ├── core/    (af-core)     # reine Logik, vollständig getestet, läuft ohne PDF
│   ├── pdf/     (af-pdf)      # pdfium-render + lopdf
│   ├── ocr/     (af-ocr)      # Trait OcrEngine + Plattform-Implementierungen
│   └── ai/      (af-ai)       # optional: Ollama-Client (Feature „ai“)
├── cli/         (ablagefuchs-cli)
├── app/         (ablagefuchs) # egui-Oberfläche
├── xtask/                     # Entwicklerwerkzeuge: Demo-PDFs, pdfium holen
├── assets/                    # icon.svg, Schriften, dmg-background.svg, .desktop
├── scripts/                   # bundle-macos.sh, make-dmg.sh, install-macos.sh, make-icon.sh
├── docs/                      # Screenshots fürs README
├── tests/fixtures/            # anonymisierte Texte + erwartete Ergebnisse
├── .github/workflows/         # ci.yml, release.yml
├── deny.toml                  # Lizenz-/Sicherheitsprüfung
├── LICENSE-MIT, LICENSE-APACHE, README.md
```

**Regeln (übernommen von FadeWerk DJ)**
- `af-core` hängt weder von PDF noch von GUI ab. Er bekommt Text mit Positionen (`PageText`) und liefert Vorschläge. Damit ist er schnell und deterministisch testbar.
- UI-Module zeichnen nur und geben Aktionen zurück (`FormAction`, `ListAction`, …). Die Logik liegt in `app/src/app.rs`.
- Lange Arbeit läuft nie im UI-Thread: Analyse, Rendern, OCR und Schreiben laufen in Worker-Threads, die über `crossbeam-channel` Nachrichten schicken. Danach folgt `ctx.request_repaint()`.
- Edition 2024, `cargo clippy --workspace -- -D warnings` muss sauber sein.

### Wichtige Abhängigkeiten (Versionen bei Projektstart prüfen)

| Zweck | Crate | Lizenz | Hinweis |
|---|---|---|---|
| GUI | `eframe`, `egui`, `egui_extras` | MIT/Apache | wie FadeWerk DJ (0.36 o. neuer), Renderer wgpu mit Ausweichweg glow |
| PDF rendern + Text mit Positionen | `pdfium-render` | MIT/Apache | lädt `libpdfium` dynamisch. Binärdateien von `bblanchon/pdfium-binaries` (Apache-2.0/BSD) werden mitgeliefert. |
| PDF schreiben (Metadaten, Textebene) | `lopdf` | MIT | |
| Datum | `chrono` | MIT/Apache | |
| Muster | `regex` | MIT/Apache | |
| Unscharfer Vergleich | `strsim` | MIT | Firmennamen trotz OCR-Fehlern finden |
| Serialisierung | `serde`, `serde_json` | MIT/Apache | Einstellungen, Wissen, Journal (JSONL) |
| Ordner überwachen | (keine Bibliothek) | – | Abfrage alle 2 s statt Dateisystem-Ereignissen: auf NAS-Freigaben (SMB) zuverlässiger, siehe §10 |
| Dateidialog | `rfd` | MIT | |
| Pfade | `dirs` | MIT/Apache | |
| Hash (Duplikate) | `blake3` | CC0/Apache | |
| Finder-Tags | `xattr`, `plist` | MIT | nur macOS |
| macOS-OCR | `objc2-vision` (+ `objc2-foundation`) | MIT/Apache/Zlib | |
| Windows-OCR | `windows` (Feature `Media_Ocr`) | MIT/Apache | in Windows eingebaut, nichts mitzuliefern |
| Linux-OCR | Aufruf von `tesseract` als externes Programm | Apache-2.0 | vom Nutzer per Paketmanager installiert, keine Bindung zur Bauzeit |
| HTTP (KI) | `ureq` | MIT/Apache | nur mit Feature „ai“ |
| Systemsprache | `sys-locale` | MIT/Apache | Standard für die Oberflächensprache |
| Logs | `log`, `env_logger` | MIT/Apache | |

> Erwägen statt pdfium: `hayro`, ein reiner Rust-PDF-Renderer. Damit müsste keine dylib mitgeliefert werden.
> Vor M2 kurz prüfen, ob er gut genug ist. Sonst pdfium (bewährt, schnell, liefert Zeichenpositionen).

---

## 4. Ablauf einer Datei

```
Datei erscheint im Eingang
  │  (notify-Ereignis oder Ordner gewählt)
  ▼
Stabilitätsprüfung ── Größe 2 s unverändert und Datei lesbar? (Scanner schreibt noch) ──► später erneut
  ▼
Analyse-Worker (bis zu 3 Dateien im Voraus)
  1. pdfium: Text + Zeichenboxen je Seite (nur Seiten 1–2 für die Erkennung)
  2. zu wenig Text (< 30 Zeichen/Seite)? → OCR-Engine → Wörter + Boxen
  3. af-core::analyze(PageText, &Wissen) → Vorschlag { Felder, Sicherheit, Quellen, Kandidaten }
  4. Vorschaubild Seite 1 rendern (Größe passend zur Spalte, Rest beim Blättern)
  ▼
Oberfläche zeigt Vorschlag, Nutzer prüft/korrigiert
  ▼
Übernehmen (↵)
  1. Journal-Eintrag „geplant“
  2. Metadaten schreiben → Zwischendatei im selben Ordner → fsync
  3. Zwischendatei an den neuen Namen/Zielordner (rename; bei anderem Laufwerk kopieren + prüfen)
  4. Original entfernen
  5. Journal-Eintrag „erledigt“ + Lernen aus Korrekturen (Wissen aktualisieren)
```

Schlägt ein Schritt fehl, bleibt das Original unangetastet. Der Journal-Eintrag zeigt dann auf den Fehler.

---

## 5. Erkennung (af-core)

### 5.1 Eingabe

```rust
pub struct PageText { pub page: u16, pub width: f32, pub height: f32, pub words: Vec<Word> }
pub struct Word { pub text: String, pub bbox: Rect, pub font_size: f32, pub bold: bool }
pub enum TextSource { Embedded, Ocr { engine: OcrEngineKind, mean_confidence: f32 } }
```

Daraus erzeugt `af-core` Zeilen und Blöcke: Wörter mit gleicher Grundlinie bilden eine Zeile, Zeilen mit kleinem Abstand einen Block.
Jeder Block bekommt eine Zone:
- `Briefkopf`: oberes Viertel
- `Anschrift`: Fensterbereich DIN 5008
- `Infoblock`: oben rechts
- `Betreff`
- `Fließtext`
- `Fußzeile`: unteres Zehntel

### 5.2 Datum

- **Muster:**
  - `28.09.2026`, `28.9.26`, `28. September 2026`, `28. Sept. 2026`, `2026-09-28`, `28/09/2026`
  - deutsche und englische Monatsnamen
  - OCR-Verwechslungen tolerieren (`O`→`0`, `l`→`1`)
- **Jeder Fund bekommt eine Bedeutung** aus den Wörtern davor, im selben Satz oder Tabellenkopf:
  - `Rechnungsdatum`/`Datum`/Ortsangabe vor dem Datum („Stuttgart, 15. …“) → **Dokumentdatum**
  - `fällig`, `zahlbar bis`, `Abbuchung` → Fälligkeit
  - `vom … bis`, `Zeitraum`, `Leistungszeitraum` → Zeitraum
  - `Geburtsdatum`, `Vertragsbeginn` → sonstiges
- **Punktwertung für das Dokumentdatum:**
  - Pluspunkte: Bedeutung „Dokumentdatum“, Infoblock oder oben rechts, erste Seite, nahe am Scan-Datum aus Dateiname oder `mtime`
  - Abzüge: in der Zukunft, mehr als 2 Jahre vor dem Scan, im Fließtext
- **Ausgabe:** bester Kandidat plus die Liste aller Kandidaten (die Chips im Formular).

### 5.3 Rechnung ja/nein

Punktwertung:
- **Für eine Rechnung:** `Rechnung`, `Rechnungsnummer`, `Rechnungsbetrag`, `zahlbar bis`, Steuersätze (`19 % USt`, `MwSt`), Summenzeile mit `€`, IBAN des Absenders
- **Dagegen:** `Bescheid`, `Bescheinigung`, `Angebot`, `Mahnung` (einstellbar), `Gutschrift`, `Kontoauszug`
- **Grenzfälle** (Nebenkostenabrechnung, Steuerbescheid mit Zahlbetrag): mittlere Sicherheit, dann prüft der Nutzer.

Die Schlüsselwörter lassen sich in den Einstellungen bearbeiten.

### 5.4 Unternehmen

Reihenfolge, die erste Quelle mit Treffer gewinnt:

1. **Eindeutige Kennungen** aus dem Wissen (Sicherheit 97–99 %):
   - USt-IdNr. (`DE` + 9 Ziffern)
   - IBAN mit gültiger Prüfsumme
   - Steuernummer, Gläubiger-ID, Kunden-/Vertragsnummer
   - E-Mail-Domain, Web-Domain
2. **Bekannter Name** im Briefkopf/in der Fußzeile, unscharf verglichen (Jaro-Winkler ≥ 0,92 über Aliase), 85–95 %
3. **Heuristik** für neue Absender, 40–70 %:
   - größte oder fettgedruckte Schrift im Briefkopf
   - Absenderzeile über dem Anschriftenfeld
   - Rechtsformen (`GmbH`, `AG`, `e. V.`, `KG`, `Praxis`, `Finanzamt`)
4. Leer, falls nichts gefunden

**Schreibweise:** Das Wissen enthält den *Anzeigenamen* des Nutzers, z. B. „NetzWelt“ statt „NetzWelt Kommunikation GmbH“. Die Rechtsform wird standardmäßig entfernt (einstellbar).

### 5.5 Thema

1. **Betreffzeile:** fetter oder größerer Block zwischen Anschrift und Anrede. Füllwörter wie „Ihre“, „Ihr“, „für“ fallen weg. Kürzen auf ≤ 5 Wörter.
2. **Gelernte Themen dieses Absenders:** Ähnlichkeit zum Betreff, sonst das häufigste Thema (z. B. Stadtwerke → „Jahresabrechnung Strom“).
3. **Schlüsselwort-Wörterbuch:** Kontoauszug, Lohnabrechnung, Beitragsbescheinigung, Versicherungsschein, Kündigung …
4. **Optional KI** (siehe 5.8)

**Jahr im Thema:** Nennt der Betreff ein Bezugsjahr („für das Jahr 2025“), wird es angehängt, z. B. „Beitragsbescheinigung 2025“.

### 5.6 Sicherheit (Confidence)

- Jede Regel liefert einen Rohwert. Daraus wird eine Sicherheit von 0–100.
- Diese Werte werden an den eigenen Testdaten abgestimmt: 90 % soll heißen, dass in mindestens 90 % der Fälle nicht korrigiert werden muss.
- Bei OCR-Text wird die Sicherheit mit der OCR-Qualität multipliziert.
- **Sicherheit einer Datei** = Minimum ihrer Felder.
  - Liegt sie unter der Schwelle, erscheint die Datei im Filter „Prüfen“.
  - Liegt sie darüber, nimmt der Stapelmodus sie mit.

### 5.7 Lernen („Gelerntes Wissen“)

Datei `wissen.json` im Einstellungsordner, menschenlesbar:

```json
{
  "version": 1,
  "unternehmen": [{
    "name": "Stadtwerke Musterstadt",
    "aliase": ["Stadtwerke Musterstadt GmbH", "SWM"],
    "kennungen": { "ust_id": ["DE123456789"], "iban": ["DE12660501010000123456"], "domains": ["stadtwerke-musterstadt.de"] },
    "themen": { "Jahresabrechnung Strom": 9, "Abschlagsänderung": 3 },
    "rechnung_anteil": 0.85,
    "anzahl": 14
  }]
}
```

- **Datenschutz:** Lernen läuft ausschließlich lokal in der App (fester Rust-Code, kein Netzwerk, keine KI); `wissen.json` verlässt den Rechner nie. Entwickelt und getestet wird nur mit fiktiven Demo-Daten (`cargo xtask demo-archiv`).
- **Aus dem Archiv lernen** (Einstellungen → Gelerntes Wissen):
  1. Ordner rekursiv durchgehen und Dateinamen mit der aktuellen Vorlage zurückparsen (deshalb `_-_`)
  2. Text der ersten Seite lesen (keine OCR, nur vorhandene Textebene; OCR optional und langsam)
  3. Kennungen sammeln
  - Läuft im Hintergrund, lässt sich abbrechen, mit Fortschrittsanzeige.
- **Aus Korrekturen lernen:** Nach jedem Übernehmen werden Kennungen, Aliase und Themenzähler aktualisiert. Hat der Nutzer das Unternehmen geändert, wird die gefundene Kennung dem neuen Namen zugeordnet.
- **Bearbeitbar:** Unternehmen zusammenführen oder umbenennen, Kennungen löschen.

### 5.8 KI (optional, Feature `ai`, Standard aus)

- **Anbindung:** Ollama über `http://localhost:11434`. Modell wählbar, Vorschlag: ein kleines Modell mit 3–4 Mrd. Parametern.
- **Wann:** nur wenn ein Feld unter 80 % liegt oder für das Thema, wenn kein Betreff gefunden wurde.
- **Eingabe:** gekürzter Text der ersten Seite (≤ 2 000 Zeichen) plus Liste bekannter Unternehmen und Themen.
- **Antwort:** ausschließlich JSON (`{"datum","unternehmen","thema","rechnung"}`).
- **Kein Freifahrtschein:** KI-Ergebnisse bekommen höchstens 85 % Sicherheit. Der Stapelmodus übernimmt sie also nicht automatisch.
- **Cloud-API:** erst später und nur auf ausdrücklichen Wunsch, mit deutlichem Hinweis, dass Dokumenttext übertragen wird.

---

## 6. Dateiname: Vorlage und Bereinigung

### Vorlagensprache

| Platzhalter | Wert |
|---|---|
| `{datum}` | `JJJJ-MM-TT` |
| `{jahr}` `{monat}` `{tag}` | Teile des Datums |
| `{unternehmen}` `{thema}` | bereinigte Feldwerte |
| `{rechnung}` | `Rechnung` oder leer |
| `{feld?:text}` | `text`, wenn `feld` gesetzt ist, sonst nichts, z. B. `{rechnung?:_Rechnung}` |

Dieselbe Vorlagensprache gilt für die Unterordner-Vorlage (`{jahr}/{unternehmen}`) und den Metadaten-Titel.

**Parser:** Ein kleiner eigener Parser (kein Template-Crate), dafür mit verständlichen Fehlermeldungen in den Einstellungen,
z. B. „Unbekannter Platzhalter {firma} – meintest du {unternehmen}?“.

### Bereinigung je Feldwert (Reihenfolge)

1. Unicode NFC, Steuerzeichen entfernen, trimmen
2. Umlaute: `ä→ae ö→oe ü→ue Ä→Ae Ö→Oe Ü→Ue ß→ss`
3. Akzente entfernen (NFD und Kombinationszeichen weg): `é→e`, `ç→c`
4. `&` → ` und `, `+` → ` plus `
5. Bindestrich mit Leer- oder Unterstrichen drumherum → `-` (damit `_-_` als Trenner reserviert bleibt)
6. Alles außer `A–Z a–z 0–9 - _ Leerzeichen` → Leerzeichen
7. Leerzeichen zusammenfassen und durch `_` ersetzen, mehrfache `_`/`-` zusammenfassen

### Danach am ganzen Namen

- Verwaiste Trenner entfernen (leeres Thema → kein `_-_` am Ende).
- „Rechnung“ am Themenende entfernen, wenn `{rechnung}` gesetzt ist.
- Für Windows reservierte Namen vermeiden (`CON`, `PRN`, `AUX`, `NUL`, `COM1–9`, `LPT1–9`).
- Länge: höchstens 200 Byte für den Namen und 240 Zeichen für den ganzen Pfad (Windows MAX_PATH). Gekürzt wird zuerst das Thema, dann das Unternehmen.
- Gibt es den Namen schon: `_2`, `_3` … vor `.pdf`.

### Zurückparsen (fürs Lernen aus dem Archiv)

Die Vorlage wird in einen regulären Ausdruck übersetzt:
- `{datum}` → `\d{4}-\d{2}-\d{2}`
- `_-_` → fester Trenner
- `_Rechnung$` → optionaler Zusatz

### Tests (Beispiele, als Tabelle in `core/tests/naming.rs`)

| Eingabe (Unternehmen / Thema / Rechnung) | Ergebnis |
|---|---|
| Stadtwerke Musterstadt / Jahresabrechnung Strom / ja | `2026-09-28_Stadtwerke_Musterstadt_-_Jahresabrechnung_Strom_Rechnung.pdf` |
| Krankenkasse Süd / Beitragsbescheinigung 2025 / nein | `2026-09-15_Krankenkasse_Sued_-_Beitragsbescheinigung_2025.pdf` |
| Zahnarztpraxis Dr. Weber / Professionelle Zahnreinigung / ja | `2026-09-30_Zahnarztpraxis_Dr_Weber_-_Professionelle_Zahnreinigung_Rechnung.pdf` |
| Müller - Partner / Strom Rechnung / ja | `…_Mueller-Partner_-_Strom_Rechnung.pdf` |
| Café & Co. / (leer) / nein | `…_Cafe_und_Co.pdf` |

---

## 7. Oberfläche (app, egui)

Vorlage ist `mockup.html`. Was dort gezeigt wird, gilt als Anforderung.

### 7.1 Bereiche

| Bereich | Inhalt | Modul |
|---|---|---|
| Titelleiste/Toolbar | ☰ Liste ein/aus, Eingangsordner (zuletzt benutzte Ordner), Fortschritt, Stapelmodus, Markierungen ein/aus, Einstellungen | `toolbar.rs` |
| Dateiliste | Filter Alle/Offen/Prüfen/Erledigt, Status-Symbol, Originalname, Vorschlag bzw. neuer Name, Hinweis „Ordner wird überwacht“ | `file_list.rs` |
| Vorschau | gerenderte Seite als Textur, Zoom, Seitenwechsel, farbige Markierungen der Fundstellen, Datumskandidaten anklickbar, Text markieren → als Unternehmen/Thema übernehmen | `pdf_view.rs` |
| Formular | Herkunft des Textes (Textebene/OCR), Felder mit Sicherheit und Herkunft, Datumschips, Themenvorschläge, Rechnungsschalter, Live-Dateiname + Zielordner + Warnungen, Metadaten (aufklappbar), Aktionen | `form.rs` |
| Statusleiste | Tastaturkürzel | `statusbar.rs` |
| Einstellungen | Seiten: Allgemein (Sprache, Aussehen), Dateiname, Ordner, Erkennung, Metadaten, Gelerntes Wissen, Über | `settings_ui.rs` |
| Hinweis-Toast | „Umbenannt → …“ mit Rückgängig-Knopf | `toast.rs` |

### 7.2 Eigene Widgets (gibt es in egui nicht fertig)

- **`autocomplete.rs`:** Textfeld mit Vorschlagsliste aus dem Wissen. Bedienung mit `↑↓` und `Tab`. `↵` übernimmt den Vorschlag, wenn die Liste offen ist, sonst die ganze Datei.
- **`pdf_view.rs`:**
  - Bild der Seite plus Rechtecke an den Wortpositionen aus `PageText` (in Bildkoordinaten umgerechnet)
  - Textauswahl durch Ziehen: Wörter, deren Box das Ziehrechteck schneidet, in Lesereihenfolge
  - Kontextmenü „→ Unternehmen / → Thema“
- **`chips.rs`:** auswählbare Chips (Datumskandidaten, Themen, Finder-Tags)
- **`confidence.rs`:** Sicherheits-Plakette (grün ≥ Schwelle, gelb ≥ 70 %, rot darunter)

### 7.3 Tastatur

| Taste | Aktion |
|---|---|
| `↵` | Übernehmen & zur nächsten offenen Datei |
| `⌘↓` / `⌘↑` | Nächste Datei (überspringen) / vorherige |
| `⌥1` `⌥2` `⌥3` | Fokus auf Datum / Unternehmen / Thema (Inhalt markiert) |
| `⌥R` | Rechnung umschalten |
| `⌘Z` | Letzte Aktion rückgängig (auch Stapel als Ganzes). In Textfeldern macht `⌘Z` die Texteingabe rückgängig. |
| `⌘L` | Dateiliste ein/aus |
| `⌘,` | Einstellungen |
| `⌘O` | Ordner wählen |
| `⌘ +/−/0` | Vorschau zoomen |
| `esc` | Eingabefeld verlassen / Vorschlagsliste schließen |

Unter Windows und Linux steht `Strg` statt `⌘` und `Alt` statt `⌥`.

### 7.4 Gestaltung

- **Farben** nur über `theme.rs`, also Hell, Dunkel und „System“. Die Feldfarben gleichen dem Mockup:
  - Datum blau `#2f6fed`
  - Unternehmen grün `#1f9d55`
  - Thema bernstein `#c98500`
  - Rechnung rot `#d64545`
- **Schrift:** egui-Standardschrift plus ein eingebetteter Ausschnitt aus DejaVu Sans (Pfeile, Tastensymbole ⌘ ⌥ ↵, ✔ ⚠ ◀ ▶; `assets/fonts/`, Lizenz Bitstream Vera). Neue Symbole nach dem Bauen im Screenshot prüfen.
- **Fenster:** Mindestgröße 1100×650 pt. Unter 1250 pt Breite wird die Dateiliste automatisch eingeklappt.

### 7.6 Sprachen (Deutsch / Englisch)

- **Einstellung:** Einstellungen → Allgemein → Sprache: `System` · `Deutsch` · `English`.
  - Wirkt sofort: Die Sprache steckt in einer globalen `AtomicU8`, das nächste Bild zeichnet alles neu.
  - Gespeichert als `sprache` in `einstellungen.json`.
  - Systemsprache über `sys-locale`: `de*` → Deutsch, sonst Englisch.
- **Komplett zweisprachig** heißt:
  - alle Oberflächentexte: Menüs, Tooltips, Fehlermeldungen, Toasts, Dialoge, Statusleiste
  - Datei- und Ordnerdialoge (`rfd`-Titel)
  - Sicherheits- und Herkunftshinweise („Gelernt aus USt-IdNr. …“)
  - macOS-Menüleiste
  - Ausgaben der CLI (`--lang` bzw. `LANG`)
- **Umsetzung:** `tr("…")` / `trf("… {} …", &[&wert])` aus `i18n.rs`. Der deutsche Text ist der Schlüssel, die englische Übersetzung steht in der Tabelle `en()` (wie FadeWerk DJ). Mehrzahl über `trn(n, "1 Datei", "{} Dateien")`.
- **Absichern:**
  - Fehlende Übersetzungen zeigen Deutsch und werden im Debug-Log gemeldet.
  - Ein Test in `app/tests/i18n.rs` sucht per Regex alle `tr("…")`-Aufrufe im Quellcode und schlägt fehl, wenn eine Übersetzung fehlt.
- **Was nicht übersetzt wird:** Fachbegriffe im *Dateinamen* hängen nicht an der Oberflächensprache. Der Rechnungs-Zusatz ist Teil der Vorlage (`{rechnung?:_Rechnung}`, englisch z. B. `{rechnung?:_Invoice}`). Beim ersten Start wird die Standardvorlage passend zur Sprache vorbelegt.
- **Erkennung:** Datumsmuster, Monatsnamen und Rechnungs-Schlüsselwörter decken immer Deutsch *und* Englisch ab, unabhängig von der Oberflächensprache.

### 7.7 Seite „Über“

Pflichtangaben und Transparenz, für Open Source üblich:

| Abschnitt | Inhalt |
|---|---|
| Kopf | Icon, „Ablagefuchs“, Version (`CARGO_PKG_VERSION`) und Commit-Kurzhash (per `build.rs` aus `git rev-parse`), Build-Datum |
| Autor | © 2026 Markus Eckert (aus `CARGO_PKG_AUTHORS`) |
| Lizenz | „Lizenziert unter MIT oder Apache-2.0, nach Wahl.“ Beide Lizenztexte eingebettet (`include_str!("../../LICENSE-MIT")`, `LICENSE-APACHE`), aufklappbar und scrollbar |
| Haftung | Hinweis „ohne jegliche Gewährleistung“ (aus den Lizenzen) |
| Quellcode | Link auf das Repo (`CARGO_PKG_REPOSITORY`), Link „Fehler melden“ (GitHub Issues) |
| Fremdbestandteile | pdfium (Apache-2.0/BSD-3), eingebettete Schrift (OFL-1.1), Rust-Bibliotheken: Liste aus `THIRD-PARTY-LICENSES` (zur Bauzeit von `cargo bundle-licenses` erzeugt und eingebettet), durchsuchbar |
| Datenschutz | „Ablagefuchs arbeitet vollständig offline. Es werden keine Daten übertragen.“ Ist KI über Ollama an: „Text wird nur an den lokalen Ollama-Dienst gesendet.“ |
| Diagnose | Knöpfe „Logdatei anzeigen“ und „Einstellungsordner öffnen“ |

Alle Texte der Seite sind ebenfalls zweisprachig. Die Lizenztexte selbst bleiben im englischen Original.

### 7.8 Grafik-Backend und Absturzschutz (aus FadeWerk DJ übernommen)

**Hintergrund:** FadeWerk DJ stürzte unter Windows sporadisch beim Start ab (Log vom 2026-10-11, Intel UHD 620).
Die Ursache war ein Absturz im **Vulkan-Treiber** direkt nach der Grafikkarten-Suche, ohne Rust-Panic. Ablagefuchs
nutzt denselben Unterbau (eframe → wgpu) und wäre genauso betroffen. Darum gilt von Anfang an:

| Maßnahme | Umsetzung |
|---|---|
| Windows: nur DirectX 12 | `wgpu_options()` in `main.rs` setzt die Backends unter Windows auf `DX12`, kein Vulkan. Die Umgebungsvariable `WGPU_BACKEND` überschreibt das zum Testen. |
| macOS/Linux | macOS: Metal. Linux: Vulkan, sonst GL (wgpu-Standard). |
| Ausweichweg OpenGL | eframe mit Feature `glow`. Scheitert wgpu, bevor die App erzeugt ist, startet `main` neu mit OpenGL und setzt `opengl = true` in den Einstellungen. |
| Schutz vor Abstürzen ohne Panic | Datei `grafikstart` im Einstellungsordner: wird vor dem Start mit „wgpu“ angelegt und nach dem 3. gezeichneten Bild gelöscht. Liegt sie beim nächsten Start noch da, kam der letzte Start nicht bis zum Bild → automatisch OpenGL. |
| Manuell umschaltbar | Einstellungen → Allgemein → „Grafik: Automatisch / DirectX 12 / OpenGL“, Entwicklerschalter `AF_RENDERER=glow\|wgpu` |
| Fehlerfenster | Unter Windows zeigt ein Absturz der Oberfläche ein Fehlerfenster (`rfd`) statt stillem Beenden. Panics landen mit Aufrufliste in `ablagefuchs.log`. |
| Nachstellen auf dem Mac | `WGPU_BACKEND=dx12` erzwingt den Fehlstart → prüft den Ausweichweg |

Den Code dafür (`main.rs`, `diagnose.rs`) können wir weitgehend aus FadeWerk DJ übernehmen.

### 7.5 Zustand und Speicherorte

| Datei | Ort | Inhalt |
|---|---|---|
| `einstellungen.json` | Einstellungsordner `Ablagefuchs` (`dirs::config_dir()`) | Vorlage, Bereinigung, Ordner, Schwelle, OCR/KI, Metadaten, Fenster, zuletzt benutzte Ordner |
| `wissen.json` | Einstellungsordner | gelernte Unternehmen/Themen/Kennungen |
| `journal.jsonl` | `<Eingang>/.ablagefuchs/` (Ausweichort: Einstellungsordner, wenn das NAS schreibgeschützt ist) | jede Umbenennung: Zeit, alt, neu, Hash, Metadaten vorher/nachher |
| `cache/` | `dirs::cache_dir()/Ablagefuchs` | Analyseergebnisse je Datei-Hash (spart OCR beim erneuten Öffnen) |
| `ablagefuchs.log` | Einstellungsordner | Log mit Rotation ab 1 MB, Panics mit Aufrufliste |

Geschrieben wird immer atomar: Zwischendatei, dann `rename` (`write_atomic`, wie in FadeWerk DJ).
Fehlende Felder in den JSON-Dateien fallen auf Standardwerte zurück.

---

## 8. PDF-Bearbeitung (af-pdf)

| Aufgabe | Umsetzung |
|---|---|
| Seite rendern | pdfium → RGBA → `egui::ColorImage`. Seite 1 sofort, weitere bei Bedarf. Bei Dateien auf dem NAS: einmal lokal in den Cache kopieren. |
| Text + Positionen | pdfium `text().chars()` → Wörter mit Boxen, Schriftgröße, fett |
| Metadaten | lopdf: Info-Dictionary `Title`, `Author`, `Subject`, `Keywords`, `CreationDate` (= Dokumentdatum), `Producer = Ablagefuchs x.y` |
| XMP | XMP-Paket mit `dc:title`, `dc:creator`, `dc:subject` (Stichwörter), `xmp:CreateDate`. Für Spotlight und Windows-Suche. |
| Finder-Tags (macOS) | erweitertes Attribut `com.apple.metadata:_kMDItemUserTags` als binäre plist |
| OCR-Textebene | unsichtbarer Text (Darstellungsmodus 3) an den OCR-Wortpositionen. Schrift Helvetica mit WinAnsiEncoding (enthält Umlaute). Danach ist die Datei im Volltext durchsuchbar. |
| Original sichern | optional: unveränderte Kopie in `.ablagefuchs/originale/` |

**pdfium mitliefern:**
- **macOS:** `libpdfium.dylib` in `Ablagefuchs.app/Contents/Frameworks/`. Die arm64- und x64-Fassung werden mit `lipo` zu einer Universal-Datei zusammengefügt.
- **Windows:** `pdfium.dll` neben der `.exe`
- **Linux:** `libpdfium.so` neben dem Programm
- **Laden:** `Pdfium::bind_to_library` sucht zuerst neben dem Programm bzw. in `Frameworks/`, dann im System.
- **Holen:** `cargo xtask fetch-pdfium` lädt die passende Version für Entwicklung und CI.

---

## 9. OCR (af-ocr)

```rust
pub trait OcrEngine: Send + Sync {
    fn name(&self) -> &'static str;
    fn available(&self) -> bool;
    fn recognize(&self, image: &RgbaImage, langs: &[&str]) -> anyhow::Result<Vec<Word>>;
}
```

| System | Engine | Hinweis |
|---|---|---|
| macOS | **Apple Vision** (`VNRecognizeTextRequest`, Stufe *accurate*, Sprachen `de-DE`, `en-US`) | sehr gut, schnell, nichts zu installieren |
| Windows | **Windows.Media.Ocr** | eingebaut, braucht das deutsche Sprachpaket (sonst Hinweis mit Link zu den Einstellungen) |
| Linux | **Tesseract** (`tesseract … tsv`) | Hinweis zum Installieren, falls nicht vorhanden (`apt install tesseract-ocr tesseract-ocr-deu`) |

- **Auflösung:** OCR läuft auf 300-dpi-Renderings der ersten beiden Seiten, wenn Seite 1 weniger als 30 Zeichen Text hat.
- **Cache:** Ergebnisse der Texterkennung liegen im Cache-Ordner des Systems unter `Ablagefuchs/ocr/v1/<BLAKE3>.json` (Schlüssel = Dateiinhalt, übersteht Umbenennen).
- **Fettdruck:** OCR kennt keinen Fettdruck; die Betreffzeile wird deshalb auch über größere Schrift als der Fließtext erkannt.
- **Apple Vision:** Beim allerersten Lauf einer neuen Programmversion bereitet macOS das Modell vor (~25 s, einmalig), danach ~1 s pro Seite.
- **Schiefe Scans:** einfache Schräglagenkorrektur nur, wenn die Engine sie nicht selbst macht (Vision und Windows tun es).

---

## 10. Dateioperationen und Sicherheit

- **Stabilität:** neue Dateien erst, wenn Größe und `mtime` zwischen zwei Abfragen (2 s) gleich bleiben. Ändert sich eine angezeigte, noch offene Datei danach doch (Scanner schreibt mit Pause weiter, Datei ersetzt), wird sie automatisch neu gelesen. Von außen gelöschte Dateien erscheinen als „nicht mehr vorhanden“.
- **Kein Überschreiben:** Kollisionen bekommen `_2`, `_3` …. Vor dem Umbenennen wird erneut geprüft (falls jemand parallel arbeitet).
- **Verschieben auf ein anderes Laufwerk:** kopieren, BLAKE3-Hash vergleichen, erst dann löschen.
- **Rückgängig:**
  - Journal rückwärts abspielen: neuer Name → alter Name
  - Metadaten nur zurücksetzen, wenn eine Originalkopie existiert. Sonst Hinweis „Name zurückgesetzt, Metadaten bleiben“.
- **Duplikate:** gleicher Hash wie eine bereits erledigte Datei → Hinweis „Doppelt gescannt?“ mit „Löschen“ (in den Papierkorb, Crate `trash`).
- **Rechte/NAS:** Schreibt das NAS nicht, gibt es eine klare Meldung und keinen halben Zustand.

---

## 11. CLI (`ablagefuchs-cli`)

Für Tests, Automatisierung und Fehlersuche. Nutzt dieselbe Logik wie die App.

```sh
ablagefuchs-cli suggest scan.pdf             # Vorschlag als Text, mit Sicherheiten und Quellen
ablagefuchs-cli suggest --json Eingang/      # alle Dateien als JSON
ablagefuchs-cli rename --min-confidence 90 Eingang/   # Stapelmodus ohne GUI (Probelauf mit --dry-run)
ablagefuchs-cli learn /Volumes/NAS/Archiv    # Wissen aus Archiv aufbauen
ablagefuchs-cli undo [--last N]              # aus dem Journal zurücknehmen
ablagefuchs-cli text scan.pdf                # erkannter Text mit Zonen (Fehlersuche)
```

### 11.1 Anbindung an lokale KI-Assistenten (Idee vom 2026-10-13)

Ziel: Ein lokaler Assistent soll den Eingangsordner mit Ablagefuchs abarbeiten können, z. B. so:
„Benenne alle sicheren Scans um und zeig mir die unsicheren.“

| Stufe | Wie | Stand |
|---|---|---|
| 1. CLI mit JSON | Ein Assistent ruft `ablagefuchs-cli suggest --json <ordner>` auf und bekommt je Datei Vorschlag, Sicherheit, Herkunft und Datumskandidaten. Später `rename --dry-run`, `rename`, `undo`. | `suggest --json` gibt es seit M1 |
| 2. MCP-Server | `ablagefuchs-cli mcp` spricht das Model Context Protocol über stdin/stdout. Einrichten: im jeweiligen Assistenten als MCP-Server mit dem Befehl `ablagefuchs-cli mcp` eintragen. Werkzeuge: `list_inbox`, `suggest`, `read_text`, `rename` (Felder übergeben, Vorlage wird angewendet), `undo`, `learn`. | geplant (M8, auf Wunsch früher) |

**Leitplanken:**
- Die gleichen Sicherheitsregeln wie in der App:
  - `rename` schreibt ins Journal und lässt sich rückgängig machen.
  - Kein Überschreiben, Kollisionen bekommen `_2`.
  - Nur Dateien im angegebenen Eingangsordner.
- Kein Netzwerk: Der MCP-Server läuft lokal als Kindprozess des Assistenten.
- Dokumentinhalte gehen nur an den Assistenten, wenn der Nutzer ihn dafür einsetzt. Das wird in der README erklärt.
- Die Antworten nutzen dieselben Strukturen wie `--json` (`Suggestion`, `Source` …). App, CLI und MCP bleiben so deckungsgleich.

---

## 12. Testen und Qualität

| Ebene | Was | Wo |
|---|---|---|
| Einheitstests | Vorlage, Bereinigung, Zurückparsen, Datumsmuster, Kennungen (IBAN-Prüfsumme, USt-IdNr.), Punktwertung | `crates/core` |
| Golden-Tests | `tests/fixtures/*.txt` (anonymisierter Text mit Positionen als JSON) → erwartetes `*.expected.json`. Eine Änderung an Regeln zeigt sofort, welche Dokumente kippen. | `crates/core/tests` |
| Messung der Treffsicherheit | `cargo xtask accuracy`: Trefferquote je Feld und Kalibrierung der Sicherheit über alle Fixtures. Wird bei Regeländerungen beobachtet. | `xtask` |
| PDF | Demo-PDFs (von `cargo xtask demo-pdfs` aus fiktiven Firmen erzeugt, wie im Mockup) → Text, Metadaten schreiben und wieder lesen | `crates/pdf/tests` |
| Dateioperationen | Temp-Verzeichnisse: Kollision, Rückgängig, Fehler mitten im Ablauf (simuliert) | `crates/core/tests` |
| Oberfläche | `AF_DEMO=1 AF_SCREENSHOT=/tmp/af.bmp` → Bild ansehen (wie bei FadeWerk DJ) | `app` |

**Echte Dokumente** kommen nie ins Repo. Für Fixtures gibt es `cargo xtask anonymize scan.pdf`:
Es ersetzt Namen, Adressen, IBANs und Beträge durch Platzhalter und behält Layout und Positionen.

**Entwicklerschalter** (nur Debug-Builds, speichern nichts):

| Variable | Wirkung |
|---|---|
| `AF_DEMO=1` | Demo-Ordner mit erzeugten PDFs laden |
| `AF_SCREENSHOT=/pfad.bmp` | nach ~3 s Bild speichern und beenden |
| `AF_SIZE=1600x1000` | Fenstergröße |
| `AF_LANG=de\|en`, `AF_THEME=hell\|dunkel` | Sprache/Aussehen |
| `AF_SETTINGS=allgemein\|name\|ordner\|erkennung\|metadaten\|wissen\|ueber` | Einstellungen öffnen |
| `AF_FILE=3` | Datei Nr. 3 auswählen |
| `AF_RENDERER=glow\|wgpu` | Grafik-Backend erzwingen |
| `AF_KEYS=enter,cmd+z,alt+r,cmd+down` | drückt nacheinander diese Tasten (Tastatur-Ablauf ohne Maus testen, auch mit `text:…`) |

---

## 13. Bauen, Paketieren, Veröffentlichen

Übernommen und angepasst von **FadeWerk DJ** (`~/PhpstormProjects/FadeWerk DJ/scripts`, `.github/workflows`).

### Skripte

| Skript | Aufgabe |
|---|---|
| `scripts/bundle-macos.sh` | baut `dist/Ablagefuchs.app`. Info.plist mit `de.eckonator.ablagefuchs`, `LSApplicationCategoryType = public.app-category.productivity`, `LSMinimumSystemVersion 11.0`. Kopiert `libpdfium.dylib` nach `Contents/Frameworks`, signiert ad hoc (`codesign --force --deep --sign -`). `AF_SKIP_BUILD=1` nutzt ein vorhandenes Universal-Binary. |
| `scripts/make-dmg.sh` | DMG mit Ziehfenster über `dmgbuild` (Layout `scripts/dmg-settings.py`). Der Hintergrund `assets/dmg-background.svg` → `.tiff` (`make-dmg-background.sh`) zeigt einen **Hilfe-Kasten mit dem `xattr`-Befehl**, weil „Dennoch öffnen“ ab Sequoia oft nicht hilft. |
| `scripts/install-macos.sh` | `curl -fsSL …/install-macos.sh \| bash`: lädt die neueste `-macOS.zip` über die GitHub-API, installiert nach `/Applications` (bei Bedarf mit `sudo`), entfernt die Quarantäne, startet die App. Bricht ab, wenn die App läuft. Testschalter `AF_INSTALL_DIR`, `AF_NO_OPEN`. |
| `scripts/make-icon.sh` | `assets/icon.svg` → PNGs + `Ablagefuchs.icns` (Chrome headless, `sips`, `iconutil`) |

### Hinweis für DMG und README (macOS)

```sh
xattr -dr com.apple.quarantine "/Applications/Ablagefuchs.app"
```

Die echte Lösung wäre eine Notarisierung mit Apple Developer ID (kostenpflichtig). Das bleibt bewusst für später.

### GitHub Actions

- **`ci.yml`** läuft bei jedem Push und jedem Pull Request auf `macos-14`, `windows-latest` und `ubuntu-22.04`:
  - `cargo test --workspace`
  - `cargo clippy -- -D warnings`
  - `cargo deny check licenses advisories`
  - pdfium vorher mit `cargo xtask fetch-pdfium` holen
- **`release.yml`** bei jedem Push auf `main`: Release `v<Version aus Cargo.toml>` wird ersetzt. Eine neue Version entsteht durch Erhöhen von `version`.
  - **macOS:** Universal-Binary (`lipo` aus aarch64 und x86_64, `MACOSX_DEPLOYMENT_TARGET=11.0`), `.app` als `-macOS.zip` (für das Install-Skript) und `-macOS.dmg`
  - **Windows:** `Ablagefuchs-x.y.z-Windows.zip` mit `.exe`, `pdfium.dll`, Lizenzen. Das Icon wird mit `winresource` in die `.exe` eingebettet.
  - **Linux:** `Ablagefuchs-x.y.z-Linux-x86_64.tar.gz` mit Programm, `libpdfium.so`, `.desktop`, Icon. AppImage folgt später.
  - **Alle drei:** `THIRD-PARTY-LICENSES.yml` über `cargo bundle-licenses`, plus der pdfium-Lizenztext

### Repo-Konventionen (wie FadeWerk DJ)

- Committen nur auf Nachfrage. Pushen macht Markus selbst.
- README auf Englisch mit Screenshots aus der englischen Oberfläche (`AF_LANG=en`) und Demo-Dokumenten fiktiver Firmen.
- Keine fremden Grafiken oder Icons. Das Icon ist eine eigene Gestaltung (Fuchs + Dokument, macOS-Squircle).

---

## 14. Meilensteine

Jeder Meilenstein endet mit grünen Tests, sauberem Clippy und – ab M2 – einem Screenshot zur Abnahme durch Markus.

| # | Meilenstein | Inhalt | Fertig, wenn … |
|---|---|---|---|
| **M0** | Grundgerüst | Workspace, Lizenzen, `deny.toml`, CI, README-Gerüst, `xtask fetch-pdfium`, `xtask demo-pdfs` | CI ist auf allen drei Systemen grün |
| **M1** | Kern + CLI | Vorlage, Bereinigung, Zurückparsen, Datum, Rechnung, einfache Unternehmens-/Themenheuristik, pdfium-Text, `cli suggest` | `ablagefuchs-cli suggest` liefert für alle Demo-PDFs (mit Textebene) den richtigen Namen |
| **M2** | Erste Oberfläche | Fenster mit Grafik-Absturzschutz (§7.8), Ordner wählen, Dateiliste mit Filtern, Vorschau (Seite 1, Zoom, Blättern), Formular, Live-Dateiname, Übernehmen, Journal + Rückgängig, Einstellungen „Allgemein“ (Sprache DE/EN, Aussehen), „Dateiname“, „Ordner“, „Über“; i18n von Anfang an (`tr()` ab der ersten Zeile, Prüftest) | Demo-Ordner lässt sich komplett mit der Tastatur abarbeiten, in beiden Sprachen |
| **M3** | Interaktion | Markierungen, Datumskandidaten anklicken, Text markieren → Feld, Autovervollständigung, alle Tastenkürzel, Toast, Hell/Dunkel | Abnahme gegen `mockup.html` |
| **M4** | OCR + Hintergrund | af-ocr (Vision, Windows, Tesseract), Analyse-Worker mit Vorausladen, Cache, Ordnerüberwachung mit Stabilitätsprüfung | gescannte Demo-PDFs ohne Textebene werden erkannt, kein Ruckeln beim Weiterschalten |
| **M5** | Wissen | `wissen.json`, Lernen aus Archiv + Korrekturen, Kennungen, Sicherheitswerte, Filter „Prüfen“, Stapelmodus, Seite „Gelerntes Wissen“ | Mit eingelesenem Archiv ≥ 90 % der Testdateien ohne Korrektur |
| **M6** | Metadaten | Info + XMP, Finder-Tags, OCR-Textebene, Originale sichern, Duplikaterkennung | Spotlight findet eine Datei über Stichwort und Volltext |
| **M7** | Release 0.1.0 | Icon, Übersetzungen vollständig prüfen, Bundle/DMG/Install-Skript, Release-Workflow, README mit Screenshots, Markenrecherche erledigt | Download aus GitHub läuft auf einem frischen Mac, Windows-PC und Linux |
| **M8** | Ausbau | MCP-Server für KI-Assistenten (§11.1), Ollama-Anbindung, Verschieben ins Archiv als eigener Assistent, mehrere Dokumente in einem Scan trennen, leere Seiten entfernen, AppImage, Notarisierung | je nach Wunsch |

---

## 15. Risiken und offene Fragen

| Risiko | Gegenmaßnahme |
|---|---|
| pdfium als dylib mitliefern (Signatur, Pfade, Universal) | früh in M0/M2 im echten `.app` testen. Alternative `hayro` beobachten. |
| OCR-Qualität unter Linux (Tesseract schwächer) | Sicherheit an die OCR-Qualität koppeln, schwache Dateien landen in „Prüfen“ |
| Windows ohne deutsches OCR-Sprachpaket | erkennen und mit Anleitung darauf hinweisen |
| Grafiktreiber-Absturz unter Windows (Vulkan) | nur DX12 + OpenGL-Ausweichweg + Startmarkierung (§7.8), früh auf einem echten Windows-Laptop testen |
| NAS über SMB langsam | Analyse liest nur die ersten Seiten, Cache lokal, Vorausladen |
| Neuere Crate-Versionen als das Trainingswissen (eframe, pdfium-render, objc2) | API im Quellcode unter `~/.cargo/registry/src` nachsehen (FadeWerk-Erfahrung) |
| Erkennung neuer Absender ohne Wissen | Mittel- bis Langfrist-Lernen. Anfangs bewusst niedrige Sicherheit, damit nichts falsch automatisch durchläuft. |
| Rechtliches (Name) | DPMA/EUIPO-Recherche vor Veröffentlichung (§2) |

**Offen für später:**
- Soll „Mahnung“ als Rechnung zählen?
- Welche Finder-Tags automatisch (Jahr? Unternehmen?)?
- Mehrseitige Scans mit mehreren Dokumenten trennen: lohnt sich das für deinen Scanner-Workflow?
