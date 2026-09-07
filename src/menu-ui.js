// Aufklappmenüs, benutzt vom Hauptfenster (Kachel-Kontextmenü und Aktionen der
// Ergebniszeile) und vom Historie-Fenster. Deshalb liegt die Mechanik hier und
// nicht in einer Seite: sonst müsste jede Seite Positionierung, Pfeiltasten und
// das Schließen erneut hinschreiben — und die drei Fassungen liefen auseinander.
//
// Das Menü-Element selbst bleibt in der jeweiligen Seite, weil es dort direkt am
// <body> hängen muss: die Panels scrollen, innerhalb würde es abgeschnitten.
// Pro Seite ist immer höchstens ein Menü offen.

let cur = null;   // { el, arg, trigger } des offenen Menüs

/**
 * Verdrahtet ein Menü-Element.
 * @param el      das <div class="menu"> der Seite
 * @param onPick  (aktion, arg) => void — `aktion` ist das data-a des Eintrags
 */
export function defineMenu(el, onPick) {
  el.onclick = e => {
    const b = e.target.closest("button[data-a]");
    if (!b || b.disabled) return;
    const arg = cur && cur.arg;
    closeMenu(false);
    onPick(b.dataset.a, arg);
  };
  return { el };
}

/**
 * Klappt `m` auf. x/y ist die gewünschte obere linke Ecke, mit `right` die obere
 * rechte. `arg` geht unverändert an onPick zurück. `trigger` liefert die
 * auslösende Schaltfläche jedes Mal neu, weil die Listen zwischendurch neu
 * gezeichnet werden und eine gemerkte Referenz dann ins Leere zeigt.
 */
export function openMenu(m, x, y, { arg = null, trigger = null, right = false } = {}) {
  closeMenu(false);
  cur = { el: m.el, arg, trigger };
  m.el.hidden = false;
  // erst einblenden, dann messen und am Bildschirmrand umklappen
  const w = m.el.offsetWidth, h = m.el.offsetHeight;
  if (right) x -= w;
  m.el.style.left = Math.max(4, Math.min(x, innerWidth - w - 4)) + "px";
  m.el.style.top = Math.max(4, Math.min(y, innerHeight - h - 4)) + "px";
  markTrigger(true);
  const first = m.el.querySelector("button:not(:disabled)");
  if (first) first.focus();
}

export function closeMenu(toTrigger) {
  if (!cur) return;
  const t = markTrigger(false);
  cur.el.hidden = true;
  cur = null;
  if (toTrigger && t) t.focus();
}

/** Steht das Menü gerade für genau dieses `arg` offen? Für Umschalt-Klicks. */
export function menuOpenFor(arg) { return !!cur && cur.arg === arg; }

// Umgestellt wird nur, was aria-expanded schon mitbringt: die Kacheln öffnen ein
// Kontextmenü und sollen die Rolle einer Aufklapp-Schaltfläche nicht bekommen.
function markTrigger(on) {
  const t = cur && cur.trigger && cur.trigger();
  if (t && t.hasAttribute("aria-expanded")) t.setAttribute("aria-expanded", String(on));
  return t;
}

addEventListener("keydown", e => {
  if (!cur) return;
  if (e.key === "Escape") { e.preventDefault(); closeMenu(true); return; }
  if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
  e.preventDefault();
  const items = [...cur.el.querySelectorAll("button:not(:disabled)")];
  const i = items.indexOf(document.activeElement);
  items[e.key === "ArrowDown" ? (i + 1) % items.length : (i - 1 + items.length) % items.length].focus();
});
// Der Klick auf die eigene Schaltfläche ist ausgenommen: sonst schlösse das
// mousedown das Menü und der darauf folgende Klick risse es sofort wieder auf.
addEventListener("mousedown", e => {
  if (!cur || cur.el.contains(e.target)) return;
  const t = cur.trigger && cur.trigger();
  if (t && t.contains(e.target)) return;
  closeMenu(false);
});
addEventListener("contextmenu", e => { if (cur && !cur.el.contains(e.target)) closeMenu(false); });
addEventListener("blur", () => closeMenu(false));
addEventListener("resize", () => closeMenu(false));
