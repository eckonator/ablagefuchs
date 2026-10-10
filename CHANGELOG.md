# Changelog

## 0.3.2

- **Update from inside the app.** When a new version is out, **Update now** in the bar downloads and installs
  it and restarts Ablagefuchs (not while learning). On macOS the new app is only accepted if it is signed with
  the same certificate as the installed one; from this version on, Mac builds are signed with the project's own
  certificate. On Windows the files of the zip are replaced in their folder; on Linux this works when installed
  with `install.sh`. Otherwise the bar links to the download page as before.
- **Check for updates now** in the settings.

## 0.3.1

- **Linux:** the archive now contains `install.sh`, which adds Ablagefuchs with its icon to the application
  menu (per user, in `~/.local`; `./install.sh --uninstall` removes it). The window now also shows its icon
  in the dock on Wayland.

## 0.3.0

- **Recognizes senders by their letterhead image.** Many letters print the company name only as a logo,
  which OCR cannot read. Ablagefuchs now stores a small fingerprint of each company's letterhead (a coarse
  raster, not an image of the letter), corrected for skewed and shifted scans. New scans whose letterhead
  matches are suggested with that company (to be confirmed), and archive files whose name mentions the
  company are learned through it. Fingerprints are added automatically for files learned before.
- **Groups of identical letterheads:** companies that never print their name as text are learned when
  several letters with the same letterhead carry the same company in their file names. Words from the
  letters' subjects and known topics are never taken as the company.
- **Account statements and other lists** no longer hand their many foreign IBANs to the sender: with three or
  more identifiers in the body text, only letterhead and footer count.
- A company can be marked as "never suggest as a possible duplicate" (e.g. your bank). Possible duplicates
  show why they were suggested; a shared IBAN no longer counts (it may be your own account number).
- Possible duplicates now also include companies with the same letterhead.
- `ablagefuchs-cli logo <files…>` prints how similar letterheads are (numbers only).

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
