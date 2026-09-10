# Docfill unter Windows bauen

Es gibt zwei Wege zur Installationsdatei. Der erste braucht keinen
Windows-Rechner.

## Der einfache Weg: GitHub bauen lassen

Auf GitHub unter **Actions ▸ build ▸ Run workflow** den Lauf auf `main` starten.
Der erste Durchlauf dauert 10–20 Minuten, spätere ein bis zwei. Danach liegt
unten im Lauf unter **Artifacts** das Paket `docfill-windows-installer`:
herunterladen und entpacken. Darin die `…-setup.exe` für den USB-Stick, die
`.msi` für eine zentrale Verteilung und die `SHA256SUMS.txt` zum Gegenprüfen.

Gebaut wird nach `.github/workflows/build.yml` auf einem Windows-Rechner von
GitHub. Node, Rust und die Build Tools sind dort schon eingerichtet — die
Installation aus Abschnitt 2 entfällt damit komplett.

**Was wann läuft.** Jeder Push prüft: Tests, `cargo audit` und die Prüfung der
Oberfläche, auf Linux in ein bis zwei Minuten. Der Windows-Installer entsteht
dabei nicht — er dauert zehn bis zwanzig Minuten und brächte nichts, was die
Prüfung nicht schon gesagt hätte. Gebaut wird nur bei einem Versionsschild oder
wenn der Lauf von Hand gestartet wird.

## Eine Fassung veröffentlichen

Ein Versionsschild löst denselben Lauf aus und hängt das Ergebnis zusätzlich an
ein Release — anders als das Artefakt oben verfällt das nicht nach 30 Tagen und
hat einen festen Link. Das Repo ist öffentlich: der Link lässt sich weitergeben,
zum Herunterladen braucht niemand ein Konto.

Zuerst die Version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` und
`package.json` auf die neue Nummer setzen und einchecken — die erste benennt die
Installationsdatei. Weicht eine davon vom Schild ab, bricht der Lauf gleich zu
Beginn ab, statt eine falsch benannte Datei zu veröffentlichen.

Auch hier läuft die Prüfung zuerst — Tests, `cargo audit`, Oberfläche. Ist dort
etwas rot, entsteht kein Release.

```bash
git tag v0.2.1
git push origin v0.2.1
```

Danach steht unter **Releases** `Docfill_0.2.1_x64-setup.exe` samt `.msi` und
der `SHA256SUMS.txt` zum Gegenprüfen.

## Der andere Weg: von Hand auf einem Windows-Rechner

Nötig, wenn ohne GitHub gebaut werden soll. Der Rest dieser Anleitung richtet
sich an die Person, die den Windows-Rechner hat. Einmaliger Aufwand: etwa 30
Minuten, davon das meiste Wartezeit.

### 1. Projekt auf den Windows-Rechner kopieren

Den Projektordner `docfill-tauri` kopieren — **ohne** diese beiden Unterordner,
die nur Zwischenergebnisse enthalten und mehrere Gigabyte groß sein können:

- `node_modules`
- `src-tauri/target`

### 2. Einmalig installieren

| Was | Woher | Hinweis |
| --- | --- | --- |
| Node.js (LTS) | https://nodejs.org | Standardinstallation genügt |
| Rust | https://rustup.rs | `rustup-init.exe`, alle Vorgaben bestätigen |
| Visual Studio Build Tools | https://visualstudio.microsoft.com/visual-cpp-build-tools/ | Beim Installieren die Arbeitslast **„Desktopentwicklung mit C++"** ankreuzen |

Die Build Tools sind der übliche Stolperstein: ohne die C++-Arbeitslast bricht der
Build später mit einer Meldung über einen fehlenden Linker (`link.exe`) ab.

Nach der Installation einmal ab- und wieder anmelden, damit die Pfade greifen.

### 3. Bauen

Eingabeaufforderung oder PowerShell im Projektordner öffnen:

```bat
npm install
npm run build
```

Der erste Durchlauf dauert 10–20 Minuten, weil Rust alle Abhängigkeiten übersetzt.
Spätere Durchläufe brauchen ein bis zwei Minuten.

### 4. Ergebnis

Die fertige Installationsdatei liegt hier:

```
src-tauri\target\release\bundle\nsis\Docfill_0.2.1_x64-setup.exe
```

Diese eine Datei auf den USB-Stick kopieren — mehr wird nicht gebraucht.

Daneben entstehen noch `...\bundle\msi\Docfill_0.2.1_x64_en-US.msi` (für eine
zentrale Verteilung durch die IT) und `src-tauri\target\release\docfill.exe`
(die Anwendung ohne Installation). Für den Test genügt die Datei aus `nsis`.

---

## Hinweis für die Kolleginnen und Kollegen

Dieser Abschnitt kann als Textdatei mit auf den Stick gelegt werden.

**Installation.** `Docfill_0.2.1_x64-setup.exe` doppelklicken. Die Installation
erfolgt nur für das eigene Benutzerkonto, ein Administratorkennwort wird nicht
gebraucht. Danach steht Docfill im Startmenü; deinstallieren lässt es sich unter
*Einstellungen ▸ Apps* wie jedes andere Programm.

**Windows zeigt eine blaue Warnung.** „Der Computer wurde durch Windows
geschützt" erscheint, weil das Programm kein gekauftes Zertifikat hat — nicht,
weil etwas nicht stimmt. Auf **Weitere Informationen** und dann auf **Trotzdem
ausführen** klicken. Die Meldung kommt nur beim ersten Mal.

**Ist es wirklich die echte Datei?** Weil das Zertifikat fehlt, kann Windows die
Frage nicht beantworten — die Prüfsumme kann es. Bei jedem Release liegt eine
Datei `SHA256SUMS.txt`. Die Zeile für den eigenen Installer mit dem vergleichen,
was dieser Befehl in der Eingabeaufforderung ausgibt:

```
certutil -hashfile Docfill_0.2.1_x64-setup.exe SHA256
```

Stimmen die beiden Zeichenfolgen überein, ist die Datei unverändert. Stimmen sie
nicht überein, die Datei **nicht** ausführen und Bescheid geben.

**Drucken.** Die Schaltfläche „Drucken" gibt das Dokument an Word weiter, genau
wie ein Rechtsklick ▸ Drucken im Explorer. Wo kein Word installiert ist, wird
LibreOffice benutzt; fehlt auch das, stattdessen „Öffnen" verwenden und von dort
drucken.

**Die Originaldokumente werden nie verändert.** Docfill legt immer neue Dateien
mit der Endung `_ausgefuellt.docx` an — bei einer makrofähigen Ausgangsdatei
`_ausgefuellt.docm`. Aus einer Vorlage (`.dotx`, `.dotm`) wird dabei ein Dokument;
die Vorlage selbst bleibt unverändert liegen.
