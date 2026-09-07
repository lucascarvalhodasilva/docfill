// Darstellung der Sitzungs-Historie, benutzt vom Historie-Fenster (history.html)
// und — wenn die Oberfläche ohne Tauri im Browser läuft — vom Ersatzdialog in
// index.html. Deshalb liegt sie hier und nicht in einer Seite.
//
// Die Historie ist nach Ausfüll-Vorgang gegliedert: eine Überschrift je Lauf
// (Uhrzeit und Formularname), darunter die entstandenen Dokumente.
//
// Wichtig: hier stehen nur Metadaten. Die Dokumente selbst bleiben als Blob im
// Hauptfenster, das Historie-Fenster bekommt sie nie zu sehen — es schickt für
// Öffnen, Speichern und Drucken lediglich einen Verweis zurück.

import { esc } from "./form-ui.js";

export function size(b) {
  return b < 1024 ? b + " B" : b < 1048576 ? Math.round(b / 1024) + " KB" : (b / 1048576).toFixed(1) + " MB";
}

// Uhrzeit genügt für den Regelfall. Läuft das Programm über Mitternacht, käme
// sonst ein zweites „09:12“ ohne Unterscheidung — dann steht das Datum davor.
function stamp(at) {
  const d = new Date(at);
  const time = d.toLocaleTimeString("de-DE", { hour: "2-digit", minute: "2-digit" });
  return d.toDateString() === new Date().toDateString()
    ? time
    : `${d.toLocaleDateString("de-DE", { day: "2-digit", month: "2-digit" })} ${time}`;
}

/**
 * Zeichnet die Läufe in `box`, den neuesten zuoberst.
 * @param runs   [{ id, at, group, docs: [{ name, size }] }] — nur Metadaten
 * @param onMenu (schaltfläche, laufId, dokumentIndex) => void, für die Aktionen
 */
export function renderHistory(box, runs, onMenu) {
  box.innerHTML = "";
  box.className = "history";
  if (!runs.length) {
    box.innerHTML = `<div class="empty"><b>Noch nichts ausgefüllt</b>Was in dieser Sitzung ausgefüllt wird,
      sammelt sich hier — bis das Programm beendet wird.</div>`;
    return;
  }
  for (const run of [...runs].reverse()) {
    const sec = document.createElement("section");
    sec.className = "run";
    const n = run.docs.length;
    sec.innerHTML = `<h3><span class="at">${esc(stamp(run.at))}</span><span class="who">${esc(run.group)}</span>
      <span class="meta">${n} Dokument${n === 1 ? "" : "e"}</span></h3>`;
    run.docs.forEach((d, di) => {
      const row = document.createElement("div");
      row.className = "result";
      row.innerHTML = `<span class="name" title="${esc(d.name)}">${esc(d.name)}</span>
        <span class="size">${esc(size(d.size))}</span>
        <button class="menu-btn" data-run="${esc(run.id)}" data-doc="${di}"
          aria-haspopup="menu" aria-expanded="false"
          aria-label="Aktionen für ${esc(d.name)}">Aktionen<span class="caret" aria-hidden="true">▾</span></button>`;
      const mb = row.querySelector(".menu-btn");
      mb.onclick = () => onMenu(mb, run.id, di);
      sec.appendChild(row);
    });
    box.appendChild(sec);
  }
}
