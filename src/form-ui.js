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
export function toISO(s) { const d = new Date(s); return isNaN(d) ? "" : d.toISOString().slice(0, 10); }
export function fromISO(s) { const [y, m, d] = s.split("-"); return new Date(y, m - 1, d).toLocaleDateString("de-DE"); }

const TYPE_LABEL = { text: "Text", rich: "Text", date: "Datum", dropdown: "Liste", combo: "Liste", checkbox: "" };

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
    const meta = [TYPE_LABEL[f.type], f.count > 1 ? `${f.count} Stellen` : ""].filter(Boolean).join(" · ");
    let c;
    if (f.type === "checkbox") {
      w.innerHTML = `<label class="check"><input type="checkbox" id="${id}" data-key="${key}" ${/☒|☑/.test(cur) ? "checked" : ""}> ${esc(f.title)}</label>`;
    } else {
      if (f.type === "date") c = `<input type="date" id="${id}" data-key="${key}" value="${toISO(cur)}">`;
      else if (f.type === "dropdown") c = `<select id="${id}" data-key="${key}"><option value="">— auswählen —</option>${f.options.map(o => `<option ${o === cur ? "selected" : ""}>${esc(o)}</option>`).join("")}</select>`;
      else if (f.type === "combo") c = `<input type="text" id="${id}" data-key="${key}" list="${id}l" placeholder="${hint}" value="${esc(cur)}"><datalist id="${id}l">${f.options.map(o => `<option value="${esc(o)}">`).join("")}</datalist>`;
      else if (f.type === "rich") c = `<textarea id="${id}" data-key="${key}" placeholder="${hint}">${esc(cur)}</textarea>`;
      else c = `<input type="text" id="${id}" data-key="${key}" placeholder="${hint}" value="${esc(cur)}">`;
      w.innerHTML = `<label class="t" for="${id}">${esc(f.title)}<span class="meta">${meta}</span></label>${c}`;
    }
    container.appendChild(w);
  });
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
    const v = f.type === "checkbox" ? (el.checked ? "☒" : "☐")
      : f.type === "date" && el.value ? fromISO(el.value)
      : el.value;
    if (v !== "") values[f.key] = v;
  }
  return values;
}
