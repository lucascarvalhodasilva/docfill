// Prüft die Oberfläche, bevor gebaut wird.
//
// Nötig, weil `frontendDist` in tauri.conf.json direkt auf src/ zeigt: der
// Ordner wird unverändert eingepackt, es gibt keinen Bundler-Schritt. Ohne
// diese Prüfung bricht ein Tippfehler in index.html den Build nicht ab — man
// bekommt einen tadellos „erfolgreichen“ Installer mit kaputter Oberfläche.
// Geprüft würde sonst allein die Rust-Seite, vom Compiler.
//
// Was hier auffällt:
//   * Syntaxfehler in den Seiten und in den Modulen
//   * Importe, die ins Leere zeigen (falscher Pfad, Datei fehlt)
//   * benannte Importe, die es im Modul gar nicht gibt
//
//   * mitgelieferte Bibliotheken, die nicht mehr zu src/vendor/SHA256SUMS passen
//
// Was NICHT auffällt: Laufzeitfehler. Ein Feld, das es nicht gibt, oder eine
// Bedingung, die falsch herum steht, findet nur das Ausprobieren. Und die
// Prüfsummen sagen, dass eine Bibliothek unverändert ist — nicht, dass sie
// fehlerfrei ist. Dafür ist der Audit-Schritt im Bauplan da.
//
// Es wird nichts geschrieben (`write: false`) und nichts in src/ abgelegt —
// alles, was dort liegt, landet sonst im Installer.

import { build } from "esbuild";
import { readFile, readdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import path from "node:path";

const SRC = fileURLToPath(new URL("../src/", import.meta.url));

// Die Seiten tragen ihr Skript inline, damit sie ohne Bauschritt auskommen.
// `resolveDir` sorgt dafür, dass ./form-ui.js & Co. trotzdem aufgelöst werden.
const MODULE_SCRIPT = /<script type="module">([\s\S]*?)<\/script>/g;

async function checkPage(file) {
  const html = await readFile(path.join(SRC, file), "utf8");
  const scripts = [...html.matchAll(MODULE_SCRIPT)].map(m => m[1]);
  if (!scripts.length) return [`${file}: kein <script type="module"> gefunden`];

  const problems = [];
  for (const [i, contents] of scripts.entries()) {
    const where = scripts.length > 1 ? `${file} (Skript ${i + 1})` : file;
    try {
      await build({
        stdin: { contents, resolveDir: SRC, sourcefile: file, loader: "js" },
        bundle: true,
        write: false,
        format: "esm",
        logLevel: "silent",
      });
    } catch (e) {
      for (const err of e.errors ?? [{ text: String(e) }]) {
        const at = err.location ? ` (Zeile ${err.location.line})` : "";
        problems.push(`${where}${at}: ${err.text}`);
      }
    }
  }
  return problems;
}

// Die beiden fremden Bibliotheken liegen als Datei im Baum, nicht in
// package.json — kein Installationsschritt prüft sie also. Bisher stand die
// Kontrolle nur als Befehl im README und hing damit am Gedächtnis. Hier läuft
// sie bei jedem Bau mit, weil `beforeBuildCommand` dieses Skript aufruft.
async function checkVendor() {
  const dir = path.join(SRC, "vendor");
  let sums;
  try {
    sums = await readFile(path.join(dir, "SHA256SUMS"), "utf8");
  } catch {
    return ["src/vendor/SHA256SUMS fehlt — die mitgelieferten Bibliotheken sind ungeprüft"];
  }

  const problems = [];
  for (const line of sums.split("\n")) {
    const treffer = line.trim().match(/^([0-9a-f]{64})\s+(\S+)$/);
    if (!treffer) continue;
    const [, erwartet, datei] = treffer;
    try {
      const inhalt = await readFile(path.join(dir, datei));
      const ist = createHash("sha256").update(inhalt).digest("hex");
      if (ist !== erwartet) {
        problems.push(`src/vendor/${datei}: Prüfsumme weicht ab (erwartet ${erwartet.slice(0, 12)}…, ist ${ist.slice(0, 12)}…)`);
      }
    } catch {
      problems.push(`src/vendor/${datei}: steht in SHA256SUMS, fehlt aber`);
    }
  }
  return problems;
}

const pages = (await readdir(SRC)).filter(f => f.endsWith(".html")).sort();
const problems = [
  ...(await Promise.all(pages.map(checkPage))).flat(),
  ...(await checkVendor()),
];

if (problems.length) {
  console.error("Die Oberfläche hat Fehler — es wird nicht gebaut:\n");
  for (const p of problems) console.error("  " + p);
  console.error("");
  process.exit(1);
}
console.log(`Oberfläche geprüft: ${pages.join(", ")} — in Ordnung.`);
console.log("Mitgelieferte Bibliotheken: Prüfsummen stimmen.");
