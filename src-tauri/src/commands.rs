//! Tauri command surface (API.md §3): thin wrappers over module functions; the only UI → core entry point.
//! Arg names are the API.md names; Tauri sends them camelCase (`from_id` ↔ `fromId`, `r#type` ↔ `type`).
//! Every command runs off the main thread (`async`): DB work never blocks the window, and the
//! blocking pickers (API.md §1) are never called on the main thread. The db lock is never held
//! across a picker or a network wait.
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_dialog::{DialogExt, FilePath};

use crate::backup::{self, Backup, RestoreCheck};
use crate::capture::{CaptureInfo, Listener};
use crate::cleanup;
use crate::error::{AppError, Result};
use crate::export;
use crate::library::dashboard::{self, Counts, Dashboard};
use crate::library::filters::{self, ItemFilter, SavedFilter};
use crate::library::items::{self, ItemDetail, ItemPatch, ItemSummary, NewItem};
use crate::library::rediscover::{self, Rediscover};
use crate::library::search::{self, SearchHit};
use crate::library::tags::{self, Tag};
use crate::library::{ItemType, LinkKind, Page, Status, backlog, links as item_links, status};
use crate::links::{self, CaptureSource, Parsed, Unresolved};
use crate::logger::{self, Logger, LoggerPatch, LoggerStatus};
use crate::net;
use crate::settings::{self, Settings};
use crate::state::AppState;
use crate::text::{clean_line, limit};
use crate::usage::{self, SuccessMetrics};

type St<'a> = State<'a, AppState>;

/// Every API.md §3 command. `scripts/check.py` reads this list against API.md and `src/api.ts`.
pub fn handler<R: Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        get_counts,
        get_dashboard,
        list_items,
        get_item,
        create_item,
        update_item,
        set_status,
        delete_item,
        link_items,
        unlink_items,
        reorder_backlog,
        parse_link,
        resolve_link,
        prefill_title,
        list_unresolved,
        retry_unresolved,
        discard_unresolved,
        search,
        list_tags,
        rename_tag,
        merge_tags,
        delete_tag,
        list_saved_filters,
        save_filter,
        delete_saved_filter,
        get_rediscover,
        get_online,
        get_settings,
        update_settings,
        get_capture,
        set_capture_enabled,
        new_pairing_code,
        export_library,
        list_backups,
        backup_now,
        pick_backup,
        check_backup,
        restore_backup,
        choose_backup_dir,
        move_library,
        delete_library,
        record_page_visit,
        get_success_metrics,
        get_logger_status,
        set_logger,
        pause_logger,
        resume_logger,
    ]
}

fn path_of(p: Option<FilePath>) -> Option<PathBuf> {
    p.and_then(|p| p.into_path().ok())
}

fn settings_now(st: &AppState) -> Settings {
    st.settings.lock().unwrap().clone()
}

/* ------------------------------------------------------------------ library */

#[tauri::command(async)]
pub fn get_counts(st: St<'_>) -> Result<Counts> {
    dashboard::get_counts(&st.db.lock().unwrap())
}

#[tauri::command(async)]
pub fn get_dashboard(st: St<'_>) -> Result<Dashboard> {
    dashboard::get_dashboard(&st.db.lock().unwrap())
}

#[tauri::command(async)]
pub fn list_items(st: St<'_>, filter: ItemFilter) -> Result<Page<ItemSummary>> {
    filters::list_items(&st.db.lock().unwrap(), &filter)
}

#[tauri::command(async)]
pub fn get_item(st: St<'_>, id: i64) -> Result<ItemDetail> {
    items::get(&st.db.lock().unwrap(), id)
}

/// Free text in `input` is cleaned by `items::create` (text.rs).
#[tauri::command(async)]
pub fn create_item(st: St<'_>, input: NewItem) -> Result<ItemDetail> {
    items::create(&mut st.db.lock().unwrap(), input)
}

/// Free text in `patch` is cleaned by `items::update` (text.rs).
#[tauri::command(async)]
pub fn update_item(st: St<'_>, id: i64, patch: ItemPatch) -> Result<ItemDetail> {
    items::update(&mut st.db.lock().unwrap(), id, patch)
}

#[tauri::command(async)]
pub fn set_status(st: St<'_>, id: i64, status: Status, note: Option<String>) -> Result<ItemDetail> {
    status::set_status(&mut st.db.lock().unwrap(), id, status, note.as_deref())
}

#[tauri::command(async)]
pub fn delete_item(st: St<'_>, id: i64) -> Result<()> {
    items::delete_item(&st.db.lock().unwrap(), id)
}

#[tauri::command(async)]
pub fn link_items(st: St<'_>, from_id: i64, to_id: i64, kind: LinkKind) -> Result<()> {
    // Several writes (old album link out, new in, touch): all or nothing (NFR-05).
    let mut c = st.db.lock().unwrap();
    let tx = c.transaction()?;
    item_links::link(&tx, from_id, to_id, kind)?;
    Ok(tx.commit()?)
}

#[tauri::command(async)]
pub fn unlink_items(st: St<'_>, from_id: i64, to_id: i64, kind: LinkKind) -> Result<()> {
    // Several writes (old album link out, new in, touch): all or nothing (NFR-05).
    let mut c = st.db.lock().unwrap();
    let tx = c.transaction()?;
    item_links::unlink(&tx, from_id, to_id, kind)?;
    Ok(tx.commit()?)
}

#[tauri::command(async)]
pub fn reorder_backlog(st: St<'_>, ids: Vec<i64>) -> Result<()> {
    backlog::reorder_backlog(&mut st.db.lock().unwrap(), &ids)
}

/* ------------------------------------------------------------------ links */

#[tauri::command(async)]
pub fn parse_link(st: St<'_>, raw: String) -> Result<Parsed> {
    let p = links::parse(&clean_line(&raw, limit::LINK));
    links::with_existing(&st.db.lock().unwrap(), p)
}

/// API.md `resolve_link` result.
#[derive(Debug, Serialize)]
pub struct Resolved {
    pub uri: String,
    pub r#type: ItemType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_id: Option<i64>,
}

#[tauri::command]
pub async fn resolve_link(st: St<'_>, raw: String) -> Result<Resolved> {
    let raw = clean_line(&raw, limit::LINK);
    let (uri, kind) = links::resolve_or_queue(&st.db, &raw, CaptureSource::Paste).await?;
    let existing_id = items::id_by_uri(&st.db.lock().unwrap(), &uri)?;
    Ok(Resolved {
        uri,
        r#type: kind,
        existing_id,
    })
}

#[tauri::command]
pub async fn prefill_title(uri: String) -> Result<Value> {
    let title = links::prefill_title(&clean_line(&uri, limit::LINK)).await?;
    Ok(json!({ "title": title }))
}

#[tauri::command(async)]
pub fn list_unresolved(st: St<'_>) -> Result<Vec<Unresolved>> {
    links::list_unresolved(&st.db.lock().unwrap())
}

/// After a retry: `capture-received` per new item (queued phone shares finish here, TSD §5.1).
pub fn emit_resolved<R: Runtime>(app: &AppHandle<R>, st: &AppState, new: &[i64]) {
    for &id in new {
        if let Ok(item) = items::summary(&st.db.lock().unwrap(), id) {
            let _ = app.emit("capture-received", item);
        }
    }
}

#[tauri::command]
pub async fn retry_unresolved<R: Runtime>(app: AppHandle<R>, st: St<'_>) -> Result<Value> {
    let (n, new) = links::retry_unresolved(&st.db).await?;
    emit_resolved(&app, &st, &new);
    Ok(json!({ "resolved": n }))
}

#[tauri::command(async)]
pub fn discard_unresolved(st: St<'_>, id: i64) -> Result<()> {
    links::discard_unresolved(&st.db.lock().unwrap(), id)
}

/* ------------------------------------------------------------------ search, tags, filters */

/// `q` is cleaned by `search::search`.
#[tauri::command(async)]
pub fn search(
    st: St<'_>,
    q: String,
    limit: Option<i64>,
    r#type: Option<ItemType>,
) -> Result<Vec<SearchHit>> {
    search::search(&st.db.lock().unwrap(), &q, limit, r#type)
}

#[tauri::command(async)]
pub fn list_tags(st: St<'_>) -> Result<Vec<Tag>> {
    tags::list_tags(&st.db.lock().unwrap())
}

#[tauri::command(async)]
pub fn rename_tag(st: St<'_>, id: i64, name: String) -> Result<Tag> {
    tags::rename_tag(&mut st.db.lock().unwrap(), id, &name)
}

#[tauri::command(async)]
pub fn merge_tags(st: St<'_>, from_id: i64, into_id: i64) -> Result<Tag> {
    tags::merge_tags(&mut st.db.lock().unwrap(), from_id, into_id)
}

#[tauri::command(async)]
pub fn delete_tag(st: St<'_>, id: i64) -> Result<()> {
    tags::delete_tag(&st.db.lock().unwrap(), id)
}

#[tauri::command(async)]
pub fn list_saved_filters(st: St<'_>) -> Result<Vec<SavedFilter>> {
    filters::list_saved_filters(&st.db.lock().unwrap())
}

#[tauri::command(async)]
pub fn save_filter(st: St<'_>, name: String, definition: ItemFilter) -> Result<SavedFilter> {
    filters::save_filter(&st.db.lock().unwrap(), &name, definition)
}

#[tauri::command(async)]
pub fn delete_saved_filter(st: St<'_>, id: i64) -> Result<()> {
    filters::delete_saved_filter(&st.db.lock().unwrap(), id)
}

#[tauri::command(async)]
pub fn get_rediscover(st: St<'_>) -> Result<Rediscover> {
    rediscover::get_rediscover(&st.db.lock().unwrap())
}

/* ------------------------------------------------------------------ net, settings, capture */

#[tauri::command]
pub async fn get_online() -> Result<bool> {
    Ok(net::online().await)
}

#[tauri::command(async)]
pub fn get_settings(st: St<'_>) -> Result<Settings> {
    Ok(settings_now(&st))
}

/// `library_path` and `backup_dir` are refused by `settings::merge` (Rust's pickers set them).
#[tauri::command(async)]
pub fn update_settings(st: St<'_>, patch: Value) -> Result<Settings> {
    let mut cur = st.settings.lock().unwrap();
    let next = settings::merge(&cur, &patch)?;
    settings::save(&st.paths.settings_file(), &next)?;
    *cur = next.clone();
    Ok(next)
}

#[tauri::command(async)]
pub fn get_capture(cap: State<'_, Listener>) -> Result<CaptureInfo> {
    Ok(cap.info())
}

#[tauri::command(async)]
pub fn set_capture_enabled(cap: State<'_, Listener>, on: bool) -> Result<CaptureInfo> {
    cap.set_enabled(on)
}

#[tauri::command(async)]
pub fn new_pairing_code(cap: State<'_, Listener>) -> Result<CaptureInfo> {
    cap.new_pairing_code()
}

/* ------------------------------------------------------------------ export, backup, files */

#[tauri::command(async)]
pub fn export_library<R: Runtime>(app: AppHandle<R>, st: St<'_>) -> Result<Option<Value>> {
    let name = format!(
        "runout-library-{}.json",
        chrono::Local::now().format("%Y%m%d")
    );
    let picked = app
        .dialog()
        .file()
        .add_filter("JSON", &["json"])
        .set_file_name(name)
        .blocking_save_file();
    let Some(path) = path_of(picked) else {
        return Ok(None);
    };
    let count = export::export_library(&st.db.lock().unwrap(), &path)?;
    Ok(Some(
        json!({ "path": path.to_string_lossy(), "count": count }),
    ))
}

#[tauri::command(async)]
pub fn list_backups(st: St<'_>) -> Result<Vec<Backup>> {
    let s = settings_now(&st);
    Ok(backup::list_backups(
        Path::new(&s.backup_dir),
        s.size_warn_mb,
    ))
}

#[tauri::command(async)]
pub fn backup_now<R: Runtime>(app: AppHandle<R>, st: St<'_>) -> Result<Backup> {
    let s = settings_now(&st);
    let b = backup::backup_now(
        &st.db.lock().unwrap(),
        Path::new(&s.backup_dir),
        s.size_warn_mb,
    )?;
    let _ = app.emit("backup-finished", &b);
    Ok(b)
}

#[tauri::command(async)]
pub fn pick_backup<R: Runtime>(app: AppHandle<R>, st: St<'_>) -> Result<Option<String>> {
    let dir = settings_now(&st).backup_dir;
    let picked = app
        .dialog()
        .file()
        .add_filter("Runout backup", &["zip"])
        .set_directory(dir)
        .blocking_pick_file();
    Ok(path_of(picked).map(|p| p.to_string_lossy().into_owned()))
}

/// The UI only sends a path `pick_backup` returned; anything but an existing .zip is refused.
fn zip_arg(path: &str) -> Result<PathBuf> {
    let p = PathBuf::from(path);
    let is_zip = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if !is_zip || !p.is_file() {
        return Err(AppError::validation("Choose a Runout backup (.zip)."));
    }
    Ok(p)
}

#[tauri::command(async)]
pub fn check_backup(st: St<'_>, path: String) -> Result<RestoreCheck> {
    let _db = st.db.lock().unwrap(); // lock order db → settings, as move_library: path can't move
    let lib = settings_now(&st).library_path;
    backup::check_backup(&zip_arg(&path)?, Path::new(&lib))
}

#[tauri::command(async)]
pub fn restore_backup(st: St<'_>, path: String) -> Result<RestoreCheck> {
    let zip = zip_arg(&path)?;
    let mut conn = st.db.lock().unwrap(); // db → settings, as move_library
    let lib = settings_now(&st).library_path;
    backup::restore_locked(&mut conn, Path::new(&lib), &zip)
}

#[tauri::command(async)]
pub fn choose_backup_dir<R: Runtime>(app: AppHandle<R>, st: St<'_>) -> Result<Option<String>> {
    let Some(dir) = path_of(app.dialog().file().blocking_pick_folder()) else {
        return Ok(None);
    };
    let dir = dir.to_string_lossy().into_owned();
    let mut cur = st.settings.lock().unwrap();
    let next = Settings {
        backup_dir: dir.clone(),
        ..cur.clone()
    };
    settings::save(&st.paths.settings_file(), &next)?;
    *cur = next;
    Ok(Some(dir))
}

/// The library goes to `<picked folder>\library.db` (`state::AppState::move_library`).
#[tauri::command(async)]
pub fn move_library<R: Runtime>(app: AppHandle<R>, st: St<'_>) -> Result<Option<String>> {
    let Some(dir) = path_of(app.dialog().file().blocking_pick_folder()) else {
        return Ok(None);
    };
    st.move_library(&dir).map(Some)
}

#[tauri::command(async)]
pub fn delete_library(st: St<'_>, confirm: String) -> Result<()> {
    let mut conn = st.db.lock().unwrap(); // db → settings, as move_library
    let lib = settings_now(&st).library_path;
    cleanup::delete_library(&mut conn, Path::new(&lib), &confirm)
}

/* ------------------------------------------------------------------ usage */

/// `page` is checked against `[a-z0-9-]{1,40}` by `usage::record_page_visit`.
#[tauri::command(async)]
pub fn record_page_visit(st: St<'_>, page: String) -> Result<()> {
    usage::record_page_visit(&st.db.lock().unwrap(), &page)
}

#[tauri::command(async)]
pub fn get_success_metrics(st: St<'_>) -> Result<SuccessMetrics> {
    usage::get_success_metrics(&st.db.lock().unwrap())
}

/* ------------------------------------------------------------------ logger (API.md §4) */

type Lg<'a> = State<'a, std::sync::Mutex<Logger>>;

#[tauri::command(async)]
pub fn get_logger_status(st: St<'_>, lg: Lg<'_>) -> Result<LoggerStatus> {
    let lg = lg.lock().unwrap(); // lock order logger → db → settings (logger::Logger)
    let db = st.db.lock().unwrap();
    lg.status(&db, &settings_now(&st), chrono::Utc::now())
}

/// Apply `f` to the settings; when that stops logging, the current play is closed first (written
/// if ≥ 1 s), so paused or off time writes nothing. Emits `play-logged` / `logger-status`.
fn change_logger<R: Runtime>(
    app: &AppHandle<R>,
    st: &AppState,
    lg: &std::sync::Mutex<Logger>,
    f: impl FnOnce(&mut Settings) -> Result<()>,
) -> Result<LoggerStatus> {
    let mut lg = lg.lock().unwrap();
    let db = st.db.lock().unwrap();
    let now = chrono::Utc::now();
    let mut s = settings_now(st);
    f(&mut s)?;
    if !logger::active(&s, now)
        && let Some(id) = lg.close(&db, now)?
    {
        let _ = app.emit("play-logged", logger::play(&db, id)?);
    }
    settings::save(&st.paths.settings_file(), &s)?;
    *st.settings.lock().unwrap() = s.clone();
    let status = lg.status(&db, &s, now)?;
    let _ = app.emit("logger-status", &status);
    Ok(status)
}

#[tauri::command(async)]
pub fn set_logger<R: Runtime>(
    app: AppHandle<R>,
    st: St<'_>,
    lg: Lg<'_>,
    patch: LoggerPatch,
) -> Result<LoggerStatus> {
    if let Some(on) = patch.autostart {
        // absent only in the mock-runtime tests, which don't register the plugin
        if let Some(a) = app.try_state::<tauri_plugin_autostart::AutoLaunchManager>() {
            if on { a.enable() } else { a.disable() }.map_err(|e| {
                AppError::new(crate::error::Code::Io, "Couldn't change start with Windows.")
                    .with_detail(e.to_string())
            })?;
        }
    }
    change_logger(&app, &st, &lg, |s| {
        s.logger_on = patch.on.unwrap_or(s.logger_on);
        s.autostart = patch.autostart.unwrap_or(s.autostart);
        s.include_private = patch.include_private.unwrap_or(s.include_private);
        Ok(())
    })
}

/// `minutes`: 1 minute to 7 days, or `null` = until resumed (`logger::pause`).
#[tauri::command(async)]
pub fn pause_logger<R: Runtime>(
    app: AppHandle<R>,
    st: St<'_>,
    lg: Lg<'_>,
    minutes: Option<i64>,
) -> Result<LoggerStatus> {
    change_logger(&app, &st, &lg, |s| {
        logger::pause(s, minutes, chrono::Utc::now())
    })
}

#[tauri::command(async)]
pub fn resume_logger<R: Runtime>(app: AppHandle<R>, st: St<'_>, lg: Lg<'_>) -> Result<LoggerStatus> {
    change_logger(&app, &st, &lg, |s| {
        logger::resume(s);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets};
    use tauri::{Manager, WebviewWindowBuilder};

    fn app() -> (tauri::App<tauri::test::MockRuntime>, PathBuf) {
        let d = std::env::temp_dir().join(format!(
            "runout-cmd-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        let paths = crate::state::Paths {
            config_dir: d.join("cfg"),
            data_dir: d.clone(),
            local_data_dir: d.join("local"),
        };
        let settings = Settings::defaults(&d, &d);
        let st = AppState {
            db: Mutex::new(crate::db::open_in_memory().unwrap()),
            paths,
            settings: Mutex::new(settings),
        };
        let app = mock_builder()
            .invoke_handler(handler())
            .build(mock_context(noop_assets()))
            .unwrap();
        app.manage(st);
        app.manage(Mutex::new(Logger::default()));
        (app, d)
    }

    fn call(
        w: &tauri::WebviewWindow<tauri::test::MockRuntime>,
        cmd: &str,
        args: Value,
    ) -> std::result::Result<Value, Value> {
        get_ipc_response(
            w,
            tauri::webview::InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                // WebView2's app origin (not an outbound host, so not in the TSD §7 allow-list).
                url: format!("{}://tauri.localhost", "http").parse().unwrap(),
                body: InvokeBody::Json(args),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|b| b.deserialize::<Value>().unwrap())
    }

    /// Round trip with the exact keys `src/api.ts` sends (camelCase multi-word args, `type`).
    #[test]
    fn invoke_keys_match_api_ts() {
        let (app, d) = app();
        let w = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let new = |name: &str, kind: &str, uri: Value| json!({"input": {"type": kind, "display_name": name, "spotify_uri": uri, "provenance": "user"}});
        let uri = json!("spotify:album:SyntheticAlbumId000001");
        let album = call(
            &w,
            "create_item",
            new("Synthetic Album", "album", uri.clone()),
        )
        .unwrap();
        let track = call(
            &w,
            "create_item",
            new("Synthetic Song", "track", Value::Null),
        )
        .unwrap();
        let counts = call(&w, "get_counts", json!({})).unwrap();
        assert_eq!(
            (counts["library"].clone(), counts["albums"].clone()),
            (json!(2), json!(1))
        );
        let (a, t) = (album["id"].clone(), track["id"].clone());
        call(
            &w,
            "link_items",
            json!({"fromId": t, "toId": a, "kind": "on_album"}),
        )
        .unwrap();
        assert_eq!(
            call(&w, "get_item", json!({"id": t})).unwrap()["album_name"],
            json!("Synthetic Album")
        );
        let hits = call(&w, "search", json!({"q": "synthetic", "type": "track"})).unwrap();
        assert_eq!(hits.as_array().unwrap().len(), 1);
        assert_eq!(hits[0]["item"]["id"], t);
        let p = call(&w, "parse_link", json!({"raw": "not a link"})).unwrap();
        assert_eq!(p["state"], json!("invalid"));
        // errors reach the UI as the API.md AppError object
        let e = call(&w, "create_item", new("Other", "album", uri)).unwrap_err();
        assert_eq!(e["code"], json!("duplicate"));
        assert_eq!(e["existing_id"], a);
        let e = call(&w, "get_item", json!({"id": 999})).unwrap_err();
        assert_eq!(e["code"], json!("not_found"));
        for key in ["library_path", "backup_dir"] {
            let e = call(&w, "update_settings", json!({"patch": { key: "x" }})).unwrap_err();
            assert_eq!(e["code"], json!("validation"), "{key}");
        }
        // logger: single-word keys `minutes` / `patch`; nothing in this test runs SMTC → idle
        let lg = call(&w, "pause_logger", json!({"minutes": 60})).unwrap();
        assert_eq!(lg["state"], json!("paused"));
        assert!(lg["paused_until"].is_string());
        let lg = call(&w, "set_logger", json!({"patch": {"include_private": true}})).unwrap();
        assert_eq!(lg["include_private"], json!(true));
        let lg = call(&w, "resume_logger", json!({})).unwrap();
        assert_eq!((&lg["state"], &lg["paused_until"]), (&json!("idle"), &Value::Null));
        let e = call(&w, "set_logger", json!({"patch": {"nope": true}})).unwrap_err();
        assert_eq!(e["code"], json!("validation"), "unknown patch key");
        let e = call(&w, "pause_logger", json!({"minutes": 0})).unwrap_err();
        assert_eq!(e["code"], json!("validation"));
        let lg = call(&w, "get_logger_status", json!({})).unwrap();
        assert_eq!((&lg["autostart"], &lg["now_playing"]), (&json!(false), &Value::Null));
        let e = call(&w, "check_backup", json!({"path": "C:\\nope.txt"})).unwrap_err();
        assert_eq!(e["code"], json!("validation"));
        drop(w);
        drop(app);
        let _ = std::fs::remove_dir_all(d);
    }
}
