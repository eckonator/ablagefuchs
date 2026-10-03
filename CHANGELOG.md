# Changelog

## 0.2.0

### Learning from your archive
- **Learns from archives with any naming scheme.** File names without the template's `_-_` separator
  (e.g. `2025-03-14_Stadtwerke_Musterstadt_Abschlag.pdf`) are now read freely. A company counts only
  when the letter confirms it: in the letterhead, sender line or footer, through a known identifier
  (VAT ID, IBAN …), or, for companies already confirmed by other letters, anywhere in the text
  (also with small OCR misreadings).
- **Re-learning is incremental.** Ablagefuchs remembers which files it has learned (`lernstand.json`
  next to `wissen.json`) and only reads new, changed or still unconfirmed files. Nothing is counted
  twice, and OCR results are cached, so a second run is fast.
- **Detailed learning statistics:** why files were not confirmed (no text, company only in the body
  text, similar spelling, not found at all).
- **Go through unlearned files:** opens the files that could not be confirmed like an inbox. You confirm
  the company, Ablagefuchs learns from it. Archive files keep their names.

### Managing knowledge
- Search, rename and merge companies under *Settings → Knowledge*.
- **Possible duplicates** are suggested ("Stadtwerke" / "Stadtwerke Musterstadt", similar spellings,
  shared identifiers) and can be merged or dismissed with one click.

### Settings
- **Export and import** all settings and the learned knowledge (*Settings → General*), e.g. to use
  Ablagefuchs on a second computer without learning again. On import, knowledge is merged;
  device-specific settings stay.

### Fixes
- The company table no longer widens the settings window.
- "Last run" statistics are now translated into English.

## 0.1.0

First release: recognition of date, company, topic and invoices; preview with highlighted findings;
file name template; batch mode with undo; local learning from corrections and archives; PDF metadata,
OCR text layer and Finder tags; duplicate detection; OCR with Apple Vision, Windows OCR or Tesseract.
