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
- `src/keys.html` — das Zahnrad-Fenster: zeigt die Inhaltssteuerelemente **eines** Dokuments. Jedes lässt sich fürs Formular ab- und anwählen und bekommt bei Bedarf einen eigenen Schlüssel; auf Wunsch wird der Schlüssel als `w:tag` ins Dokument geschrieben. Es kennt nur Beschreibungen; gelesen und geschrieben wird im Hauptfenster.
- `src/history.html` — das Historie-Fenster: zeigt, was in dieser Sitzung ausgefüllt wurde, nach Ausfüll-Vorgang gegliedert. Es kennt nur Namen, Größen und Zeiten; Öffnen, Speichern und Drucken führt das Hauptfenster aus.
- `src/app.css` — Farben, Formularfelder, Schaltflächen, Statuszeile sowie Dokumentliste, Aufklappmenü und die Steuerelement-Liste des Zahnrad-Fensters, von allen vier Seiten benutzt. Muss in jeder Seite **vor** dem eigenen `<style>` eingebunden werden.
- `src/form-ui.js` — `renderFields` und `collectValues`, von Formularfenster und Ersatzdialog benutzt, damit die Darstellung nicht auseinanderläuft.
- `src/history-ui.js` — `renderHistory` und `size`, von Historie-Fenster und Ersatzdialog benutzt.
- `src/keys-ui.js` — `renderControls` und `collectOverrides`, von Zahnrad-Fenster und Ersatzdialog benutzt.
- `src/menu-ui.js` — die Aufklappmenüs (Kachel-Kontextmenü, Aktionen der Dokumentzeile). Das Menü-Element bleibt in der Seite, weil es dort am `<body>` hängen muss; die Mechanik steht hier.
- `src/sign.html` — die Unterschriftsseite für das Tablet. Sie ist die einzige Seite, die **nicht** im Webview läuft, sondern in Safari auf einem fremden Gerät: sie wird vom kleinen Server in `lib.rs` ausgeliefert und bringt Stil und Skript deshalb vollständig selbst mit — sie kann nichts nachladen. Sie liegt trotzdem in `src/`, damit `scripts/check.mjs` sie mitprüft.
- `src/vendor/` — mitgelieferte Bibliotheken, siehe unten.
- `src-tauri/src/lib.rs` — 30 Befehle. Zum Öffnen, Speichern und Drucken: `open_document`, `pick_documents`, `write_back`, `print_document`, `save_document`, `save_all_begin`, `save_all_write`, `save_all_finish`, `list_printers`. Sie nehmen Inhalt und Wunschnamen entgegen, nie einen Zielpfad. Für das Formularfenster: `open_form_window`, `form_payload`, `form_cache_values`, `form_submit`, `close_form_window`. Für die Historie: `history_publish`, `open_history_window`, `history_payload`, `history_action`. Für das Zahnrad-Fenster: `open_keys_window`, `keys_payload`, `keys_submit`, `close_keys_window`. Für die gemerkten Formulare: `group_save_meta`, `group_save_doc`, `group_list`, `group_read_doc`, `group_delete`. Für die Unterschrift vom Tablet: `sign_begin`, `sign_cancel`, `sign_qr`.
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

Reicht das nicht — dieselbe Frage heißt in zwei Vorlagen verschieden —, vergibt
man den Schlüssel im Zahnrad-Fenster von Hand. `keyOf()` legt diese
Überschreibung über `sdtKey()`; sie ist immer ein `w:tag`-Wert, ergibt also genau
den Schlüssel, den ein Dokument mit diesem Tag von sich aus bekäme. Deshalb
verschmilzt ein überschriebenes Feld mit einem, das das Tag wirklich trägt — und
wer den Schlüssel anschließend ins Dokument schreiben lässt, ändert am Ergebnis
nichts. Dort abgewählte Felder überspringen Lesen und Ausfüllen gleichermaßen:
sie erscheinen nicht im Formular und bleiben im Dokument unangetastet.

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
- Zwischen den Fenstern gehen nur Feldbeschreibungen, eingegebene Werte, die im Zahnrad-Fenster
  vergebenen Schlüssel und — für die Historie —
  Namen, Größen und Zeiten hin und her, niemals Dokumentinhalte. Die bleiben im Hauptfenster.
  Auch „Öffnen“, „Speichern“ und „Drucken“ aus der Historie werden dort ausgeführt; das
  Historie-Fenster schickt nur, welche Zeile gemeint war.
- Die Historie liegt ausschließlich im Arbeitsspeicher und ist mit dem Programm weg; auf die
  Platte kommt nichts. Damit das auch dann greift, wenn zuerst das Hauptfenster geschlossen wird,
  gehen die Nebenfenster mit ihm zu — sonst liefe der Prozess weiter und der Ablageordner bliebe
  liegen. Über einen langen Arbeitstag verwirft sie die ältesten Läufe, sobald sie 200 MB
  überschreitet.
- Ausgefüllte Dokumente sind immer Kopien; die Vorlage wird dabei nie angefasst. Die **einzige**
  Ausnahme im ganzen Programm ist der Schalter „Schlüssel dauerhaft ins Dokument schreiben" im
  Zahnrad-Fenster: er ersetzt die Vorlage ohne Nachfrage durch die Fassung mit den `w:tag`-Werten.
  Das geht nur bei Dateien, die über den Öffnen-Dialog geladen wurden — nur zu denen kennt Rust
  einen Pfad. Hereingezogene Dateien bringen keinen mit; für sie wird weiterhin eine Kopie über den
  Speichern-Dialog angeboten. Der Schalter im Fenster sagt jedes Mal, welcher der beiden Fälle gilt.
- Die Pfade der geöffneten Dateien liegen in Rust (`OriginPaths`), nicht in der Oberfläche. Die nennt
  beim Zurückschreiben nur die Kennung, die sie beim Öffnen bekommen hat — ein anderes Ziel kann sie
  nicht angeben. Geschrieben wird daneben und dann umbenannt (`write_over`), damit ein abgebrochener
  Schreibvorgang die alte Fassung nicht halb zerstört.
- Die Oberfläche kennt keine Zielpfade. Sie übergibt Inhalt und Wunschnamen; wohin
  gespeichert wird, entscheidet ein nativer Dialog innerhalb von Rust. Dateinamen werden
  dabei so bereinigt, dass sie den gewählten Ordner nicht verlassen können.
- Eingelesene Dateien sind auf 25 MB begrenzt, einzelne XML-Teile im Dokument auf
  40 MB entpackt. Das verhindert, dass ein präpariertes .docx den Arbeitsspeicher füllt.
- Beim Speichern aller Dokumente wird der Zielordner einmal abgefragt und in Rust
  gehalten; die Dokumente gehen einzeln hinüber, nie alle gleichzeitig.
- Die Vorschau im Browser rendert fremde Dokumentinhalte in einem `sandbox`-iframe
  ohne Skriptrechte.

### Unterschrift vom Tablet
- Der Server läuft **nur im lokalen Netz** und **nur**, solange der Dialog offen ist. Es gibt
  keinen Tunnel und keinen fremden Anbieter. Gestartet wird er erst beim ersten
  „Unterschreiben lassen" — wer das Feature nie benutzt, bindet nie einen Port, und die
  Firewall-Abfrage des Betriebssystems kommt an der Stelle, an der sie sich erklärt.
- **Das Dokument verlässt den Rechner nicht.** Ausgeliefert wird ausschließlich die eine
  einkompilierte Unterschriftsseite; der Server kennt keinen Pfad aus der Anfrage und kann
  strukturell nichts anderes herausgeben. Hinaus geht der Dateiname, herein kommt ein Bild.
- Der Link trägt ein Zeichen aus 128 Bit vom Zufallsgenerator des Browsers, gilt zehn Minuten
  und ist nach der ersten Unterschrift verbraucht. Alles andere — falsches Zeichen, abgelaufen,
  bereits benutzt — bekommt dieselbe nichtssagende Antwort. Nach zwanzig Fehlgriffen ist die
  Sitzung zu, damit niemand im selben WLAN in Ruhe raten kann.
- Die Verbindung ist unverschlüsselt. Das ist eine bewusste Entscheidung: über den Draht gehen
  nur der Dateiname und das Unterschriftsbild, nie der Vertrag. TLS bräuchte im lokalen Netz ein
  Zertifikat, dem das Tablet traut; ein selbstsigniertes erzeugt nur eine Warnung, die weggeklickt
  wird und damit gar nichts mehr schützt.
- Neben dem Bild legt Docfill einen Beleg als `docfill/unterschrift.json` ins Dokument:
  Prüfsumme der unterschriebenen Fassung, Zeitpunkt und der aufgezeichnete Schreibvorgang.
  Die Prüfsumme ist das Einzige, was eine spätere Änderung erkennbar macht. **Rechtlich** ist das
  eine einfache elektronische Signatur — sie genügt der gesetzlichen Schriftform nach § 126 BGB
  nicht, und wo die elektronische Form ausgeschlossen ist (Kündigung § 623 BGB, Bürgschaft
  § 766 BGB), hilft sie gar nicht.
- Die aufgezeichneten Schreibdaten sind personenbezogen. Die Unterschriftsseite sagt das
  ungefragt, bevor unterschrieben wird.

**Beim Entwickeln beachten:** `src/sign.html` wird über `include_str!` zur Übersetzungszeit ins
Programm eingebacken — sie muss ja auf einem fremden Gerät ankommen, ohne dass Docfill dort
Dateien nachliest. Eine Änderung daran wirkt deshalb **erst nach einem Rust-Neubau**, und
`tauri dev` beobachtet nur `src-tauri`. Also `npm run dev` neu starten. Die übrigen Seiten in
`src/` genügt es im Fenster neu zu laden.
- Die zugehörigen Tests laufen mit `cd src-tauri && cargo test`.
