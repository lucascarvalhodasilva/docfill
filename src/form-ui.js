// Die Felddarstellung wird von beiden Fenstern benutzt: vom Formularfenster
// (form.html) und — wenn die Oberfläche ohne Tauri im Browser läuft — vom
// Ersatzdialog in index.html. Deshalb liegt sie hier und nicht in einer Seite.
//
// Werte werden über `data-key` an das Feld gebunden, nicht über die Position in
// der Liste: `f.key` ist eindeutig, eine Umsortierung kann die Werte also nicht
// mehr auf die falschen Felder schieben.

export function esc(s) {
  return String(s ?? "").replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}
/* ---------- Datum ----------
   Im Eingabefeld steht ein Datum immer als JJJJ-MM-TT; ins Dokument geht es in
   dem Format, das die Vorlage im Feld angibt (w:dateFormat) oder das im
   Zahnrad-Fenster eingestellt wurde. */

// Die gängigen Formate für die Auswahlliste im Zahnrad-Fenster. Words Kürzel,
// damit das Eingestellte auch dann noch stimmt, wenn es in die Vorlage
// geschrieben wird.
export const DATE_FORMATS = [
  { val: "dd.MM.yyyy",    zeigt: "15.03.2026" },
  { val: "d.M.yyyy",      zeigt: "15.3.2026" },
  { val: "dd.MM.yy",      zeigt: "15.03.26" },
  { val: "yyyy-MM-dd",    zeigt: "2026-03-15" },
  { val: "d. MMMM yyyy",  zeigt: "15. März 2026" },
  { val: "MMMM yyyy",     zeigt: "März 2026" },
];
export const DATE_DEFAULT = "dd.MM.yyyy";

const MONATE = ["Januar", "Februar", "März", "April", "Mai", "Juni",
                "Juli", "August", "September", "Oktober", "November", "Dezember"];

/** Ein getipptes oder im Dokument stehendes Datum nach JJJJ-MM-TT. */
export function toISO(s) {
  const t = String(s ?? "").trim();
  if (/^\d{4}-\d{2}-\d{2}$/.test(t)) return t;
  // 15.03.2026, 15.3.2026, 15/03/2026 — `new Date` liest das im Deutschen
  // falsch oder gar nicht und lieferte bisher ein leeres Feld.
  const m = /^(\d{1,2})[.\/](\d{1,2})[.\/](\d{4})$/.exec(t);
  const zwei = n => String(n).padStart(2, "0");
  if (m) return `${m[3]}-${zwei(m[2])}-${zwei(m[1])}`;
  const d = new Date(t);
  return isNaN(d) ? "" : d.toISOString().slice(0, 10);
}

/** JJJJ-MM-TT in das Format der Vorlage. */
export function fromISO(s, format) {
  const [y, m, d] = String(s ?? "").split("-").map(Number);
  if (!y || !m || !d) return "";
  const zwei = n => String(n).padStart(2, "0");
  // In einem Durchgang ersetzen, das längste Kürzel zuerst: nacheinander würde
  // `M` den soeben eingesetzten Monatsnamen gleich wieder zerlegen.
  return (format || DATE_DEFAULT).replace(/yyyy|yy|MMMM|MM|M|dd|d/g, t => ({
    yyyy: String(y), yy: zwei(y % 100),
    MMMM: MONATE[m - 1], MM: zwei(m), M: String(m),
    dd: zwei(d), d: String(d),
  })[t]);
}

// Auch von keys-ui.js benutzt: die Zahnrad-Liste nennt denselben Feldtyp
// wie das Formular, sonst hieße dasselbe Steuerelement zweimal verschieden.
//
// Die Namen unterscheiden, was Word unterscheidet: eine Auswahlliste lässt nur
// die vorgegebenen Einträge zu, ein Kombinationsfeld auch eigene. Wer das nicht
// weiß, tippt in ein Feld, das gar nichts annimmt.
//
// Jeder Typ hat einen Namen, auch das Ankreuzfeld. Im Formular sieht man ihm
// zwar an, was es ist — im Zahnrad-Fenster ist der Typ aber die einzige Angabe
// dazu, und eine zugeklappte Zeile stünde sonst ohne da.
export const TYPE_LABEL = {
  text: "Text", multi: "Text, mehrzeilig", rich: "Rich Text", date: "Datum",
  dropdown: "Auswahlliste", combo: "Liste, auch frei", checkbox: "Ankreuzfeld",
  picture: "Unterschrift",
};

/**
 * In wie vielen Dokumenten der Gruppe das Feld steht. Beim Ausfüllen ist das
 * die eigentliche Frage: schreibt dieser Wert in alle Kopien oder nur in eine?
 * Bei einem einzigen Dokument sagt die Zeile nichts und bleibt weg — ebenso bei
 * Formularen, die vor dieser Angabe gespeichert wurden.
 */
function docsText(f) {
  if (!f.ofDocs || f.ofDocs < 2 || !f.docs) return "";
  return f.docs >= f.ofDocs ? `in allen ${f.ofDocs} Dokumenten` : `in ${f.docs} von ${f.ofDocs} Dokumenten`;
}

/** Bereits eingegebener Wert, sonst der Wert aus dem Dokument. */
function initial(f, values) {
  const v = values && Object.prototype.hasOwnProperty.call(values, f.key) ? values[f.key] : undefined;
  return v === undefined ? f.current : v;
}

/**
 * Zeichnet die Felder in `container`.
 * @param values Objekt `{ [key]: wert }` mit bereits eingegebenen Werten (optional).
 */
export function renderFields(container, fields, values) {
  container.innerHTML = "";
  fields.forEach((f, i) => {
    const id = "f" + i, key = esc(f.key), cur = initial(f, values);
    // Words Platzhaltertext: nur als grauer Hinweis, nie als Wert
    const hint = esc(f.hint || "");
    const w = document.createElement("div");
    w.className = "field";
    const meta = [TYPE_LABEL[f.type], f.count > 1 ? `${f.count} Stellen` : "", docsText(f)].filter(Boolean).join(" · ");
    let c;
    if (f.type === "checkbox") {
      // Angekreuzt ist, was die Vorlage dafür hält: sie bestimmt das Zeichen,
      // nicht wir. ☑ bleibt als zweite Möglichkeit stehen, weil manche Vorlage
      // es benutzt, ohne es im w14:checkedState zu nennen.
      const an = f.checked || "☒";
      w.innerHTML = `<label class="check"><input type="checkbox" id="${id}" data-key="${key}" ${cur.includes(an) || /☒|☑/.test(cur) ? "checked" : ""}> ${esc(f.title)}${meta ? `<span class="meta">${esc(meta)}</span>` : ""}</label>`;
    } else {
      // Jeder Feldtyp bekommt seine eigene Gestalt. Ein Umschlag mit einer
      // Klasse trägt das Zeichen rechts im Feld (siehe app.css): der Pfeil sagt
      // „hier klappt etwas auf", ohne dass ein Bild nachgeladen werden müsste.
      if (f.type === "date")
        c = `<span class="wrap datum"><input type="date" id="${id}" data-key="${key}" value="${toISO(cur)}"></span>`;
      // Bringt die Vorlage selbst eine „nichts gewählt"-Zeile mit (ein
      // Listeneintrag ohne w:value), dann ist sie der Platzhalter — und steht
      // nicht ein zweites Mal zwischen den Antworten.
      else if (f.type === "dropdown") {
        const antworten = f.options.filter(o => o !== f.optionLeer);
        c = `<span class="wrap liste"><select id="${id}" data-key="${key}"><option value="">${esc(f.optionLeer || "— auswählen —")}</option>${antworten.map(o => `<option ${o === cur ? "selected" : ""}>${esc(o)}</option>`).join("")}</select></span>`;
      }
      else if (f.type === "combo") {
        // Im Kombinationsfeld ist der Platzhalter kein Vorschlag, sondern der
        // graue Text im leeren Feld — Words eigener Hinweis geht vor.
        const antworten = f.options.filter(o => o !== f.optionLeer);
        const grau = hint || esc(f.optionLeer) || "wählen oder eintippen";
        c = `<span class="wrap liste frei"><input type="text" id="${id}" data-key="${key}" list="${id}l" placeholder="${grau}" value="${esc(cur)}"><datalist id="${id}l">${antworten.map(o => `<option value="${esc(o)}">`).join("")}</datalist></span>`;
      }
      else if (f.type === "rich" || f.type === "multi")
        c = `<textarea id="${id}" data-key="${key}" placeholder="${hint}">${esc(cur)}</textarea>`;
      else c = `<input type="text" id="${id}" data-key="${key}" placeholder="${hint}" value="${esc(cur)}">`;
      w.innerHTML = `<label class="t" for="${id}">${esc(f.title)}<span class="meta">${meta}</span></label>${c}`;
    }
    container.appendChild(w);
  });
}

/**
 * Wie viele Felder noch leer sind. Beide Wege ins Ausfüllen — das
 * Formularfenster und der Ersatzdialog — fragen hier, damit sie dasselbe zählen.
 *
 * Leer heißt: `collectValues` hat für den Schlüssel nichts geliefert, im
 * Dokument bleibt dort also stehen, was die Vorlage vorgibt. Ein Ankreuzfeld
 * liefert immer ☒ oder ☐ und ist deshalb nie leer; eine Auswahlliste auf ihrer
 * „nichts gewählt"-Zeile liefert nichts und zählt mit. Beides ergibt sich von
 * selbst aus `collectValues`, ohne Sonderfall.
 */
export function countEmpty(container, fields) {
  const werte = collectValues(container, fields);
  const alle = fields || [];
  return { leer: alle.filter(f => !(f.key in werte)).length, gesamt: alle.length };
}

/** „3 von 8 Feldern noch leer“ — leer, solange nichts fehlt. */
export function emptyText(container, fields) {
  const { leer, gesamt } = countEmpty(container, fields);
  return leer ? `${leer} von ${gesamt} Feld${gesamt === 1 ? "" : "ern"} noch leer` : "";
}

/**
 * Liest die eingegebenen Werte aus `container`.
 * @returns Objekt `{ [key]: wert }`; leere Felder fehlen darin, damit der
 *          ursprüngliche Inhalt des Dokuments an dieser Stelle stehen bleibt.
 */
export function collectValues(container, fields) {
  const byKey = new Map(fields.map(f => [f.key, f]));
  const values = {};
  for (const el of container.querySelectorAll("[data-key]")) {
    const f = byKey.get(el.dataset.key);
    if (!f) continue;
    // Zeichen und Datumsformat kommen aus der Vorlage bzw. aus dem
    // Zahnrad-Fenster — hier steht nichts Festes mehr.
    const v = f.type === "checkbox" ? (el.checked ? (f.checked || "☒") : (f.unchecked || "☐"))
      : f.type === "date" && el.value ? fromISO(el.value, f.format)
      : el.value;
    if (v !== "") values[f.key] = v;
  }
  return values;
}
