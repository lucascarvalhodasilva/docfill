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

import { esc, TYPE_LABEL, DATE_FORMATS, DATE_DEFAULT, fromISO } from "./form-ui.js";

/* ---------- die Einstellungen eines Feldtyps ----------
   Listeneinträge, Datumsformat und die beiden Ankreuz-Zeichen. Vorbelegt ist
   immer das, was in der Vorlage steht (`c.optionsDoc`, `c.formatDoc`, …); was
   hier abweicht, wird als Überschreibung gespeichert — und auf Wunsch in die
   Vorlage geschrieben.

   Ein Feldtyp ohne Einstellungen bekommt gar keinen Block: eine leere Fläche
   unter jedem Textfeld ließe das Fenster doppelt so lang werden, ohne etwas
   zu sagen.

   Der Name des Steuerelements steht **nicht** mehr neben dem Haken. Er kam aus
   w:alias oder w:tag und war dort eher verwirrend als hilfreich: „OrderFreefield03"
   sagt niemandem, um welches Feld es geht. Woran man die Zeile erkennt, steht
   jetzt darunter — im Schlüssel und im Platzhaltertext, beides lesbar und beides
   änderbar. Fürs Vorleseprogramm trägt der Haken den Namen weiterhin als
   `aria-label`. */

const BEISPIEL = "2026-03-15";

// Der Platzhaltertext — der graue, den das Feld im Dokument zeigt. Er steht
// bewusst hier und nicht in der Überschrift: in einer echten Vorlage tragen
// oft alle Felder denselben („Click here to enter text."), als Überschrift
// wären dann alle Zeilen gleich. Als eigenes Feld leistet er beides — er zeigt,
// was im Dokument steht, und lässt dort einen eintragen, wo keiner ist.
//
// Gibt es noch keinen, bleibt das Feld leer und der derzeitige Inhalt steht
// grau darin — als Hinweis, nicht als Wert. Stünde er als Wert da, zählte jedes
// Feld ohne Platzhalter beim Schließen als geändert, ohne dass jemand es
// angefasst hätte. Sichtbar muss er trotzdem sein: beim Schreiben wird er
// ersetzt.
function platzhalterHtml(c, id) {
  if (c.type === "picture") return "";   // wird nicht getippt, hat keinen Platzhalter
  const grau = c.current ? `jetzt: ${c.current}` : "Platzhalter eintragen…";
  return `<div class="opt"><label class="lbl" for="${id}p">Platzhalter</label>
    <input type="text" id="${id}p" data-k="hint" maxlength="200"
      value="${esc(c.hint || "")}" placeholder="${esc(c.hint ? "" : grau)}"></div>`;
}

function settingsHtml(c, id) {
  if (c.type === "dropdown" || c.type === "combo") {
    const zeilen = (c.options || []).map((o, k) => eintragHtml(o, k)).join("");
    return `<div class="opt" data-k="options">
      <span class="lbl">Einträge</span>
      <div class="eintraege">${zeilen}</div>
      <button type="button" class="add" data-k="add">Eintrag hinzufügen</button></div>`;
  }
  if (c.type === "date") {
    // Ein Format, das die Vorlage mitbringt und das nicht in der Liste steht,
    // darf nicht verlorengehen — es bekommt einen eigenen Eintrag.
    const eigen = c.format && !DATE_FORMATS.some(f => f.val === c.format)
      ? [{ val: c.format, zeigt: fromISO(BEISPIEL, c.format) }] : [];
    const opts = [...eigen, ...DATE_FORMATS].map(f =>
      `<option value="${esc(f.val)}" ${f.val === (c.format || DATE_DEFAULT) ? "selected" : ""}>${esc(f.zeigt)}</option>`).join("");
    return `<div class="opt"><label class="lbl" for="${id}d">Datumsformat</label>
      <span class="wrap liste"><select id="${id}d" data-k="format">${opts}</select></span></div>`;
  }
  if (c.type === "checkbox") {
    // Ein Zeichen je Feld. `maxlength` steht auf 2, weil manche Zeichen aus
    // zwei UTF-16-Einheiten bestehen und sonst nicht eingegeben werden könnten.
    return `<div class="opt zeichen">
      <label class="lbl" for="${id}a">Angekreuzt</label>
      <input type="text" id="${id}a" data-k="checked" maxlength="2" value="${esc(c.checked || "")}">
      <label class="lbl" for="${id}u">Leer</label>
      <input type="text" id="${id}u" data-k="unchecked" maxlength="2" value="${esc(c.unchecked || "")}">
    </div>`;
  }
  return "";
}

const eintragHtml = (wert, k) => `<span class="eintrag">
  <input type="text" data-k="option" maxlength="200" value="${esc(wert)}"
    aria-label="Eintrag ${k + 1}">
  <button type="button" class="x" data-k="del" title="Eintrag entfernen" aria-label="Eintrag ${k + 1} entfernen"></button></span>`;

// Die Nummer gehört zur Stelle in der Liste, nicht zur Zeile. Ohne dieses
// Nachzählen hat, wer die zweite von vier Zeilen löscht, hinterher „Eintrag 1,
// 3, 4" — und die nächste neue hieße wieder „Eintrag 4", weil sie ihre Nummer
// aus der Anzahl der Zeilen bekommt. Zwei Felder mit demselben Namen sind für
// ein Vorleseprogramm nicht mehr auseinanderzuhalten.
function nummerieren(liste) {
  Array.from(liste.children).forEach((z, k) => {
    z.querySelector("[data-k=option]").setAttribute("aria-label", `Eintrag ${k + 1}`);
    z.querySelector("[data-k=del]").setAttribute("aria-label", `Eintrag ${k + 1} entfernen`);
  });
}

/** Klicks auf „hinzufügen" und „entfernen" — eine Behandlung für die ganze Liste. */
function bindSettings(row) {
  const block = row.querySelector("[data-k=options]");
  if (!block) return;
  block.addEventListener("click", e => {
    const add = e.target.closest("[data-k=add]"), del = e.target.closest("[data-k=del]");
    if (!add && !del) return;
    e.preventDefault();
    const liste = block.querySelector(".eintraege");
    if (add) {
      liste.insertAdjacentHTML("beforeend", eintragHtml("", liste.children.length));
      liste.lastElementChild.querySelector("input").focus();
    } else {
      del.closest(".eintrag").remove();
    }
    nummerieren(liste);
    block.dataset.edited = "";
  });
  block.addEventListener("input", () => { block.dataset.edited = ""; });
}

/**
 * Zeichnet die Steuerelemente in `container`.
 *
 * Im Eingabefeld steht immer ein Schlüssel: der überschriebene, sonst der, den
 * das Dokument von sich aus hergibt (`c.auto`). Ein leeres Feld mit Erklärung
 * daneben sah aus, als hätte das Steuerelement gar keinen — dabei ist gerade
 * der automatische Schlüssel das, was man vergleichen will.
 *
 * @param controls [{ orig, auto, key, hidden, title, type, shared,
 *                    hint, hintDoc, current, options, optionsDoc, format, formatDoc,
 *                    checked, checkedDoc, unchecked, uncheckedDoc }]
 *                 Die `…Doc`-Felder sind der Stand der Vorlage, die anderen die
 *                 Vorbelegung im Fenster (Vorlage plus bisherige Einstellung).
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
    // Links der Feldtyp, rechts der Haken mit seiner Beschriftung. Links stünde
    // „Im Formular" an der Stelle, an der man den Namen des Steuerelements
    // erwartet — und läse sich prompt wie einer.
    row.innerHTML = `<span class="meta">${esc(meta)}</span>
      <label class="check"><span>Im Formular</span><input type="checkbox" data-k="on" ${c.hidden ? "" : "checked"}
        aria-label="${esc(c.title)} im Formular zeigen"></label>
      <label class="k" for="${id}">Schlüssel</label>
      <input type="text" id="${id}" data-k="key" list="knownKeys" maxlength="120"
        value="${esc(c.key || c.auto)}" placeholder="ohne festen Schlüssel">
      ${platzhalterHtml(c, id)}${settingsHtml(c, id)}`;
    // Wer die Vorbelegung stehen lässt, ändert nichts. Wer denselben Text noch
    // einmal tippt, meint es aber ernst: er legt das Feld auf diesen Schlüssel
    // fest und damit mit einem gleichnamigen aus einem anderen Dokument zusammen.
    row.querySelector("[data-k=key]").addEventListener("input", e => { e.target.dataset.edited = ""; });
    bindSettings(row);
    container.appendChild(row);
  });
}

/**
 * Liest die Auswahl aus `container`.
 * @returns `{ [orig]: { key?, hidden?, hint?, options?, format?, checked?, unchecked? } }`
 *          — Einträge, die nichts ändern, fehlen darin. Sonst wüchse die Karte
 *          mit jedem Öffnen des Fensters, und ein Schlüssel, der ohnehin im
 *          Dokument steht, stünde doppelt.
 *
 * Verglichen wird immer mit dem Stand der **Vorlage** (`hintDoc`, `optionsDoc`,
 * `formatDoc`, `checkedDoc`), nicht mit der Vorbelegung: wer eine frühere
 * Änderung zurückstellt, soll sie damit auch loswerden.
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

    // Der Platzhaltertext gilt für jeden Feldtyp, deshalb steht er vor der
    // Fallunterscheidung. Verglichen wird mit dem Stand der Vorlage: wer eine
    // frühere Änderung zurückstellt, soll sie damit auch loswerden.
    const hintFeld = row.querySelector("[data-k=hint]");
    if (hintFeld) {
      const h = hintFeld.value.trim();
      if (h && h !== (c.hintDoc || "")) o.hint = h;
    }

    if (c.type === "dropdown" || c.type === "combo") {
      const eintraege = [...row.querySelectorAll("[data-k=option]")]
        .map(e => e.value.trim()).filter(Boolean);
      const wie_vorlage = eintraege.length === (c.optionsDoc || []).length
        && eintraege.every((e, i) => e === c.optionsDoc[i]);
      // Eine ganz geleerte Liste wäre eine Auswahl ohne Auswahl — dann bleibt
      // es bei dem, was die Vorlage anbietet.
      if (eintraege.length && !wie_vorlage) o.options = eintraege;
    } else if (c.type === "date") {
      const f = row.querySelector("[data-k=format]").value;
      if (f && f !== (c.formatDoc || DATE_DEFAULT)) o.format = f;
    } else if (c.type === "checkbox") {
      const z = n => row.querySelector(`[data-k=${n}]`).value.trim();
      const an = z("checked"), aus = z("unchecked");
      if (an && an !== (c.checkedDoc || "☒")) o.checked = an;
      if (aus && aus !== (c.uncheckedDoc || "☐")) o.unchecked = aus;
    }
    // Nicht aufschreiben, was ohnehin gilt: ein Schlüssel, der schon als w:tag
    // im Dokument steht, und eine unangetastete Vorbelegung. Vorbelegt ist aber
    // nur, was aus dem Dokument stammt — eine schon vergebene Überschreibung
    // (`c.key`) bleibt beim erneuten Öffnen stehen, auch wenn sie zufällig
    // genauso heißt wie der automatische Schlüssel.
    const untouched = !c.key && key === c.auto && field.dataset.edited === undefined;
    if (key && c.orig !== "tag:" + key && !untouched) o.key = key;
    if (!row.querySelector("[data-k=on]").checked) o.hidden = true;
    if (Object.keys(o).length) out[c.orig] = o;
  }
  return out;
}

/**
 * Beschriftung des Schalters in der Fußzeile. Sie muss sagen, wohin geschrieben
 * wird: über den Öffnen-Dialog geladene Dateien werden ohne Nachfrage
 * überschrieben, alle anderen kennen keinen Pfad und werden zur Kopie.
 */
export const writeTagsLabel = onDisk =>
  "Einstellungen dauerhaft ins Dokument schreiben (Schlüssel, Listeneinträge, Datumsformat, Ankreuz-Zeichen) — "
  + (onDisk ? "die Datei wird überschrieben" : "als Kopie zum Speichern");

/** „3 Schlüssel geändert · 1 Feld abgewählt“ — für die Statuszeile. */
export function overrideMeta(overrides) {
  const all = Object.values(overrides || {});
  const zaehl = f => all.filter(f).length;
  const keys = zaehl(o => o.key), off = zaehl(o => o.hidden);
  const listen = zaehl(o => o.options), daten = zaehl(o => o.format);
  const haken = zaehl(o => o.checked || o.unchecked);
  const platz = zaehl(o => o.hint);
  const parts = [];
  if (keys) parts.push(`${keys} Schlüssel geändert`);
  if (platz) parts.push(`${platz} Platzhalter geändert`);
  if (listen) parts.push(`${listen} Liste${listen === 1 ? "" : "n"} geändert`);
  if (daten) parts.push(`${daten}× Datumsformat geändert`);
  if (haken) parts.push(`${haken}× Ankreuz-Zeichen geändert`);
  if (off) parts.push(`${off} Feld${off === 1 ? "" : "er"} abgewählt`);
  return parts.join(" · ");
}
