use std::fs::{self, DirBuilder, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

/// Folder the user picked for a "save all" run. Held here rather than handed
/// back to the webview, so page script still cannot name a destination.
#[derive(Default)]
struct SaveTarget(Mutex<Option<PathBuf>>);

/// Field descriptions and entered values for the form window, as opaque JSON.
/// The two webviews cannot see each other's memory, so the payload passes
/// through here. Rust never looks inside it.
///
/// `shown` holds the window label of the group currently on display. A form
/// window reads its payload exactly once, when it loads, so a different group
/// needs a new window rather than a focus call.
#[derive(Default)]
struct FormBridge {
    payload: Mutex<Option<String>>,
    shown: Mutex<Option<String>>,
}

/// A group as it sits on disk: the folder name and the meta JSON, handed back
/// unread. Rust stores the frontend's JSON, it does not interpret it.
#[derive(serde::Serialize)]
struct StoredGroup {
    id: String,
    meta: String,
}

const MAX_DOC: usize = 25 * 1024 * 1024; // wie MAX_FILE in der Oberfläche
const MAX_GROUP: usize = 200 * 1024 * 1024;

fn groups_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|_| "Der Datenordner wurde nicht gefunden.".to_string())?;
    Ok(base.join("groups"))
}

/// Only a plain UUID may become a folder name. `safe()` is not enough here: it
/// lets dots through, so ".." would survive and could climb out of the folder.
fn group_dir(app: &tauri::AppHandle, id: &str) -> Result<PathBuf, String> {
    let plausible = id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    if !plausible {
        return Err("Ungültiger Pfad".into());
    }
    Ok(groups_dir(app)?.join(id))
}

fn dir_size(dir: &Path) -> usize {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len() as usize)
                .sum()
        })
        .unwrap_or(0)
}

/// OS error texts leak absolute paths into the UI, and are English on a German
/// interface. Callers get a fixed message; the detail goes to the log.
fn io_error(context: &str, e: std::io::Error) -> String {
    eprintln!("docfill: {context}: {e}");
    match context {
        "write" => "Die Datei konnte nicht geschrieben werden.".into(),
        _ => "Der Vorgang ist fehlgeschlagen.".into(),
    }
}

/// Per-run staging folder. The name is unpredictable so that nobody can
/// pre-place a symlink at a path we are about to write to, which on a
/// world-writable /tmp would turn a staged document into an arbitrary
/// file overwrite. Removed again when the app exits.
fn staging_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("docfill-{}-{:08x}", std::process::id(), nonce))
    })
    .clone()
}

/// Only a regular file directly inside our own staging folder may be handed to
/// a printer, a converter or the shell. Kept as a boundary check even though
/// paths no longer cross IPC, so the invariant is enforced where it matters.
///
/// The returned path is rebuilt from the staging folder rather than being the
/// canonical one: on Windows `canonicalize` yields a `\\?\` verbatim path that
/// PowerShell and LibreOffice do not reliably accept.
fn confine(path: &Path) -> Result<PathBuf, String> {
    let base = staging_dir()
        .canonicalize()
        .map_err(|_| "Ungültiger Pfad".to_string())?;
    let candidate = path
        .canonicalize()
        .map_err(|_| "Ungültiger Pfad".to_string())?;

    if !candidate.is_file() || candidate.parent() != Some(base.as_path()) {
        return Err("Ungültiger Pfad".into());
    }
    let name = candidate
        .file_name()
        .ok_or_else(|| "Ungültiger Pfad".to_string())?;
    Ok(staging_dir().join(name))
}

/// Write a document into the staging folder and return its path.
fn stage(name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let dir = staging_dir();
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700); // owner only: staged documents carry personal data
    let _ = builder.create(&dir);

    // refuse to use the folder at all if something replaced it with a symlink
    if fs::symlink_metadata(&dir)
        .map_err(|e| io_error("stat", e))?
        .file_type()
        .is_symlink()
    {
        return Err("Ungültiger Ablageort".into());
    }

    let path = dir.join(safe(name));
    let _ = fs::remove_file(&path);

    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true); // O_EXCL: never follows an existing symlink
    #[cfg(unix)]
    opts.mode(0o600);

    opts.open(&path)
        .map_err(|e| io_error("write", e))?
        .write_all(bytes)
        .map_err(|e| io_error("write", e))?;

    Ok(path)
}

/// Native "save as" dialog. Runs here rather than in the webview so that the
/// destination is chosen by the user, never named by page script.
fn ask_save_path(app: &tauri::AppHandle, name: &str) -> Option<PathBuf> {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("docx")
        .to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .set_file_name(name)
        .add_filter(ext.to_uppercase(), &[ext.as_str()])
        .save_file(move |picked| {
            let _ = tx.send(picked);
        });
    rx.recv().ok().flatten().and_then(|p| p.into_path().ok())
}

fn ask_folder(app: &tauri::AppHandle, title: &str) -> Option<PathBuf> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .set_title(title)
        .pick_folder(move |picked| {
            let _ = tx.send(picked);
        });
    rx.recv().ok().flatten().and_then(|p| p.into_path().ok())
}

/// Save one document. Returns the chosen file name, or None if cancelled.
#[tauri::command]
async fn save_document(
    app: tauri::AppHandle,
    name: String,
    bytes: Vec<u8>,
) -> Result<Option<String>, String> {
    let Some(path) = ask_save_path(&app, &safe(&name)) else {
        return Ok(None);
    };
    fs::write(&path, bytes).map_err(|e| io_error("write", e))?;
    Ok(Some(
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or(name),
    ))
}

/// Ask once where a "save all" run should go. The webview sends the documents
/// one at a time afterwards, so only one document is ever held in memory.
#[tauri::command]
async fn save_all_begin(
    app: tauri::AppHandle,
    target: tauri::State<'_, SaveTarget>,
) -> Result<bool, String> {
    let picked = ask_folder(&app, "Ordner für die ausgefüllten Dokumente wählen");
    let chosen = picked.is_some();
    *target.0.lock().map_err(|_| "Interner Fehler".to_string())? = picked;
    Ok(chosen)
}

/// Write one document of a running "save all" into the chosen folder.
#[tauri::command]
async fn save_all_write(
    target: tauri::State<'_, SaveTarget>,
    name: String,
    bytes: Vec<u8>,
) -> Result<(), String> {
    let dir = target
        .0
        .lock()
        .map_err(|_| "Interner Fehler".to_string())?
        .clone()
        .ok_or("Kein Zielordner gewählt")?;
    // safe() keeps every write inside the folder the user picked
    fs::write(dir.join(safe(&name)), bytes).map_err(|e| io_error("write", e))
}

/// End a "save all" run and report where the documents went.
#[tauri::command]
async fn save_all_finish(
    target: tauri::State<'_, SaveTarget>,
) -> Result<Option<String>, String> {
    let dir = target
        .0
        .lock()
        .map_err(|_| "Interner Fehler".to_string())?
        .take();
    Ok(dir.map(|d| d.to_string_lossy().into_owned()))
}

/// Show the form window, building it the first time. The payload travels
/// through Rust because the two webviews are separate JavaScript realms; only
/// field descriptions and typed values cross, never document bytes.
#[tauri::command]
async fn open_form_window(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, FormBridge>,
    payload: String,
    id: String,
) -> Result<(), String> {
    let failed = || "Das Formularfenster konnte nicht geöffnet werden.".to_string();
    let oops = || "Interner Fehler".to_string();
    *bridge.payload.lock().map_err(|_| oops())? = Some(payload);

    // Ein Fenster pro Gruppe, damit ein bereits offenes nicht die Felder einer
    // anderen Gruppe zeigt: es liest seine Nutzlast nur beim Laden.
    let label = format!("form-{id}");
    let previous = bridge.shown.lock().map_err(|_| oops())?.clone();
    if let Some(prev) = previous {
        if prev != label {
            if let Some(w) = app.get_webview_window(&prev) {
                let _ = w.close();
            }
        }
    }
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.unminimize();
        return w.set_focus().map_err(|_| failed());
    }
    *bridge.shown.lock().map_err(|_| oops())? = Some(label.clone());
    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App("form.html".into()))
        .title("Formular")
        .inner_size(720.0, 760.0)
        .min_inner_size(420.0, 420.0)
        .build()
        .map(|_| ())
        .map_err(|_| failed())
}

/// The form window asks for what it should display.
#[tauri::command]
async fn form_payload(bridge: tauri::State<'_, FormBridge>) -> Result<String, String> {
    Ok(bridge
        .payload
        .lock()
        .map_err(|_| "Interner Fehler".to_string())?
        .clone()
        .unwrap_or_else(|| "{}".into()))
}

/// Entered values, saved as they are typed so they survive closing the window.
#[tauri::command]
async fn form_cache_values(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, FormBridge>,
    payload: String,
) -> Result<(), String> {
    *bridge.payload.lock().map_err(|_| "Interner Fehler".to_string())? = Some(payload.clone());
    // Das Hauptfenster hält dieselben Werte, sonst schreibt es beim erneuten
    // Öffnen seinen veralteten Stand über die Eingaben.
    let _ = app.emit_to("main", "docfill://values", payload);
    Ok(())
}

/// "Ausfüllen" in the form window: keep the values and wake the main window,
/// which owns the documents and does the actual filling.
#[tauri::command]
async fn form_submit(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, FormBridge>,
    payload: String,
) -> Result<(), String> {
    *bridge.payload.lock().map_err(|_| "Interner Fehler".to_string())? = Some(payload.clone());
    app.emit_to("main", "docfill://fill", payload)
        .map_err(|_| "Das Hauptfenster antwortet nicht.".to_string())
}

/// Close the form window and bring the main window back to the front.
#[tauri::command]
async fn close_form_window(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, FormBridge>,
) -> Result<(), String> {
    let label = bridge
        .shown
        .lock()
        .map_err(|_| "Interner Fehler".to_string())?
        .take();
    if let Some(l) = label {
        if let Some(w) = app.get_webview_window(&l) {
            let _ = w.close();
        }
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_focus();
    }
    Ok(())
}

/// Gespeicherte Gruppen liegen unter %APPDATA%\<identifier>\groups\<id>\ —
/// meta.json mit Name, Feldern und Werten, daneben 0.docx, 1.docx, …
/// Die Oberfläche nennt nie einen Pfad, nur die Kennung der Gruppe.
#[tauri::command]
async fn group_save_meta(app: tauri::AppHandle, id: String, payload: String) -> Result<(), String> {
    let dir = group_dir(&app, &id)?;
    fs::create_dir_all(&dir).map_err(|e| io_error("write", e))?;
    fs::write(dir.join("meta.json"), payload).map_err(|e| io_error("write", e))
}

/// One document per call — ein einzelner Aufruf mit allen Dokumenten würde
/// hunderte Megabyte durch die IPC-Grenze schieben.
#[tauri::command]
async fn group_save_doc(
    app: tauri::AppHandle,
    id: String,
    index: usize,
    bytes: Vec<u8>,
) -> Result<(), String> {
    if bytes.len() > MAX_DOC {
        return Err("Das Dokument ist zu groß.".into());
    }
    let dir = group_dir(&app, &id)?;
    fs::create_dir_all(&dir).map_err(|e| io_error("write", e))?;
    if dir_size(&dir) + bytes.len() > MAX_GROUP {
        return Err("Die Gruppe ist zu groß.".into());
    }
    fs::write(dir.join(format!("{index}.docx")), bytes).map_err(|e| io_error("write", e))
}

#[tauri::command]
async fn group_list(app: tauri::AppHandle) -> Result<Vec<StoredGroup>, String> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(groups_dir(&app)?) else {
        return Ok(out); // noch nichts gespeichert
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if group_dir(&app, &id).is_err() {
            continue; // Fremder Ordner, keine Gruppe
        }
        if let Ok(meta) = fs::read_to_string(path.join("meta.json")) {
            out.push(StoredGroup { id, meta });
        }
    }
    Ok(out)
}

#[tauri::command]
async fn group_read_doc(
    app: tauri::AppHandle,
    id: String,
    index: usize,
) -> Result<Vec<u8>, String> {
    let dir = group_dir(&app, &id)?;
    fs::read(dir.join(format!("{index}.docx"))).map_err(|e| io_error("read", e))
}

#[tauri::command]
async fn group_delete(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let dir = group_dir(&app, &id)?;
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| io_error("delete", e))?;
    }
    Ok(())
}

/// Stage the document and hand it to Word / Pages / LibreOffice.
#[tauri::command]
async fn open_document(name: String, bytes: Vec<u8>) -> Result<(), String> {
    let path = stage(&name, &bytes)?;
    open::that(&confine(&path)?).map_err(|e| io_error("open", e))
}

/// Stage the document, convert it to PDF with LibreOffice and send it to the
/// default printer. Returns "printed" or "opened" (fallback: PDF opened in the
/// viewer for manual printing).
#[tauri::command]
async fn print_document(
    name: String,
    bytes: Vec<u8>,
    printer: Option<String>,
) -> Result<String, String> {
    let staged = stage(&name, &bytes)?;
    let src = confine(&staged)?;

    // Word steuert das Dokument unsichtbar aus und druckt auf den gewählten
    // Drucker. LibreOffice wird dafür überhaupt nicht gebraucht.
    #[cfg(target_os = "windows")]
    {
        if word_print(&src, printer.as_deref()) {
            return Ok("printed".into());
        }
        // Notnagel: das „Drucken“-Verb der Shell. Dabei wird Word sichtbar und
        // der Drucker lässt sich nicht wählen, aber gedruckt wird.
        if shell_print(&src) {
            return Ok("printed".into());
        }
    }

    // Sonst der plattformunabhängige Weg: LibreOffice wandelt nach PDF, weil es
    // hier keinen anderen Weg gibt, ein .docx ohne Word zu setzen.
    let out_dir = src.parent().ok_or("Ungültiger Pfad")?.to_path_buf();
    let soffice = find_soffice().ok_or(
        "Zum Drucken wird Word oder LibreOffice gebraucht. Sonst \u{201e}\u{00d6}ffnen\u{201c} verwenden und von dort drucken.",
    )?;
    let status = Command::new(soffice)
        .args(["--headless", "--convert-to", "pdf", "--outdir"])
        .arg(&out_dir).arg(&src)
        .status().map_err(|e| io_error("convert", e))?;
    if !status.success() { return Err("PDF-Konvertierung fehlgeschlagen".into()); }
    let pdf = src.with_extension("pdf");

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let ok = {
        let mut c = Command::new("lp");
        if let Some(p) = printer.as_deref().filter(|p| !p.is_empty()) {
            c.arg("-d").arg(p);
        }
        c.arg(&pdf).status().map(|s| s.success()).unwrap_or(false)
    };
    #[cfg(target_os = "windows")]
    let ok = shell_print(&pdf);

    if ok { Ok("printed".into()) } else {
        open::that(&pdf).map_err(|e| io_error("open", e))?;
        Ok("opened".into())
    }
}

/// Word im Hintergrund: Dokument öffnen, drucken, schließen — ohne Fenster.
/// Nur so lässt sich auch der Drucker bestimmen; das „Drucken“-Verb der Shell
/// nimmt immer den Standarddrucker und zeigt Word dabei an.
///
/// ActivePrinter gilt in Word programmweit und bleibt gespeichert, deshalb wird
/// die vorherige Einstellung am Ende zurückgesetzt.
/// PowerShell als Hilfsprozess starten, ohne dass ein Konsolenfenster aufblitzt.
/// Docfill selbst ist ein GUI-Programm, powershell.exe ist ein Konsolenprogramm —
/// ohne CREATE_NO_WINDOW macht Windows dafür ein sichtbares Fenster auf.
#[cfg(target_os = "windows")]
fn powershell(script: &str) -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut c = Command::new("powershell");
    c.args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW);
    c
}

#[cfg(target_os = "windows")]
const WORD_PRINT: &str = r#"
$ErrorActionPreference = 'Stop'
$w = New-Object -ComObject Word.Application
$w.Visible = $false
$w.DisplayAlerts = 0
$old = $null
try {
  if ($env:DOCFILL_PRINTER) { $old = $w.ActivePrinter; $w.ActivePrinter = $env:DOCFILL_PRINTER }
  $w.Options.PrintBackground = $false
  $doc = $w.Documents.Open($env:DOCFILL_PRINT, $false, $true)
  $doc.PrintOut()
  $doc.Close(0)
} finally {
  if ($old) { try { $w.ActivePrinter = $old } catch {} }
  $w.Quit()
}
"#;

#[cfg(target_os = "windows")]
fn word_print(path: &Path, printer: Option<&str>) -> bool {
    powershell(WORD_PRINT)
        .env("DOCFILL_PRINT", path)
        .env("DOCFILL_PRINTER", printer.unwrap_or(""))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Die eingerichteten Drucker, damit die Oberfläche einen anbieten kann.
#[derive(serde::Serialize)]
struct Printer {
    name: String,
    default: bool,
}

#[tauri::command]
async fn list_printers() -> Result<Vec<Printer>, String> {
    #[cfg(target_os = "windows")]
    let out =
        powershell("Get-CimInstance Win32_Printer | ForEach-Object { \"$($_.Name)`t$([int]$_.Default)\" }")
            .output();
    #[cfg(not(target_os = "windows"))]
    let out = Command::new("lpstat").arg("-a").output();

    let out = out.map_err(|e| io_error("printers", e))?;
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .lines()
        .filter_map(|line| {
            #[cfg(target_os = "windows")]
            let (name, flag) = {
                let mut parts = line.splitn(2, '\t');
                (parts.next()?.trim(), parts.next().unwrap_or("").trim() == "1")
            };
            #[cfg(not(target_os = "windows"))]
            let (name, flag) = (line.split_whitespace().next()?, false);

            if name.is_empty() {
                return None;
            }
            Some(Printer { name: name.to_string(), default: flag })
        })
        .collect())
}

/// Über das „Drucken“-Verb der Shell drucken lassen — für .docx übernimmt das
/// Word, für .pdf der eingestellte Betrachter.
///
/// Der Pfad reist in einer Umgebungsvariablen statt im Befehlstext, damit ein
/// Anführungszeichen im Dateinamen das Argument nicht beenden und einen neuen
/// Befehl anfangen kann.
#[cfg(target_os = "windows")]
fn shell_print(path: &Path) -> bool {
    powershell("Start-Process -FilePath $env:DOCFILL_PRINT -Verb Print -PassThru | Out-Null")
        .env("DOCFILL_PRINT", path)
        .status().map(|s| s.success()).unwrap_or(false)
}

fn find_soffice() -> Option<PathBuf> {
    let candidates: &[&str] = if cfg!(target_os = "macos") {
        &["/Applications/LibreOffice.app/Contents/MacOS/soffice"]
    } else if cfg!(target_os = "windows") {
        &[r"C:\Program Files\LibreOffice\program\soffice.exe", r"C:\Program Files (x86)\LibreOffice\program\soffice.exe"]
    } else {
        &["/usr/bin/soffice", "/usr/bin/libreoffice", "/snap/bin/libreoffice"]
    };
    candidates.iter().map(PathBuf::from).find(|p| p.exists())
        .or_else(|| which("soffice"))
}

fn which(bin: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).map(|d| d.join(bin)).find(|p| p.exists())
    })
}

fn safe(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || "._- ".contains(c) { c } else { '_' }).collect()
}

mod open {
    // Unter Windows läuft das über super::powershell, dort wird Command nicht gebraucht.
    #[cfg(not(target_os = "windows"))]
    use std::process::Command;
    pub fn that(p: &std::path::Path) -> std::io::Result<()> {
        #[cfg(target_os = "macos")] let mut c = { let mut c = Command::new("open"); c.arg(p); c };
        // Not `cmd /C start`: cmd.exe re-parses its arguments, and `start` will
        // launch an executable as readily as it opens a PDF.
        #[cfg(target_os = "windows")] let mut c = {
            // ebenfalls ohne Konsolenfenster, siehe super::powershell
            let mut c = super::powershell("Start-Process -FilePath $env:DOCFILL_OPEN");
            c.env("DOCFILL_OPEN", p);
            c
        };
        #[cfg(target_os = "linux")] let mut c = { let mut c = Command::new("xdg-open"); c.arg(p); c };
        c.spawn().map(|_| ())
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .manage(SaveTarget::default())
        .manage(FormBridge::default())
        .invoke_handler(tauri::generate_handler![
            open_document,
            print_document,
            save_document,
            save_all_begin,
            save_all_write,
            save_all_finish,
            open_form_window,
            form_payload,
            form_cache_values,
            form_submit,
            close_form_window,
            group_save_meta,
            group_save_doc,
            group_list,
            group_read_doc,
            group_delete,
            list_printers
        ])
        .build(tauri::generate_context!())
        .expect("error while running docfill")
        .run(|_app, event| {
            // staged documents hold personal data; don't leave them in temp
            if let tauri::RunEvent::Exit = event {
                let _ = fs::remove_dir_all(staging_dir());
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_neutralises_separators_and_quotes() {
        assert_eq!(safe(r"..\..\Windows\System32\evil.dll"), ".._.._Windows_System32_evil.dll");
        assert_eq!(safe("../../etc/passwd"), ".._.._etc_passwd");
        let injected = safe("a'; calc; #.docx");
        assert!(!injected.contains('\''), "quote survived: {injected}");
        assert!(!injected.contains(';'), "semicolon survived: {injected}");
    }

    #[test]
    fn confine_accepts_only_staged_files() {
        let staged = stage("ok.docx", b"x").unwrap();
        assert!(confine(&staged).is_ok(), "a genuinely staged file must be usable");

        // outside the staging folder
        let outside = std::env::temp_dir().join("docfill-outside-probe.docx");
        fs::write(&outside, b"x").unwrap();
        assert!(confine(&outside).is_err(), "path outside staging accepted");

        // traversal back out of the staging folder
        let traversal = staging_dir().join("..").join("docfill-outside-probe.docx");
        assert!(confine(&traversal).is_err(), "traversal accepted");

        // a directory, and a path that does not exist
        assert!(confine(&staging_dir()).is_err(), "directory accepted");
        assert!(confine(Path::new("/nonexistent/nope.docx")).is_err(), "missing path accepted");
        let _ = fs::remove_file(&outside);
    }

    #[test]
    fn save_all_cannot_escape_the_chosen_folder() {
        // the webview supplies names, not paths: every name is flattened
        for hostile in [r"..\..\Windows\System32\evil.dll", "../../etc/cron.d/evil", "/etc/passwd"] {
            let flattened = safe(hostile);
            let joined = Path::new("/tmp/chosen").join(&flattened);
            assert_eq!(joined.parent(), Some(Path::new("/tmp/chosen")),
                       "name escaped the chosen folder: {hostile} -> {flattened}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn stage_does_not_follow_a_preplaced_symlink() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let victim = std::env::temp_dir().join("docfill-victim.txt");
        fs::write(&victim, b"ORIGINAL").unwrap();

        // attacker pre-places a symlink where the next staged file will land
        let dir = staging_dir();
        let mut b = DirBuilder::new();
        b.recursive(true);
        b.mode(0o700);
        let _ = b.create(&dir);
        let target = dir.join("trap.docx");
        let _ = fs::remove_file(&target);
        symlink(&victim, &target).unwrap();

        stage("trap.docx", b"EVIL").unwrap();

        assert_eq!(fs::read(&victim).unwrap(), b"ORIGINAL",
                   "symlink was followed: the victim file was overwritten");
        assert_eq!(fs::read(&target).unwrap(), b"EVIL");

        let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "staged document is not owner-only: {mode:o}");
        let dmode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(dmode, 0o700, "staging folder is not owner-only: {dmode:o}");

        let _ = fs::remove_file(&victim);
    }
}
