# Docfill (Tauri)

Dieselben Inhaltssteuerelemente in mehreren Word-Dokumenten und -Vorlagen ausfüllen, ohne sie zu öffnen.

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
- `src/index.html` — die gesamte Oberfläche (auf Deutsch). Läuft auch in einem normalen Browser; innerhalb von Tauri werden stattdessen native Dialoge, Dateizugriffe, „mit Word öffnen“ und das Drucken im Hintergrund verwendet. Hier steht auch `FORMATE`, die einzige Stelle, an der die vier angenommenen Dateitypen (`.docx`, `.dotx`, `.docm`, `.dotm`) beschrieben sind — siehe „Vorlagen und Makros“.
- `src/form.html` — das Formularfenster: ein zweites, echtes Fenster, das über die Kachel im Hauptfenster geöffnet wird. Es zeigt nur Felder an und schickt die Werte zurück; die Dokumente bleiben im Hauptfenster.
- `src/keys.html` — das Zahnrad-Fenster: zeigt die Inhaltssteuerelemente **eines** Dokuments. Jedes lässt sich fürs Formular ab- und anwählen, bekommt bei Bedarf einen eigenen Schlüssel und — je nach Typ — eigene Listeneinträge, ein Datumsformat oder andere Ankreuz-Zeichen; auf Wunsch wird das alles dauerhaft ins Dokument geschrieben. Es kennt nur Beschreibungen; gelesen und geschrieben wird im Hauptfenster.
- `src/history.html` — das Historie-Fenster: zeigt, was in dieser Sitzung ausgefüllt wurde, nach Ausfüll-Vorgang gegliedert. Es kennt nur Namen, Größen und Zeiten; Öffnen, Speichern und Drucken führt das Hauptfenster aus.
- `src/app.css` — Farben, Formularfelder, Schaltflächen, Statuszeile sowie Dokumentliste, Aufklappmenü und die Steuerelement-Liste des Zahnrad-Fensters, von allen vier Seiten benutzt. Muss in jeder Seite **vor** dem eigenen `<style>` eingebunden werden.
- `src/form-ui.js` — `renderFields` und `collectValues`, von Formularfenster und Ersatzdialog benutzt, damit die Darstellung nicht auseinanderläuft. Dort steht auch `TYPE_LABEL`: jeder Feldtyp bekommt sein eigenes Eingabefeld und seinen eigenen Namen — siehe „Die Feldtypen“.
- `src/history-ui.js` — `renderHistory` und `size`, von Historie-Fenster und Ersatzdialog benutzt.
- `src/keys-ui.js` — `renderControls` und `collectOverrides`, von Zahnrad-Fenster und Ersatzdialog benutzt.
- `src/menu-ui.js` — die Aufklappmenüs (Kachel-Kontextmenü, Aktionen der Dokumentzeile). Das Menü-Element bleibt in der Seite, weil es dort am `<body>` hängen muss; die Mechanik steht hier.
- `src/sign.html` — die Unterschriftsseite für das Tablet. Sie ist die einzige Seite, die **nicht** im Webview läuft, sondern in Safari auf einem fremden Gerät: sie wird vom kleinen Server in `lib.rs` ausgeliefert und bringt Stil und Skript deshalb vollständig selbst mit — sie kann nichts nachladen. Sie liegt trotzdem in `src/`, damit `scripts/check.mjs` sie mitprüft.
- `src/vendor/` — mitgelieferte Bibliotheken, siehe unten.
- `src-tauri/src/lib.rs` — 30 Befehle. Zum Öffnen, Speichern und Drucken: `open_document`, `pick_documents`, `write_back`, `print_document`, `save_document`, `save_all_begin`, `save_all_write`, `save_all_finish`, `list_printers`. Sie nehmen Inhalt und Wunschnamen entgegen, nie einen Zielpfad. Für das Formularfenster: `open_form_window`, `form_payload`, `form_cache_values`, `form_submit`, `close_form_window`. Für die Historie: `history_publish`, `open_history_window`, `history_payload`, `history_action`. Für das Zahnrad-Fenster: `open_keys_window`, `keys_payload`, `keys_submit`, `close_keys_window`. Für die gemerkten Formulare: `group_save_meta`, `group_save_doc`, `group_list`, `group_read_doc`, `group_delete`. Für die Unterschrift vom Tablet: `sign_begin`, `sign_cancel`, `sign_qr`. Rust liest kein OOXML und soll es auch nicht: von den vier Dateitypen kennt es nur die vier Endungen, und die allein für die beiden nativen Dialoge.
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

**Im Formular heißt ein Feld wie sein Schlüssel.** `scanFields` nimmt dafür
`autoKey()`, das aus „tag:Branche" wieder „Branche" macht — dieselbe Zeichenkette
also, die im Zahnrad-Fenster im Schlüssel steht. Wer dort einen eigenen vergibt,
sieht ihn im Formular wieder; zwei Namen für dieselbe Sache gibt es nicht. Trägt
ein Steuerelement gar keinen festen Schlüssel (`pos:…`), bleibt der
Platzhaltertext und zuletzt eine Nummer.

Das heißt auch: ein `w:tag` schlägt einen `w:alias`, weil `sdtKey` es so
staffelt. Wo eine Vorlage beides pflegt und der Alias der schönere Name ist,
trägt man ihn im Zahnrad-Fenster als Schlüssel ein.

Lesen (`readControls`) und Ausfüllen (`fillDoc`) rufen dieselbe Funktion auf und
zählen die Position über alle Dokumentteile hinweg — Kopf- und Fußzeilen
eingeschlossen. Weichen die beiden Zählungen voneinander ab, landen Werte in
falschen Feldern.

## Die Feldtypen

Word unterscheidet mehrere Arten von Inhaltssteuerelementen, und das Formular
zeigt sie verschieden — sonst sähe ein Feld, das nur vorgegebene Antworten
annimmt, aus wie eines, in das man alles tippen darf. `readControls` in
`src/index.html` erkennt den Typ, `renderFields` in `src/form-ui.js` zeichnet
ihn, `app.css` gibt ihm seine Gestalt.

| Im Dokument | Typ | Im Formular |
| --- | --- | --- |
| `w:text` | `text` | einzeiliges Feld |
| `w:text w:multiLine="1"` | `multi` | mehrzeiliges Feld |
| `w:richText` | `rich` | mehrzeiliges Feld |
| `w:dropDownList` | `dropdown` | Auswahlliste, abgesetzter Pfeil — nur die Einträge |
| `w:comboBox` | `combo` | Liste mit gestricheltem Pfeil — Einträge **oder** eigener Text |
| `w:date` | `date` | Datumsfeld mit Kalender |
| `w14:checkbox` | `checkbox` | Ankreuzfeld, schreibt die Zeichen der Vorlage |
| `w:picture` | `picture` | gar nicht — das ist die Stelle für die Unterschrift |

Die Zeichen im Feld (`▾`) kommen aus der Schrift, nicht aus einer Bilddatei: die
Content-Security-Policy verbietet Nachladen, und ein `data:`-URI im Stylesheet
wäre schwerer zu pflegen als ein Buchstabe. Der Unterschied zwischen
Auswahlliste und Kombinationsfeld steckt genau in diesem Pfeil — abgesetzt heißt
„nur das hier“, gestrichelt heißt „oder etwas Eigenes“.

Bringt eine Liste einen Eintrag mit **leerem `w:value`** mit, ist das in Word
die Zeile „nichts gewählt". Docfill zeigt sie als Platzhalter — statt einer
eigenen Zeile „— auswählen —" — und schreibt sie nie ins Dokument. In `options`
bleibt sie trotzdem stehen, weil der Listen-Editor im Zahnrad-Fenster daraus
zurückschreibt; fiele sie dort heraus, verschwände sie aus der Vorlage.
Ausdrücklich leer muss der Wert sein: ein ganz **fehlendes** `w:value` gehört zu
einem Eintrag, der sehr wohl geschrieben werden will.

`rich` und `multi` sehen gleich aus. Das ist ehrlich: beides ist mehrzeiliger
Text, und was Word daran unterscheidet — dass Rich Text Formatierung behalten
kann — lässt sich in einem Eingabefeld nicht ausdrücken. Die Beschriftung sagt,
was es ist.

### Was sich am Feld einstellen lässt

Im Zahnrad-Fenster hängt an jedem Feld der **Platzhaltertext** — der graue, den
das Steuerelement im Dokument zeigt — und dazu, was zu seinem Typ gehört: die
**Listeneinträge** einer Auswahlliste oder eines Kombinationsfelds, das
**Datumsformat** und die beiden **Ankreuz-Zeichen**. Vorbelegt ist immer, was in
der Vorlage steht — Docfill liest `w:listItem`, `w:dateFormat` und
`w14:checkedState` und erfindet nichts dazu. Vorher schrieb es fest ☒/☐ und sein
eigenes Datumsformat hinein; eine Vorlage mit Wingdings-Haken bekam damit ein
Zeichen, das ihre Schrift gar nicht kennt.

Was abweicht, liegt bei den übrigen Überschreibungen (`overrides`) und wird mit
gemerkten Formularen gespeichert. Es steuert das Formular **und** das, was ins
ausgefüllte Dokument geschrieben wird. Der Schalter in der Fußzeile schreibt es
zusätzlich dauerhaft in die Vorlage: `setListItems`, `setDateFormat` und
`setCheckStates` in `src/index.html` setzen dafür genau die Stellen, aus denen
gelesen wurde. Danach bringt die Vorlage die Einstellung selbst mit, und die
Überschreibung fällt weg — dasselbe Spiel wie beim Schlüssel.

Der Platzhaltertext steht **nicht** in der Überschrift der Zeile, obwohl er das
Feld am besten beschreibt. Der Grund steht in echten Vorlagen: dort tragen oft
alle Felder denselben („Click here to enter text."), während `w:alias`
unterscheidet — als Überschrift sähen dann alle Zeilen gleich aus. Er bekommt
deshalb ein eigenes Feld, das beides leistet: es zeigt, was im Dokument steht,
und lässt dort einen eintragen, wo keiner ist.

Wo noch keiner da ist, bleibt das Eingabefeld leer und der derzeitige Inhalt
steht grau darin. Als Wert dürfte er dort nicht stehen — sonst zählte jedes
solche Feld beim Schließen als geändert, ohne dass jemand es angefasst hätte.
Sichtbar muss er trotzdem sein: einen Platzhalter zu schreiben **ersetzt** den
Inhalt des Steuerelements.

Zwei Dinge hängen daran. `sdtKey` nimmt den Platzhaltertext an dritter Stelle
als Schlüssel — ein Feld ohne `w:tag` und ohne `w:alias` heißt danach also
`hint:…` statt `pos:…`, und `neuerSchluessel` in `src/index.html` zieht `hidden`
und `optional` mit um. Und `setPlaceholder` setzt `w:showingPlcHdr` **vor** dem
Typ-Element ein, weil die Kinder von `sdtPr` eine Folge in fester Reihenfolge
sind — dieselbe Regel wie bei `setTag`.

Beim Ersetzen der Listeneinträge bleibt ein vorhandener `w:value` stehen: Word
bindet daran mitunter Daten, und bearbeitet wurde nur der angezeigte Text.
Zugeordnet wird er über den Anzeigetext, nicht über die Stelle in der Liste —
sonst erbte beim Entfernen einer Zeile die nächste deren Wert, und aus einem
gelöschten Platzhalter würde unbemerkt ein neuer.

### Leere Felder

Aufgehalten wird niemand — aber es wird gesagt, wenn etwas leer geblieben ist,
denn dort steht im Ergebnis noch, was die Vorlage vorgibt. An zwei Stellen:
mitlaufend in der Fußzeile des Formulars („3 von 8 Feldern noch leer") und
danach an jeder fertigen Datei in der Ergebnisliste („3 Felder leer", gelb).

Gezählt wird je **Dokument**, nicht je Formular: ein Feld, das in einer der
Dateien gar nicht vorkommt, fehlt dort auch nicht. Leer heißt schlicht, dass
`collectValues` für den Schlüssel nichts geliefert hat. Ein Ankreuzfeld liefert
immer ☒ oder ☐ und ist damit nie leer; ein Bildfeld steht gar nicht erst im
Formular. Beides ergibt sich von selbst, ohne Sonderfall im Code.

Eine Kennzeichnung einzelner Felder als „Pflicht" gibt es bewusst nicht: gewarnt
wird über leere Felder, und damit entscheidet der Inhalt, nicht eine zusätzliche
Einstellung, die man erst pflegen müsste.

### Kein Name neben dem Haken

Im Zahnrad-Fenster steht neben dem Haken, mit dem man ein Steuerelement fürs
Formular an- und abwählt, **kein Name** mehr. Er kam aus `w:alias` oder `w:tag`
und war dort eher verwirrend als hilfreich: „OrderFreefield03" sagt niemandem,
um welches Feld es geht, und wo die Vorlage gar nichts pflegt, stand dort
„Feld 7".

Woran man die Zeile erkennt, steht jetzt darunter — im **Schlüssel** und im
**Platzhaltertext**, beides lesbar und beides änderbar. Fürs Vorleseprogramm
trägt der Haken den Namen weiterhin als `aria-label`; sichtbar ist er nicht mehr.

In der Kopfzeile stehen **Feldtyp** und Haken „Im Formular" beide **rechts**, als
ein Block Nebenangaben. Links bleibt frei: dort begänne sonst eine Beschriftung
an genau der Stelle, an der man den Namen des Steuerelements erwartet — und
würde prompt für einen gehalten.
Ein abgewähltes Feld tritt als Ganzes zurück; nur der Haken selbst bleibt
kräftig, denn er ist das, was man dann anklicken will.

Rechts neben dem Haken klappt ein Falter die Zeile zu. Zugeklappt bleibt die
Kopfzeile stehen und bekommt links den **Schlüssel** dazu — ohne ihn wüsste man
bei einer zugeklappten Zeile nicht mehr, welches Feld sie meint. Abgelesen wird
er erst beim Zuklappen, damit ein inzwischen geänderter auch dort steht.
Eingaben bleiben dabei erhalten und werden weiterhin eingesammelt; der Zustand
hält nur, solange das Fenster offen ist.

## Vorlagen und Makros

Docfill nimmt vier Dateitypen an. Innen sind sie dasselbe — dieselben Teile,
dieselben `w:sdt`; unterschieden werden sie allein durch den Typ des Haupt-Teils
in `[Content_Types].xml`. Die Tabelle `FORMATE` in `src/index.html` ist die
einzige Stelle, an der das steht.

| Endung | Haupt-Teil in `[Content_Types].xml` | Ergebnis beim Ausfüllen |
| --- | --- | --- |
| `.docx` | `…wordprocessingml.document.main+xml` | `.docx` |
| `.dotx` | `…wordprocessingml.template.main+xml` | **`.docx`** |
| `.docm` | `application/vnd.ms-word.document.macroEnabled.main+xml` | `.docm` |
| `.dotm` | `application/vnd.ms-word.template.macroEnabled.main+xml` | **`.docm`** |

**Aus einer Vorlage wird beim Ausfüllen ein Dokument.** `alsDokument()` schreibt
dafür den Haupt-Teil-Typ um. Bliebe er „Vorlage“, legte Word beim Doppelklick
auf das Ergebnis wieder eine neue unbenannte Kopie an, statt die ausgefüllte
Datei zu zeigen — ein ausgefülltes Formular ist keine Vorlage mehr. Makrofähig
bleibt makrofähig, sonst fiele `word/vbaProject.bin` aus dem Paket und Word böte
an, die Datei zu reparieren.

**Entschieden wird am Paket, nicht am Dateinamen.** `formatVon()` liest den
Inhaltstyp aus `[Content_Types].xml`; die Endung ist nur der Notnagel. Eine im
Explorer nach `.docx` umbenannte Vorlage ist innen unverändert eine Vorlage, und
Word richtet sich nach dem Paket. Gefunden wird der Eintrag über seinen
`PartName` `/word/document.xml` — denselben Pfad, den auch `PARTS` an erster
Stelle nennt, damit beide nicht auseinanderlaufen. `word/glossary/document.xml`,
die Bausteine, die Vorlagen oft mitbringen, steht unter einem anderen Pfad und
einem anderen Typ und fällt so doppelt heraus.

`w:attachedTemplate` in `settings.xml` wird **nicht** angefasst. Word setzt dort
den Pfad der Vorlage ein, aus der ein Dokument entstanden ist — Docfill kennt
gar keinen Pfad (die liegen in Rust, siehe unten), und ein Pfad auf den eigenen
Rechner hat in einer Datei nichts zu suchen, die anschließend verschickt wird.
Gebraucht wird der Eintrag auch nicht: Formatvorlagen, Nummerierungen, Schriften
und das VBA-Projekt stecken vollständig im Paket selbst.

`.doc` und `.rtf` bleiben draußen. Das sind keine ZIP-Pakete, sondern ganz andere
Formate — sie zu öffnen wäre ein Konverter, kein Dateifilter.

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
  Zahnrad-Fenster: er ersetzt die Vorlage ohne Nachfrage durch die Fassung mit den dort gemachten
  Einstellungen — Schlüssel (`w:tag`), Listeneinträge, Datumsformat und Ankreuz-Zeichen.
  Das geht nur bei Dateien, die über den Öffnen-Dialog geladen wurden — nur zu denen kennt Rust
  einen Pfad. Hereingezogene Dateien bringen keinen mit; für sie wird weiterhin eine Kopie über den
  Speichern-Dialog angeboten. Der Schalter im Fenster sagt jedes Mal, welcher der beiden Fälle gilt.
  Dabei bleibt eine Vorlage eine Vorlage: geschrieben werden nur die `w:tag`-Werte, der Pakettyp
  wird nicht angefasst — anders als beim Ausfüllen, wo aus einer `.dotx` bewusst eine `.docx` wird.
- **Makros.** Docfill fügt keine hinzu und entfernt keine: `.docx` bleibt `.docx`, und ein
  ausgefülltes `.docm` trägt das VBA-Projekt seiner Vorlage unverändert weiter. Ob es je läuft,
  entscheidet allein das Sicherheitscenter von Word — der Ablageordner bekommt bei jedem
  Programmstart einen neuen Namen und kann deshalb nie ein vertrauenswürdiger Speicherort werden.
  **Beim Drucken läuft es nie:** Word wird dafür mit `AutomationSecurity = ForceDisable` gestartet.
  Ohne das würde ein Klick auf „Drucken" ein `AutoOpen`-Makro in einem unsichtbaren Word ausführen,
  denn unter COM-Steuerung lässt Word Makros standardmäßig ohne Rückfrage laufen.
- Die Pfade der geöffneten Dateien liegen in Rust (`OriginPaths`), nicht in der Oberfläche. Die nennt
  beim Zurückschreiben nur die Kennung, die sie beim Öffnen bekommen hat — ein anderes Ziel kann sie
  nicht angeben. Geschrieben wird daneben und dann umbenannt (`write_over`), damit ein abgebrochener
  Schreibvorgang die alte Fassung nicht halb zerstört.
- Die Oberfläche kennt keine Zielpfade. Sie übergibt Inhalt und Wunschnamen; wohin
  gespeichert wird, entscheidet ein nativer Dialog innerhalb von Rust. Dateinamen werden
  dabei so bereinigt, dass sie den gewählten Ordner nicht verlassen können.
- Eingelesene Dateien sind auf 25 MB begrenzt, einzelne XML-Teile im Dokument auf
  40 MB entpackt. Das verhindert, dass eine präparierte Word-Datei den Arbeitsspeicher füllt.
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
