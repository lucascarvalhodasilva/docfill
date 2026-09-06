# Docfill (Tauri)

Dieselben Inhaltssteuerelemente in mehreren Word-Dokumenten ausfüllen, ohne sie zu öffnen.

## Voraussetzungen
- Rust (https://rustup.rs) und Node.js
- Tauri-Voraussetzungen für das jeweilige Betriebssystem: https://tauri.app/start/prerequisites/
- Drucken: unter Windows übernimmt das **Word** über das „Drucken“-Verb der Shell — dasselbe,
  das im Explorer im Kontextmenü steht. Dafür wird nichts zusätzlich gebraucht.
  Auf macOS und Linux (und unter Windows ohne Word) springt **LibreOffice** ein: es wandelt
  das Dokument im Hintergrund nach PDF, das dann an den Drucker geht. Fehlt beides, weist
  „Drucken“ darauf hin, die Datei zu öffnen und von dort zu drucken.

## Erster Start
```bash
npm install
npx tauri icon app-icon.png     # erzeugt src-tauri/icons/* aus dem Platzhalter (später durch eigenes PNG ersetzen)
npm run dev                      # Entwicklungsfenster
npm run build                    # Installationspakete in src-tauri/target/release/bundle/
```

## Aufbau
- `src/index.html` — die gesamte Oberfläche (auf Deutsch). Läuft auch in einem normalen Browser; innerhalb von Tauri werden stattdessen native Dialoge, Dateizugriffe, „mit Word öffnen“ und das Drucken im Hintergrund verwendet.
- `src/form.html` — das Formularfenster: ein zweites, echtes Fenster, das über die Kachel im Hauptfenster geöffnet wird. Es zeigt nur Felder an und schickt die Werte zurück; die Dokumente bleiben im Hauptfenster.
- `src/app.css` — Farben, Formularfelder, Schaltflächen und Statuszeile, von beiden Seiten benutzt. Muss in beiden Seiten **vor** dem eigenen `<style>` eingebunden werden.
- `src/form-ui.js` — `renderFields` und `collectValues`, ebenfalls von beiden Seiten benutzt, damit die Darstellung nicht auseinanderläuft.
- `src/vendor/` — mitgelieferte Bibliotheken, siehe unten.
- `src-tauri/src/lib.rs` — elf Befehle. Zum Speichern und Öffnen: `open_document`, `print_document`, `save_document`, `save_all_begin`, `save_all_write`, `save_all_finish`. Sie nehmen Inhalt und Wunschnamen entgegen, nie einen Zielpfad. Für das Formularfenster: `open_form_window`, `form_payload`, `form_cache_values`, `form_submit`, `close_form_window`.
- `src-tauri/capabilities/default.json` — nur `core:default`; die Oberfläche hat keinen Zugriff auf Dateisystem, Dialoge oder Shell.

## Wie ein Feld in mehreren Dokumenten wiedererkannt wird

Damit dieselbe Angabe alle ausgewählten Dokumente füllt, brauchen die
Inhaltssteuerelemente einen gemeinsamen Schlüssel. `sdtKey()` in `src/index.html`
vergibt ihn, in dieser Reihenfolge:

1. `w:tag` — die saubere Lösung, wenn die Vorlage sie pflegt.
2. `w:alias` — der in Word angezeigte Titel.
3. **Der Platzhaltertext**, sofern das Feld noch nicht ausgefüllt ist
   (`w:showingPlcHdr`). Viele Vorlagen tragen weder Tag noch Alias; dann ist die
   Frage selbst („Branche:“) das einzige verlässliche Merkmal.
4. Die Position im Dokument — nur als letzte Rettung und **auf das jeweilige
   Dokument beschränkt**, damit zwei verschieden lange Dokumente an derselben
   Stelle nicht fremde Felder zusammenwerfen.

Lesen (`readControls`) und Ausfüllen (`fillDoc`) rufen dieselbe Funktion auf und
zählen die Position über alle Dokumentteile hinweg — Kopf- und Fußzeilen
eingeschlossen. Weichen die beiden Zählungen voneinander ab, landen Werte in
falschen Feldern.

## Mitgelieferte Bibliotheken

Alle Abhängigkeiten der Oberfläche liegen lokal in `src/vendor/`. Nichts wird zur Laufzeit
nachgeladen: die App funktioniert vollständig offline, und die Content-Security-Policy in
`src-tauri/tauri.conf.json` (`script-src 'self'`) verbietet externe Skripte.

| Datei | Herkunft |
| --- | --- |
| `jszip.min.js` | JSZip 3.10.1, unverändert übernommen |
| `docx-preview.min.js` | docx-preview 0.3.3, unverändert übernommen |
| `tauri-api.js` | **erzeugt** — nicht von Hand bearbeiten |

Die beiden fremden Bibliotheken sind mit Prüfsummen festgeschrieben. Nach jedem
Austausch prüfen:

```bash
cd src/vendor && shasum -a 256 -c SHA256SUMS
```


`tauri-api.js` enthält `invoke` und `listen` — `listen` nur, damit das Hauptfenster erfährt,
wann das Formularfenster seine Werte abgeschickt hat. Datei-Dialoge, Schreibzugriffe und „Öffnen mit“
laufen vollständig in Rust, deshalb braucht die Oberfläche keine weitere Tauri-API. Das
Modul wird als ES-Modul eingebunden, damit nichts davon am `window`-Objekt hängt
(`withGlobalTauri` steht auf `false`).

**Nach jedem Tauri-Update neu erzeugen**, sonst laufen JavaScript- und Rust-Seite
auseinander:

```bash
npx esbuild vendor-entry.js --bundle --format=esm --minify --outfile=src/vendor/tauri-api.js
```

Der Einstiegspunkt dafür ist `vendor-entry.js` im Projektstamm. Kommt eine weitere
Tauri-API dazu, dort exportieren und den Befehl erneut ausführen.

## Sicherheitshinweise
- Zum Öffnen und Drucken werden Dokumente in einem Ordner zwischengespeichert, dessen Name pro Programmstart
  neu vergeben wird (nur für den eigenen Benutzer lesbar, `0700`/`0600`). Beim Beenden
  wird der Ordner gelöscht — ausgefüllte Verträge bleiben also nicht im Temp-Verzeichnis
  liegen.
- `print_document` und `open_document` arbeiten ausschließlich mit Dateien aus genau
  diesem Ordner. Alles andere wird abgewiesen, bevor LibreOffice oder die Shell es zu
  sehen bekommt.
- Das Formularfenster wird in Rust erzeugt, nicht im JavaScript. Damit bleibt es dabei, dass
  die Oberfläche keine Adresse nennen kann; die Berechtigung
  `core:webview:allow-create-webview-window` wird nicht vergeben.
- Zwischen den beiden Fenstern gehen nur Feldbeschreibungen und eingegebene Werte hin und her,
  niemals Dokumentinhalte. Die bleiben im Hauptfenster.
- Die Oberfläche kennt keine Zielpfade. Sie übergibt Inhalt und Wunschnamen; wohin
  gespeichert wird, entscheidet ein nativer Dialog innerhalb von Rust. Dateinamen werden
  dabei so bereinigt, dass sie den gewählten Ordner nicht verlassen können.
- Eingelesene Dateien sind auf 25 MB begrenzt, einzelne XML-Teile im Dokument auf
  40 MB entpackt. Das verhindert, dass ein präpariertes .docx den Arbeitsspeicher füllt.
- Beim Speichern aller Dokumente wird der Zielordner einmal abgefragt und in Rust
  gehalten; die Dokumente gehen einzeln hinüber, nie alle gleichzeitig.
- Die Vorschau im Browser rendert fremde Dokumentinhalte in einem `sandbox`-iframe
  ohne Skriptrechte.
- Die zugehörigen Tests laufen mit `cd src-tauri && cargo test`.
