# Docfill unter Windows bauen

Diese Anleitung richtet sich an die Person, die den Windows-Rechner hat. Am Ende
entsteht eine Installationsdatei, die per USB-Stick weitergegeben werden kann.
Einmaliger Aufwand: etwa 30 Minuten, davon das meiste Wartezeit.

## 1. Projekt auf den Windows-Rechner kopieren

Den Projektordner `docfill-tauri` kopieren — **ohne** diese beiden Unterordner,
die nur Zwischenergebnisse enthalten und mehrere Gigabyte groß sein können:

- `node_modules`
- `src-tauri/target`

## 2. Einmalig installieren

| Was | Woher | Hinweis |
| --- | --- | --- |
| Node.js (LTS) | https://nodejs.org | Standardinstallation genügt |
| Rust | https://rustup.rs | `rustup-init.exe`, alle Vorgaben bestätigen |
| Visual Studio Build Tools | https://visualstudio.microsoft.com/visual-cpp-build-tools/ | Beim Installieren die Arbeitslast **„Desktopentwicklung mit C++"** ankreuzen |

Die Build Tools sind der übliche Stolperstein: ohne die C++-Arbeitslast bricht der
Build später mit einer Meldung über einen fehlenden Linker (`link.exe`) ab.

Nach der Installation einmal ab- und wieder anmelden, damit die Pfade greifen.

## 3. Bauen

Eingabeaufforderung oder PowerShell im Projektordner öffnen:

```bat
npm install
npm run build
```

Der erste Durchlauf dauert 10–20 Minuten, weil Rust alle Abhängigkeiten übersetzt.
Spätere Durchläufe brauchen ein bis zwei Minuten.

## 4. Ergebnis

Die fertige Installationsdatei liegt hier:

```
src-tauri\target\release\bundle\nsis\Docfill_0.1.0_x64-setup.exe
```

Diese eine Datei auf den USB-Stick kopieren — mehr wird nicht gebraucht.

Daneben entstehen noch `...\bundle\msi\Docfill_0.1.0_x64_en-US.msi` (für eine
zentrale Verteilung durch die IT) und `src-tauri\target\release\docfill.exe`
(die Anwendung ohne Installation). Für den Test genügt die Datei aus `nsis`.

---

## Hinweis für die Kolleginnen und Kollegen

Dieser Abschnitt kann als Textdatei mit auf den Stick gelegt werden.

**Installation.** `Docfill_0.1.0_x64-setup.exe` doppelklicken. Die Installation
erfolgt nur für das eigene Benutzerkonto, ein Administratorkennwort wird nicht
gebraucht. Danach steht Docfill im Startmenü; deinstallieren lässt es sich unter
*Einstellungen ▸ Apps* wie jedes andere Programm.

**Windows zeigt eine blaue Warnung.** „Der Computer wurde durch Windows
geschützt" erscheint, weil das Programm kein gekauftes Zertifikat hat — nicht,
weil etwas nicht stimmt. Auf **Weitere Informationen** und dann auf **Trotzdem
ausführen** klicken. Die Meldung kommt nur beim ersten Mal.

**Drucken.** Die Schaltfläche „Drucken" gibt das Dokument an Word weiter, genau
wie ein Rechtsklick ▸ Drucken im Explorer. Wo kein Word installiert ist, wird
LibreOffice benutzt; fehlt auch das, stattdessen „Öffnen" verwenden und von dort
drucken.

**Die Originaldokumente werden nie verändert.** Docfill legt immer neue Dateien
mit der Endung `_ausgefuellt.docx` an.
