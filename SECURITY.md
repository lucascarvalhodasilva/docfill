# Sicherheit

## Etwas gefunden?

Bitte **kein öffentliches Issue** aufmachen. Der Weg führt über
**Security ▸ Report a vulnerability** hier im Repo (private
Sicherheitsmeldung). Sie ist nur für mich sichtbar, und die Antwort steht an
derselben Stelle.

Hilfreich sind: was passiert, wie es sich nachstellen lässt, und welche Fassung
betroffen ist — die steht unten rechts im Fenster.

Docfill ist ein Werkzeug einer einzelnen Person, keine Firma mit Bereitschaft.
Rechne mit einer Antwort innerhalb einer Woche.

## Was Docfill schützt — und was nicht

Das Bedrohungsmodell steht ausführlich in der README, im Abschnitt
[**Sicherheitshinweise**](README.md#sicherheitshinweise). Es wird hier nicht
wiederholt, damit es nicht an zwei Stellen auseinanderläuft. Die Kurzfassung:

- Die Oberfläche hat **keinen** Zugriff auf Dateisystem, Dialoge oder Shell —
  `src-tauri/capabilities/default.json` vergibt nur `core:default`. Wohin
  gespeichert wird, entscheidet immer ein nativer Dialog in Rust.
- Ausgefüllte Dokumente sind Kopien; die Vorlage bleibt unangetastet. Die
  einzige Ausnahme ist der ausdrücklich benannte Schalter „Schlüssel dauerhaft
  ins Dokument schreiben".
- Was auf der Platte landet — der Ablageordner fürs Öffnen und Drucken sowie die
  gemerkten Formulare — gehört dem eigenen Benutzer allein (`0700`/`0600`).
  Die Historie liegt nur im Arbeitsspeicher.
- Beim Drucken über Word laufen **keine Makros**
  (`AutomationSecurity = ForceDisable`).

Bewusst akzeptierte Restrisiken, ausführlich begründet in der README:

- **Die Unterschrift vom Tablet läuft über unverschlüsseltes HTTP** im lokalen
  Netz. Über den Draht gehen nur der Dateiname und das Unterschriftsbild, nie
  das Dokument. Der Server läuft nur, solange der Dialog offen ist.
- **Die Installationsdateien sind nicht signiert.** Ein Zertifikat ist nicht
  gekauft, deshalb warnt Windows beim Start. Zum Gegenprüfen liegt jedem Release
  eine `SHA256SUMS.txt` bei — wie man sie vergleicht, steht in
  [BUILD-WINDOWS.md](BUILD-WINDOWS.md).
- **Es gibt keinen automatischen Aktualisierungsdienst.** Eine Korrektur
  erreicht einen Rechner erst, wenn jemand den Installer neu aus dem Release
  holt. Wer Docfill weitergibt, sollte gelegentlich nachsehen.

## Unterstützte Fassungen

Korrigiert wird jeweils in der neuesten Fassung. Ältere bekommen keine
Rückportierung — es gibt nur einen Verteilweg, und der führt über ein neues
Release.
