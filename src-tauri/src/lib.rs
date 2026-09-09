use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

/// What the session filled so far, as opaque JSON — names, sizes and times, no
/// document bytes. Same passthrough as `FormBridge`: the history window cannot
/// read the main window's memory, so the metadata travels through here and Rust
/// never looks inside it.
///
/// Nothing of this reaches the disk. It lives for as long as the process does,
/// which is what "the history is gone when the app closes" rests on.
#[derive(Default)]
struct HistoryBridge {
    payload: Mutex<Option<String>>,
}

/// Field descriptions of a single document for the gear window, as opaque JSON.
/// Same passthrough as `FormBridge`, and `shown` for the same reason: the page
/// reads its payload once, when it loads, so another document needs another
/// window rather than a focus call.
#[derive(Default)]
struct KeysBridge {
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

/// Where the documents opened through the native dialog live. Only these can be
/// written back over — a file that arrived by drag & drop or through the file
/// input carries no path, not even inside the browser engine.
///
/// The webview never sees a path: it gets the handle it was given when the file
/// was opened and names that. Entries stay for the life of the process; removing
/// a document from the list only drops it in the frontend.
#[derive(Default)]
struct OriginPaths {
    by_id: Mutex<std::collections::HashMap<String, PathBuf>>,
    next: Mutex<u64>,
}

/// One document as it comes back from the open dialog. `oversize` marks a file
/// that was left unread because it is past `MAX_DOC` — the name still travels so
/// that the interface can say which one it was.
#[derive(serde::Serialize)]
struct PickedDoc {
    id: String,
    name: String,
    bytes: Vec<u8>,
    oversize: bool,
}

/// Label of the history window. Fixed, unlike the per-group form windows: there
/// is only ever one history.
const HISTORY_LABEL: &str = "history";

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
        "read" => "Die Datei konnte nicht gelesen werden.".into(),
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

/// Native "open" dialog. Same reason as `ask_save_path`: the paths are picked by
/// the user and stay in Rust.
fn ask_open_paths(app: &tauri::AppHandle) -> Vec<PathBuf> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .add_filter("DOCX", &["docx"])
        .pick_files(move |picked| {
            let _ = tx.send(picked);
        });
    rx.recv()
        .ok()
        .flatten()
        .map(|v| v.into_iter().filter_map(|p| p.into_path().ok()).collect())
        .unwrap_or_default()
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

/// Open documents through the native dialog. Their contents go to the interface
/// as they always did; what is new is that the path stays here, so the same file
/// can later be written back over.
#[tauri::command]
async fn pick_documents(
    app: tauri::AppHandle,
    origins: tauri::State<'_, OriginPaths>,
) -> Result<Vec<PickedDoc>, String> {
    let oops = || "Interner Fehler".to_string();
    let mut out = Vec::new();
    for path in ask_open_paths(&app) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let bytes = fs::read(&path).map_err(|e| io_error("read", e))?;
        if bytes.len() > MAX_DOC {
            out.push(PickedDoc { id: String::new(), name, bytes: Vec::new(), oversize: true });
            continue;
        }
        let id = {
            let mut n = origins.next.lock().map_err(|_| oops())?;
            *n += 1;
            format!("f{n}")
        };
        origins.by_id.lock().map_err(|_| oops())?.insert(id.clone(), path);
        out.push(PickedDoc { id, name, bytes, oversize: false });
    }
    Ok(out)
}

/// Write a document back over the file it was opened from — without asking, that
/// is the point of it. Only a handle from `pick_documents` names a path; anything
/// else is refused, so page script cannot pick a target.
///
/// Written beside the file first and then renamed over it: if the write breaks
/// off, the old version is still there in one piece. `fs::rename` replaces an
/// existing file on both Windows and Unix.
#[tauri::command]
async fn write_back(
    origins: tauri::State<'_, OriginPaths>,
    id: String,
    bytes: Vec<u8>,
) -> Result<String, String> {
    if bytes.len() > MAX_DOC {
        return Err("Das Dokument ist zu groß.".into());
    }
    let path = origins
        .by_id
        .lock()
        .map_err(|_| "Interner Fehler".to_string())?
        .get(&id)
        .cloned()
        .ok_or_else(|| {
            "Zu dieser Datei ist kein Pfad bekannt: sie wurde nicht über den Öffnen-Dialog geladen."
                .to_string()
        })?;

    write_over(&path, &bytes)?;
    Ok(path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default())
}

/// Replace a file with new content. Written beside it first and then renamed
/// over it, so a write that breaks off leaves the old version whole; the leftover
/// is cleared away if the rename fails. `fs::rename` replaces an existing file on
/// Windows as well as on Unix.
fn write_over(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("docfill-neu");
    fs::write(&tmp, bytes).map_err(|e| io_error("write", e))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io_error("write", e)
    })
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
        // 668 = die 640px breite Feldspalte aus form.html plus die 14px Polster
        // links und rechts: breiter wird das Fenster nur zu leerem Rand.
        .inner_size(668.0, 640.0)
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

/// The main window hands over the current session history, metadata only.
/// Called after every fill run, so an open history window finds the new state
/// the next time it asks.
#[tauri::command]
async fn history_publish(
    bridge: tauri::State<'_, HistoryBridge>,
    payload: String,
) -> Result<(), String> {
    *bridge
        .payload
        .lock()
        .map_err(|_| "Interner Fehler".to_string())? = Some(payload);
    Ok(())
}

/// Show the history window, building it the first time.
#[tauri::command]
async fn open_history_window(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, HistoryBridge>,
    payload: String,
) -> Result<(), String> {
    let failed = || "Das Historie-Fenster konnte nicht geöffnet werden.".to_string();
    *bridge
        .payload
        .lock()
        .map_err(|_| "Interner Fehler".to_string())? = Some(payload);

    // Anders als beim Formular genügt hier ein Fenster: es liest seine Nutzlast
    // bei jedem `focus` neu und zeigt deshalb nie einen veralteten Stand.
    if let Some(w) = app.get_webview_window(HISTORY_LABEL) {
        let _ = w.unminimize();
        return w.set_focus().map_err(|_| failed());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        HISTORY_LABEL,
        tauri::WebviewUrl::App("history.html".into()),
    )
    .title("Historie")
    .inner_size(620.0, 700.0)
    .min_inner_size(420.0, 360.0)
    .build()
    .map(|_| ())
    .map_err(|_| failed())
}

/// The history window asks for what it should display.
#[tauri::command]
async fn history_payload(bridge: tauri::State<'_, HistoryBridge>) -> Result<String, String> {
    Ok(bridge
        .payload
        .lock()
        .map_err(|_| "Interner Fehler".to_string())?
        .clone()
        .unwrap_or_else(|| "[]".into()))
}

/// "Öffnen"/"Speichern"/"Drucken" in the history window: the window only knows
/// which row was picked, so the request goes to the main window, which holds
/// the documents and does the work.
#[tauri::command]
async fn history_action(app: tauri::AppHandle, payload: String) -> Result<(), String> {
    app.emit_to("main", "docfill://history-action", payload)
        .map_err(|_| "Das Hauptfenster antwortet nicht.".to_string())
}

/// Show the gear window for one document, building it the first time. Only the
/// descriptions of its content controls cross; the document itself stays in the
/// main window, like the form payload.
#[tauri::command]
async fn open_keys_window(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, KeysBridge>,
    payload: String,
    id: String,
) -> Result<(), String> {
    let failed = || "Das Fenster konnte nicht geöffnet werden.".to_string();
    let oops = || "Interner Fehler".to_string();
    *bridge.payload.lock().map_err(|_| oops())? = Some(payload);

    // Ein Fenster je Dokument, damit ein offenes nicht die Steuerelemente einer
    // anderen Datei zeigt: es liest seine Nutzlast nur beim Laden.
    let label = format!("keys-{id}");
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
    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App("keys.html".into()))
        .title("Felder & Schlüssel")
        // wie beim Formularfenster: die 640px breite Spalte plus 14px Polster
        .inner_size(668.0, 640.0)
        .min_inner_size(420.0, 420.0)
        .build()
        .map(|_| ())
        .map_err(|_| failed())
}

/// The gear window asks for what it should display.
#[tauri::command]
async fn keys_payload(bridge: tauri::State<'_, KeysBridge>) -> Result<String, String> {
    Ok(bridge
        .payload
        .lock()
        .map_err(|_| "Interner Fehler".to_string())?
        .clone()
        .unwrap_or_else(|| "{}".into()))
}

/// "Übernehmen" in the gear window: the window only knows which control got
/// which key, so the result goes to the main window, which holds the document
/// and closes this window afterwards.
#[tauri::command]
async fn keys_submit(app: tauri::AppHandle, payload: String) -> Result<(), String> {
    app.emit_to("main", "docfill://keys", payload)
        .map_err(|_| "Das Hauptfenster antwortet nicht.".to_string())
}

/// Close the gear window and bring the main window back to the front.
#[tauri::command]
async fn close_keys_window(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, KeysBridge>,
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

/* ---------- Unterschrift vom Tablet ----------
   Der Klient sitzt im selben Raum und unterschreibt mit dem Stift auf dem
   Tablet des Nutzers. Beide Geräte hängen im selben WLAN, deshalb genügt ein
   winziger Server im lokalen Netz — kein Tunnel, kein fremder Anbieter.

   Ausgeliefert wird ausschließlich die Unterschriftsseite. Das ausgefüllte
   Dokument verlässt den Rechner nie: hinaus geht sein Name, herein kommt ein
   Bild. Das Staging-Verzeichnis wird hier deshalb gar nicht erst angefasst.

   Was zurückkommt, reicht Rust unbesehen ans Hauptfenster weiter — dieselbe
   Trennung wie bei den Fensterbrücken weiter oben. Rust prüft nur die Größe. */

const SIGN_MAX_BODY: usize = 2 * 1024 * 1024;
const SIGN_TTL: Duration = Duration::from_secs(10 * 60);
const SIGN_MAX_MISSES: u32 = 20;
/// Die Seite liegt in `src/`, damit `scripts/check.mjs` sie mitprüft. Sie läuft
/// in Safari auf dem Tablet, nicht im Webview, und wird deshalb hier eingebettet
/// statt über Tauris Asset-Protokoll ausgeliefert.
const SIGN_PAGE: &str = include_str!("../../src/sign.html");

#[derive(Default)]
struct SignServer {
    session: Mutex<Option<SignSession>>,
    /// Zählt die Sitzungen durch. Ein auslaufender Thread darf nur seine eigene
    /// Sitzung aufräumen, sonst schlösse er den Server einer inzwischen neu
    /// begonnenen.
    seq: Mutex<u64>,
}

struct SignSession {
    id: u64,
    server: Arc<tiny_http::Server>,
    /// Gesetzt, bevor der wartende Thread geweckt wird. `tiny_http` verrät von
    /// sich aus nicht, ob es geweckt oder nur nichts gekommen ist — an dieser
    /// Flagge unterscheidet der Thread beides.
    stop: Arc<AtomicBool>,
}

/// Beendet die laufende Sitzung, falls es eine gibt. Idempotent wie
/// `wipe_staging()` — es darf jederzeit nichts zu tun geben.
fn sign_stop(state: &SignServer) {
    let taken = state.session.lock().ok().and_then(|mut s| s.take());
    if let Some(session) = taken {
        session.stop.store(true, Ordering::SeqCst);
        // Weckt den wartenden Thread; `recv_timeout` liefert dann nichts mehr.
        session.server.unblock();
    }
}

/// Vergleicht über die volle Länge, ohne bei der ersten Abweichung auszusteigen.
/// Bei einem Geheimnis im Netz soll die Antwortzeit nicht verraten, wie viele
/// Zeichen schon stimmen.
fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn esc_html(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&#39;".into(),
            _ => c.to_string(),
        })
        .collect()
}

/// Die Adresse dieses Rechners im lokalen Netz.
///
/// Verschickt nichts: UDP ist verbindungslos, `connect` schlägt nur in der
/// Routing-Tabelle nach, welche Netzwerkkarte für ein fremdes Ziel zuständig
/// wäre, und `local_addr` verrät dann deren Adresse. Ohne Standardweg — kein
/// WLAN, kein Netz — schlägt das fehl, und der Nutzer bekommt das gesagt,
/// statt eine unerreichbare Adresse angeboten zu bekommen.
fn lan_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.168.0.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    // Nur eine Adresse aus dem privaten Bereich taugt. Bei aktivem VPN zeigt die
    // Standardroute in den Tunnel; dessen Adresse erreicht das Tablet nie, und
    // ein QR-Code, der ins Leere führt, ist schlimmer als eine klare Absage.
    match ip {
        std::net::IpAddr::V4(v4) => v4.is_private().then_some(ip),
        std::net::IpAddr::V6(_) => None,
    }
}

fn header(name: &str, value: &str) -> Option<tiny_http::Header> {
    tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()).ok()
}

/// Alles, was nicht genau der Link mit dem richtigen Zeichen ist, bekommt
/// dieselbe nichtssagende Antwort — auch der abgelaufene und der bereits
/// benutzte Link. Wer den Port findet, soll daraus nichts ableiten können.
fn sign_reject(request: tiny_http::Request) {
    let _ = request.respond(
        tiny_http::Response::from_string("Nicht gefunden.").with_status_code(404),
    );
}

/// Mindestens 128 Bit, hexadezimal. Die Länge steht hier und nicht nur in der
/// Oberfläche: das Zeichen ist das Einzige, was den Link im WLAN schützt, und
/// diese Zusage soll nicht davon abhängen, was das Webview schickt.
fn token_plausible(token: &str) -> bool {
    token.len() >= 32 && token.chars().all(|c| c.is_ascii_hexdigit())
}

/// Startet den Server und gibt die Adresse zurück, die auf den QR-Code kommt.
/// Das Zeichen kommt aus dem Webview (`crypto.getRandomValues`) — der Nonce aus
/// `staging_dir()` taugt dafür nicht, der ist aus der Uhrzeit gebaut und ratbar.
#[tauri::command]
async fn sign_begin(
    app: tauri::AppHandle,
    state: tauri::State<'_, SignServer>,
    token: String,
    doc: String,
) -> Result<String, String> {
    if !token_plausible(&token) {
        return Err("Interner Fehler".into());
    }
    sign_stop(&state);

    let ip = lan_ip().ok_or_else(|| {
        "Kein lokales Netz gefunden. Hängen Rechner und Tablet im selben WLAN?".to_string()
    })?;
    // Port 0: das Betriebssystem sucht einen freien aus, damit ein fest
    // gewählter nicht eines Tages belegt ist.
    let server = tiny_http::Server::http("0.0.0.0:0")
        .map_err(|_| "Der Server konnte nicht gestartet werden.".to_string())?;
    let port = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| "Der Server konnte nicht gestartet werden.".to_string())?
        .port();
    let server = Arc::new(server);

    let id = {
        let mut seq = state.seq.lock().map_err(|_| "Interner Fehler".to_string())?;
        *seq += 1;
        *seq
    };
    let stop = Arc::new(AtomicBool::new(false));
    *state
        .session
        .lock()
        .map_err(|_| "Interner Fehler".to_string())? = Some(SignSession {
        id,
        server: server.clone(),
        stop: stop.clone(),
    });

    let path = format!("/s/{token}");
    // Nur der Name wird eingesetzt. Wohin die Seite ihre Unterschrift schickt,
    // liest sie selbst aus ihrer eigenen Adresse — das Zeichen steht schon
    // darin und wird so kein zweites Mal zusammengesetzt.
    let page = SIGN_PAGE.replace("{{DOKUMENT}}", &esc_html(&doc));
    let url = format!("http://{ip}:{port}{path}");

    std::thread::spawn(move || {
        sign_serve(&app, &server, &path, page, id, &stop);
    });

    Ok(url)
}

/// Die Schleife des Servers. Läuft, bis unterschrieben wurde, bis die Zeit
/// abgelaufen ist oder bis jemand abbricht.
fn sign_serve(
    app: &tauri::AppHandle,
    server: &Arc<tiny_http::Server>,
    path: &str,
    page: String,
    id: u64,
    stop: &AtomicBool,
) {
    let started = Instant::now();
    let mut daneben: u32 = 0;
    // Kurze Wartezeit statt endlosem Blockieren: nur so merkt die Schleife von
    // selbst, dass die zehn Minuten um sind, wenn nie jemand anklopft.
    while started.elapsed() < SIGN_TTL {
        let request = match server.recv_timeout(Duration::from_secs(1)) {
            Ok(Some(request)) => request,
            // Nichts gekommen — oder abgebrochen. `unblock()` lässt `recv_timeout`
            // von da an dauerhaft nichts liefern; ohne diese Prüfung liefe die
            // Schleife bis zum Ablauf der Frist weiter und hielte den Port so
            // lange offen, obwohl längst abgebrochen wurde.
            Ok(None) => {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                continue;
            }
            Err(_) => break,
        };
        // Der Pfad trägt das Zeichen. Stimmt es nicht, gibt es nichts zu sehen.
        if !ct_eq(request.url(), path) {
            sign_reject(request);
            // Wer den Port findet, darf nicht in Ruhe raten. Nach ein paar
            // Fehlgriffen ist die Sitzung verbraucht statt bis zum Ablauf offen.
            daneben += 1;
            if daneben >= SIGN_MAX_MISSES {
                break;
            }
            continue;
        }
        match request.method() {
            tiny_http::Method::Get => {
                let mut response = tiny_http::Response::from_string(page.clone());
                if let Some(h) = header("Content-Type", "text/html; charset=utf-8") {
                    response.add_header(h);
                }
                // Der Link gilt einmal; ein Zwischenspeicher würde die Seite
                // nach dem Absenden noch einmal zeigen.
                if let Some(h) = header("Cache-Control", "no-store") {
                    response.add_header(h);
                }
                // Die CSP aus tauri.conf.json gilt für den Webview, nicht für
                // Safari auf dem Tablet. Die Seite lädt nichts nach, also darf
                // hier alles zu sein außer dem, was sie selbst mitbringt.
                for (name, value) in [
                    ("Content-Security-Policy",
                     "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; connect-src 'self'; form-action 'none'; base-uri 'none'"),
                    ("X-Content-Type-Options", "nosniff"),
                    ("Referrer-Policy", "no-referrer"),
                ] {
                    if let Some(h) = header(name, value) {
                        response.add_header(h);
                    }
                }
                let _ = request.respond(response);
            }
            tiny_http::Method::Post => {
                match sign_read_body(request) {
                    Ok(body) => {
                        // Rust schaut nicht hinein — das Hauptfenster kennt die
                        // Form, wie bei den Fensterbrücken auch.
                        let _ = app.emit_to("main", "docfill://signature", body);
                        break; // einmalig: nach der Unterschrift ist Schluss
                    }
                    Err(response) => {
                        let _ = response;
                    }
                }
            }
            _ => {}
        }
    }

    // Nur die eigene Sitzung aufräumen: `sign_begin` kann inzwischen eine neue
    // begonnen haben, deren Server weiterlaufen muss.
    if let Some(state) = app.try_state::<SignServer>() {
        if let Ok(mut session) = state.session.lock() {
            if session.as_ref().is_some_and(|s| s.id == id) {
                *session = None;
            }
        }
    }
}

/// Liest den Rumpf einer Unterschrift, mit harter Obergrenze. Antwortet selbst,
/// damit der Anrufer nicht ohne Antwort dasteht.
fn sign_read_body(mut request: tiny_http::Request) -> Result<String, ()> {
    if request.body_length().is_some_and(|n| n > SIGN_MAX_BODY) {
        let _ = request.respond(
            tiny_http::Response::from_string("Zu groß.").with_status_code(413),
        );
        return Err(());
    }
    let mut body = String::new();
    // `take` auch dann, wenn keine Länge angekündigt war: bei „chunked" steht
    // sie nicht im Kopf, und ohne Grenze liefe der Speicher voll.
    let read = request
        .as_reader()
        .take(SIGN_MAX_BODY as u64 + 1)
        .read_to_string(&mut body);
    if read.is_err() || body.len() > SIGN_MAX_BODY {
        let _ = request.respond(
            tiny_http::Response::from_string("Fehlerhafte Anfrage.").with_status_code(400),
        );
        return Err(());
    }
    let _ = request.respond(tiny_http::Response::from_string("Danke."));
    Ok(body)
}

/// Bricht ab, ohne dass unterschrieben wurde.
#[tauri::command]
async fn sign_cancel(state: tauri::State<'_, SignServer>) -> Result<(), String> {
    sign_stop(&state);
    Ok(())
}

/// Der QR-Code zur Adresse, als SVG. Die Oberfläche macht daraus eine
/// `data:`-Adresse — das erlaubt die CSP über `img-src 'self' data: blob:`,
/// eine Änderung daran ist also nicht nötig.
#[tauri::command]
async fn sign_qr(url: String) -> Result<String, String> {
    let code = qrcode::QrCode::new(url.as_bytes())
        .map_err(|_| "Der QR-Code konnte nicht erzeugt werden.".to_string())?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(240, 240)
        .quiet_zone(true)
        .build())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .manage(SaveTarget::default())
        .manage(FormBridge::default())
        .manage(HistoryBridge::default())
        .manage(KeysBridge::default())
        .manage(OriginPaths::default())
        .manage(SignServer::default())
        // Mit dem Hauptfenster endet das Programm. Ohne das hier lief der
        // Prozess nach dem Schließen weiter — ohne Fenster, aber am Leben —,
        // und damit blieb der Ablageordner im Temp liegen und die Historie
        // wäre nicht wirklich weg gewesen.
        .setup(|app| {
            if let Some(main) = app.get_webview_window("main") {
                let handle = app.handle().clone();
                main.on_window_event(move |event| {
                    if let tauri::WindowEvent::Destroyed = event {
                        for w in handle.webview_windows().values() {
                            if w.label() != "main" {
                                let _ = w.close();
                            }
                        }
                        // Hier aufräumen und nicht erst in RunEvent::Exit: das
                        // Versprechen, dass nichts im Temp liegen bleibt, soll
                        // nicht davon abhängen, über welchen Weg das Programm
                        // endet. remove_dir_all darf ins Leere greifen.
                        // Der Unterschriftsserver hört auf, sobald das
                        // Fenster weg ist — sonst bliebe ein offener Port im
                        // WLAN zurück, auf den niemand mehr achtet.
                        if let Some(state) = handle.try_state::<SignServer>() {
                            sign_stop(&state);
                        }
                        wipe_staging();
                        // Ausdrücklich beenden, statt darauf zu bauen, dass
                        // Tauri von selbst geht, wenn das letzte Fenster zu ist.
                        handle.exit(0);
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_document,
            pick_documents,
            write_back,
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
            history_publish,
            open_history_window,
            history_payload,
            history_action,
            open_keys_window,
            keys_payload,
            keys_submit,
            close_keys_window,
            group_save_meta,
            group_save_doc,
            group_list,
            group_read_doc,
            group_delete,
            list_printers,
            sign_begin,
            sign_cancel,
            sign_qr
        ])
        .build(tauri::generate_context!())
        .expect("error while running docfill")
        .run(|app, event| {
            // staged documents hold personal data; don't leave them in temp.
            // Zweiter Halt neben dem Fenster-Handler: greift für Wege, die am
            // Hauptfenster vorbeigehen, etwa ⌘Q auf macOS.
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<SignServer>() {
                    sign_stop(&state);
                }
                wipe_staging();
            }
        });
}

/// Removes the staging folder. Idempotent — it may well be gone already, or
/// never have been created, because `stage()` builds it only on first use.
fn wipe_staging() {
    let _ = fs::remove_dir_all(staging_dir());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialisiert die Tests, die den Ablageordner anfassen: der Pfad ist ein
    /// prozessweites `OnceLock`, und `wipe_staging` zieht den Ordner unter
    /// allem weg, was im selben Moment eine Datei ablegt.
    static STAGING: Mutex<()> = Mutex::new(());

    /// Ein gescheiterter Test soll die übrigen nicht mitreißen.
    fn staging_guard() -> std::sync::MutexGuard<'static, ()> {
        STAGING.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn ct_eq_matches_ordinary_eq() {
        for (a, b) in [
            ("", ""),
            ("abc", "abc"),
            ("abc", "abd"),
            ("abc", "ab"),
            ("", "a"),
            ("0f9e", "0F9E"),
        ] {
            assert_eq!(ct_eq(a, b), a == b, "ct_eq({a:?}, {b:?})");
        }
    }

    #[test]
    fn esc_html_neutralises_markup() {
        // Der Dateiname kommt vom Nutzer und steht auf der Seite, die ein
        // fremdes Gerät anzeigt. Er darf dort kein Auszeichnungselement werden.
        let out = esc_html(r#"<script>alert("x")</script>&'"#);
        for c in ['<', '>', '"', '\''] {
            assert!(!out.contains(c), "{c:?} hat überlebt: {out}");
        }
        assert!(out.contains("&amp;"), "kaufmännisches Und nicht maskiert: {out}");
        assert_eq!(esc_html("Vertrag Müller.docx"), "Vertrag Müller.docx");
    }

    #[test]
    fn token_plausible_rejects_the_weak_ones() {
        assert!(token_plausible("0123456789abcdef0123456789abcdef"));
        assert!(!token_plausible(""));
        // 31 Zeichen: einer zu wenig
        assert!(!token_plausible("0123456789abcdef0123456789abcde"));
        // richtige Länge, aber kein Hex — etwa ein durchgereichter Pfad
        assert!(!token_plausible("../../etc/passwd________________"));
        assert!(!token_plausible("0123456789abcdef0123456789abcde/"));
    }

    #[test]
    fn sign_stop_clears_the_session_and_runs_twice() {
        let state = SignServer::default();
        // Auf die Rückrufadresse binden, nicht auf 0.0.0.0: sonst fragt die
        // Firewall des Entwicklungsrechners bei jedem Testlauf nach.
        let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        *state.session.lock().unwrap() = Some(SignSession {
            id: 1,
            server,
            stop: stop.clone(),
        });

        sign_stop(&state);
        assert!(state.session.lock().unwrap().is_none(), "Sitzung nicht geräumt");
        // Die Flagge ist das, woran der wartende Thread den Abbruch erkennt.
        // Ohne sie liefe er bis zum Ablauf der Frist weiter und hielte den Port.
        assert!(stop.load(Ordering::SeqCst), "Abbruch nicht angezeigt");

        sign_stop(&state); // idempotent wie wipe_staging
    }

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
        let _guard = staging_guard();
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

        let _guard = staging_guard();

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

    /// Deckt den Aufräumschritt ab, den der Fenster-Handler beim Schließen
    /// des Hauptfensters auslöst. Das Tauri-Ereignis selbst lässt sich hier
    /// nicht nachstellen; geprüft wird der Teil, der die Zusage trägt: der
    /// Ordner ist danach weg, und ein zweiter Aufruf — den es gibt, weil auch
    /// `RunEvent::Exit` aufräumt — greift ins Leere, statt zu scheitern.
    #[test]
    fn write_over_replaces_the_file_and_leaves_nothing_behind() {
        let dir = std::env::temp_dir().join(format!(
            "docfill-test-{}",
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("vorlage.docx");
        fs::write(&file, b"alt").unwrap();

        write_over(&file, b"neu mit Schluesseln").unwrap();

        assert_eq!(fs::read(&file).unwrap(), b"neu mit Schluesseln");
        // die Zwischendatei darf nicht liegen bleiben
        assert!(!dir.join("vorlage.docfill-neu").exists());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);

        // und noch einmal darüber, auf eine bestehende Datei
        write_over(&file, b"noch neuer").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"noch neuer");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn wipe_staging_removes_the_folder_and_runs_twice() {
        let _guard = staging_guard();
        let dir = staging_dir();

        let staged = stage("leftover.docx", b"personal data").unwrap();
        assert!(staged.exists(), "nothing was staged, the test would prove nothing");

        wipe_staging();
        assert!(!dir.exists(), "staging folder survived cleanup: {}", dir.display());

        // beide Wege nach draußen rufen wipe_staging; der zweite darf nicht stolpern
        wipe_staging();
        assert!(!dir.exists(), "second cleanup recreated the folder");
    }
}
