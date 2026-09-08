// Die Steuerelement-Liste eines einzelnen Dokuments: Haken fürs Formular und
// der Schlüssel, unter dem das Feld über Dokumente hinweg wiedererkannt wird.
// Benutzt vom Zahnrad-Fenster (keys.html) und — wenn die Oberfläche ohne Tauri
// im Browser läuft — vom Ersatzdialog in index.html. Deshalb liegt sie hier und
// nicht in einer Seite.
//
// Ein Schlüssel ist immer ein w:tag-Wert: `sdtKey` in index.html gibt einem
// echten Tag den Schlüssel „tag:X“, eine Überschreibung mit demselben X ergibt
// also denselben Schlüssel. Wer hier „Name“ einträgt, verschmilzt das Feld mit
// jedem Dokument, das dieses Tag wirklich trägt — und wer den Schlüssel später
// ins Dokument schreiben lässt, ändert am Ergebnis nichts.
//
// Gebunden wird über `data-orig`, den Schlüssel ohne Überschreibung: er ist das
// einzige Merkmal, das das Hauptfenster beim nächsten Lesen wieder berechnet.

import { esc, TYPE_LABEL } from "./form-ui.js";

/**
 * Zeichnet die Steuerelemente in `container`.
 *
 * Im Eingabefeld steht immer ein Schlüssel: der überschriebene, sonst der, den
 * das Dokument von sich aus hergibt (`c.auto`). Ein leeres Feld mit Erklärung
 * daneben sah aus, als hätte das Steuerelement gar keinen — dabei ist gerade
 * der automatische Schlüssel das, was man vergleichen will.
 *
 * @param controls [{ orig, auto, key, hidden, title, type, shared }]
 * @param known    Schlüssel, die in den anderen Dokumenten schon vorkommen —
 *                 als Vorschlagsliste, damit man sie nicht abtippen muss.
 */
export function renderControls(container, controls, known) {
  const list = (known || []).map(k => `<option value="${esc(k)}">`).join("");
  container.innerHTML = `<datalist id="knownKeys">${list}</datalist>`;
  controls.forEach((c, i) => {
    const id = "c" + i;
    const meta = [TYPE_LABEL[c.type], c.shared ? `in ${c.shared + 1} Dokumenten` : ""].filter(Boolean).join(" · ");
    const row = document.createElement("div");
    row.className = "ctrl";
    row.dataset.orig = c.orig;
    row.innerHTML = `<label class="check"><input type="checkbox" data-k="on" ${c.hidden ? "" : "checked"}
        aria-label="${esc(c.title)} im Formular zeigen"><span class="t">${esc(c.title)}</span></label>
      <span class="meta">${esc(meta)}</span>
      <label class="k" for="${id}">Schlüssel</label>
      <input type="text" id="${id}" data-k="key" list="knownKeys" maxlength="120"
        value="${esc(c.key || c.auto)}" placeholder="ohne festen Schlüssel">`;
    // Wer die Vorbelegung stehen lässt, ändert nichts. Wer denselben Text noch
    // einmal tippt, meint es aber ernst: er legt das Feld auf diesen Schlüssel
    // fest und damit mit einem gleichnamigen aus einem anderen Dokument zusammen.
    row.querySelector("[data-k=key]").addEventListener("input", e => { e.target.dataset.edited = ""; });
    container.appendChild(row);
  });
}

/**
 * Liest die Auswahl aus `container`.
 * @returns `{ [orig]: { key?, hidden? } }` — Einträge, die nichts ändern, fehlen
 *          darin. Sonst wüchse die Karte mit jedem Öffnen des Fensters, und ein
 *          Schlüssel, der ohnehin im Dokument steht, stünde doppelt.
 */
export function collectOverrides(container, controls) {
  const byOrig = new Map(controls.map(c => [c.orig, c]));
  const out = {};
  for (const row of container.querySelectorAll(".ctrl[data-orig]")) {
    const c = byOrig.get(row.dataset.orig);
    if (!c) continue;
    const field = row.querySelector("[data-k=key]");
    const key = field.value.trim();
    const o = {};
    // Nicht aufschreiben, was ohnehin gilt: ein Schlüssel, der schon als w:tag
    // im Dokument steht, und eine unangetastete Vorbelegung. Vorbelegt ist aber
    // nur, was aus dem Dokument stammt — eine schon vergebene Überschreibung
    // (`c.key`) bleibt beim erneuten Öffnen stehen, auch wenn sie zufällig
    // genauso heißt wie der automatische Schlüssel.
    const untouched = !c.key && key === c.auto && field.dataset.edited === undefined;
    if (key && c.orig !== "tag:" + key && !untouched) o.key = key;
    if (!row.querySelector("[data-k=on]").checked) o.hidden = true;
    if (o.key || o.hidden) out[c.orig] = o;
  }
  return out;
}

/**
 * Beschriftung des Schalters in der Fußzeile. Sie muss sagen, wohin geschrieben
 * wird: über den Öffnen-Dialog geladene Dateien werden ohne Nachfrage
 * überschrieben, alle anderen kennen keinen Pfad und werden zur Kopie.
 */
export const writeTagsLabel = onDisk =>
  "Schlüssel dauerhaft ins Dokument schreiben (w:tag) — "
  + (onDisk ? "die Datei wird überschrieben" : "als Kopie zum Speichern");

/** „3 Schlüssel geändert · 1 Feld abgewählt“ — für die Statuszeile. */
export function overrideMeta(overrides) {
  const all = Object.values(overrides || {});
  const keys = all.filter(o => o.key).length, off = all.filter(o => o.hidden).length;
  const parts = [];
  if (keys) parts.push(`${keys} Schlüssel geändert`);
  if (off) parts.push(`${off} Feld${off === 1 ? "" : "er"} abgewählt`);
  return parts.join(" · ");
}
