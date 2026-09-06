// Einstiegspunkt für src/vendor/tauri-api.js — die Oberfläche braucht `invoke`
// und `listen`: `listen` nur im Hauptfenster, um zu erfahren, wann das
// Formularfenster seine Werte abgeschickt hat.
// Datei-Dialoge, Schreibzugriffe und „Öffnen mit“ laufen vollständig in Rust.
// Neu bauen mit dem Befehl im README (Abschnitt „Mitgelieferte Bibliotheken“).
export { invoke } from "@tauri-apps/api/core";
export { listen } from "@tauri-apps/api/event";
