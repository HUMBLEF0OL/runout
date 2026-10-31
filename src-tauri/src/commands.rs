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
use crate::importer::playlists::{self, ExportedPlaylist, PlaylistCheckup};
use crate::importer::preview::{self, ImportPreview};
use crate::importer::run::{self, ImportRecord, ImportResult};
use crate::insights::collection::{self, CollectionInsights};
use crate::insights::explore::{DataCoverage, ExploreQuery, ExploreResult, ExploreView};
use crate::insights::habits::{self, Discovery, Habits};
use crate::insights::history::{self, Measure, Records, Trends};
use crate::insights::listening::{self, ListeningInsights, YearReview};
use crate::insights::plays::{self, ItemListening, Play, PlayQuery, Unmatched};
use crate::insights::report::{self, PeriodReport};
use crate::insights::take::{self, TakeInsights};
use crate::insights::v3::{self, PeriodQuery};
use crate::insights::{self, Covered, Freshness, Range, Sourced};
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
use crate::spotify::autofill::{self as sp_autofill, Autofill, NewTrack};
use crate::spotify::create::{self as sp_create, CreatedPlaylist};
use crate::spotify::discover::{self as sp_discover, RecentPlay, SpotifyEntry, TopKind, TopRange};
use crate::spotify::inbox::{self as sp_inbox, Checked, InboxStatus};
use crate::spotify::library_sync as sp_library;
use crate::spotify::studio::{
    self as sp_studio, Applied, ApplyRecord, Draft, SpotifyPlaylist, StudioDiff,
};
use crate::spotify::{self, Spotify, SpotifyStatus};
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
        pick_import_source,
        preview_import,
        run_import,
        list_imports,
        list_exported_playlists,
        get_playlist_checkup,
        list_plays,
        list_unmatched,
        link_play,
        get_item_listening,
        get_freshness,
        get_listening_insights,
        get_collection_insights,
        get_take_insights,
        get_year_review,
        get_period_report,
        get_trends,
        get_discovery,
        get_listening_habits,
        get_genres_decades,
        set_music_db,
        get_records,
        explore,
        get_data_coverage,
        list_explore_views,
        save_explore_view,
        delete_explore_view,
        get_spotify_status,
        connect_spotify,
        disconnect_spotify,
        list_spotify_playlists,
        load_studio_draft,
        preview_studio_apply,
        apply_studio_draft,
        get_last_apply,
        undo_last_apply,
        create_playlist_from_filter,
        create_playlist_from_items,
        autofill,
        bulk_add_album_tracks,
        get_top_items,
        get_recent_plays_api,
        library_contains,
        save_to_spotify_library,
        remove_from_spotify_library,
        find_cross_playlist_dupes,
        get_inbox,
        create_inbox,
        check_inbox,
        set_inbox_options,
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
    items::detail(&st.db.lock().unwrap(), id, private(&st))
}

/// A mutation's returned detail counts track-list plays without private sessions; re-read it
/// when the setting is on so it matches `get_item` (TSD §4.8).
fn with_private(st: &AppState, conn: &rusqlite::Connection, d: ItemDetail) -> Result<ItemDetail> {
    if d.track_list.is_some() && private(st) {
        items::detail(conn, d.summary.id, true)
    } else {
        Ok(d)
    }
}

/// Free text in `input` is cleaned by `items::create` (text.rs).
#[tauri::command(async)]
pub fn create_item(st: St<'_>, input: NewItem) -> Result<ItemDetail> {
    let mut db = st.db.lock().unwrap();
    let d = items::create(&mut db, input)?;
    with_private(&st, &db, d)
}

/// Free text in `patch` is cleaned by `items::update` (text.rs).
#[tauri::command(async)]
pub fn update_item(st: St<'_>, id: i64, patch: ItemPatch) -> Result<ItemDetail> {
    let mut db = st.db.lock().unwrap();
    let d = items::update(&mut db, id, patch)?;
    with_private(&st, &db, d)
}

#[tauri::command(async)]
pub fn set_status(st: St<'_>, id: i64, status: Status, note: Option<String>) -> Result<ItemDetail> {
    let mut db = st.db.lock().unwrap();
    let d = status::set_status(&mut db, id, status, note.as_deref())?;
    with_private(&st, &db, d)
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
pub fn restore_backup(st: St<'_>, lg: Lg<'_>, path: String) -> Result<RestoreCheck> {
    let zip = zip_arg(&path)?;
    let mut lg = lg.lock().unwrap(); // logger → db → settings: the logger run can't straddle the swap
    let mut conn = st.db.lock().unwrap();
    let lib = settings_now(&st).library_path;
    lg.swap_db(&mut conn, chrono::Utc::now(), |c| {
        backup::restore_locked(c, Path::new(&lib), &zip)
    })
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
pub fn delete_library(st: St<'_>, lg: Lg<'_>, confirm: String) -> Result<()> {
    let mut lg = lg.lock().unwrap(); // logger → db → settings, as restore_backup
    let mut conn = st.db.lock().unwrap();
    let lib = settings_now(&st).library_path;
    lg.swap_db(&mut conn, chrono::Utc::now(), |c| {
        cleanup::delete_library(c, Path::new(&lib), &confirm)
    })
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
/// if ≥ 1 s), so paused or off time writes nothing. Emits `play-logged` / `logger-status` (and
/// updates the tray). Also the tray's pause/resume path.
pub(crate) fn change_logger<R: Runtime>(
    app: &AppHandle<R>,
    st: &AppState,
    lg: &std::sync::Mutex<Logger>,
    f: impl FnOnce(&mut Settings) -> Result<()>,
) -> Result<LoggerStatus> {
    let mut lg = lg.lock().unwrap();
    let db = st.db.lock().unwrap();
    let now = chrono::Utc::now();
    // one settings guard from read to write (lock order logger -> db -> settings)
    let (logged, s) = st.edit_settings(|s| {
        f(s)?;
        Ok(if logger::active(s, now) {
            None
        } else {
            lg.close(&db, now)?
        })
    })?;
    lg.sync_run(&db, &s, now)?;
    if let Some(id) = logged {
        let _ = app.emit("play-logged", logger::play(&db, id)?);
    }
    let status = lg.status(&db, &s, now)?;
    crate::tray::emit_status(app, &status);
    Ok(status)
}

#[tauri::command(async)]
pub fn set_logger<R: Runtime>(
    app: AppHandle<R>,
    st: St<'_>,
    lg: Lg<'_>,
    patch: Value,
) -> Result<LoggerStatus> {
    // parsed here so a bad key is a `validation` error, not Tauri's plain deserialize string
    let patch: LoggerPatch = serde_json::from_value(patch)
        .map_err(|_| AppError::validation("Unknown or mistyped logger setting."))?;
    if let Some(on) = patch.autostart {
        // absent only in the mock-runtime tests, which don't register the plugin
        if let Some(a) = app.try_state::<tauri_plugin_autostart::AutoLaunchManager>() {
            if on { a.enable() } else { a.disable() }.map_err(|e| {
                AppError::new(
                    crate::error::Code::Io,
                    "Couldn't change start with Windows.",
                )
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
pub fn resume_logger<R: Runtime>(
    app: AppHandle<R>,
    st: St<'_>,
    lg: Lg<'_>,
) -> Result<LoggerStatus> {
    change_logger(&app, &st, &lg, |s| {
        logger::resume(s);
        Ok(())
    })
}

/* ------------------------------------------------------------------ import (API.md §4) */

/// One picker for both forms: the export .zip, or any .json inside the unzipped folder (→ that folder).
#[tauri::command(async)]
pub fn pick_import_source<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>> {
    let picked = app
        .dialog()
        .file()
        .set_title("Choose the Spotify export (.zip) or any file in its folder")
        .add_filter("Spotify export", &["zip", "json"])
        .blocking_pick_file();
    Ok(path_of(picked).map(|p| {
        let is_zip = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"));
        let p = match p.parent() {
            Some(dir) if !is_zip => dir.to_path_buf(),
            _ => p,
        };
        p.to_string_lossy().into_owned()
    }))
}

#[tauri::command(async)]
pub fn preview_import(st: St<'_>, path: String) -> Result<ImportPreview> {
    let l = run::load(Path::new(&path))?; // files read before the lock
    preview::preview(&mut st.db.lock().unwrap(), &l)
}

#[tauri::command(async)]
pub fn run_import(st: St<'_>, path: String) -> Result<ImportResult> {
    let l = run::load(Path::new(&path))?;
    run::run(&mut st.db.lock().unwrap(), &l)
}

#[tauri::command(async)]
pub fn list_imports(st: St<'_>) -> Result<Vec<ImportRecord>> {
    run::list_imports(&st.db.lock().unwrap())
}

#[tauri::command(async)]
pub fn list_exported_playlists(st: St<'_>) -> Result<Vec<ExportedPlaylist>> {
    playlists::list(&st.db.lock().unwrap())
}

#[tauri::command(async)]
pub fn get_playlist_checkup(st: St<'_>, id: i64) -> Result<PlaylistCheckup> {
    playlists::checkup(&st.db.lock().unwrap(), id, private(&st))
}

/* ------------------------------------------------------------- R2 insights */

fn private(st: &AppState) -> bool {
    settings_now(st).include_private
}

#[tauri::command(async)]
pub fn list_plays(st: St<'_>, q: PlayQuery) -> Result<Page<Play>> {
    plays::list_plays(&st.db.lock().unwrap(), &q, private(&st), chrono::Utc::now())
}

#[tauri::command(async)]
pub fn list_unmatched(st: St<'_>) -> Result<Vec<Unmatched>> {
    plays::list_unmatched(&st.db.lock().unwrap(), private(&st))
}

#[tauri::command(async)]
pub fn link_play(st: St<'_>, event_id: i64, item_id: i64) -> Result<()> {
    plays::link_play(&st.db.lock().unwrap(), event_id, item_id).map(drop)
}

#[tauri::command(async)]
pub fn get_item_listening(st: St<'_>, id: i64) -> Result<Sourced<ItemListening>> {
    plays::item_listening(&st.db.lock().unwrap(), id, private(&st), chrono::Utc::now())
}

#[tauri::command(async)]
pub fn get_freshness(st: St<'_>) -> Result<Freshness> {
    insights::freshness(&st.db.lock().unwrap())
}

#[tauri::command(async)]
pub fn get_listening_insights(st: St<'_>, range: Range) -> Result<Covered<ListeningInsights>> {
    listening::listening(
        &st.db.lock().unwrap(),
        range,
        private(&st),
        chrono::Utc::now(),
    )
}

#[tauri::command(async)]
pub fn get_collection_insights(st: St<'_>, range: Range) -> Result<Sourced<CollectionInsights>> {
    collection::collection(&st.db.lock().unwrap(), range, chrono::Utc::now())
}

#[tauri::command(async)]
pub fn get_take_insights(st: St<'_>) -> Result<Sourced<TakeInsights>> {
    take::take(&st.db.lock().unwrap(), private(&st), chrono::Utc::now())
}

#[tauri::command(async)]
pub fn get_year_review(st: St<'_>, year: i32) -> Result<Sourced<YearReview>> {
    if !(1900..=9999).contains(&year) {
        return Err(AppError::validation("Pick a year between 1900 and 9999."));
    }
    listening::year_review(&st.db.lock().unwrap(), year, private(&st))
}

/* ------------------------------------------------------------- Insights v3 (API.md §4.1, M10) */
// Local time zone, now; every arg is validated again in `insights` (TSD §7).

fn v3<T>(st: &AppState, f: impl FnOnce(&v3::Ctx<chrono::Local>) -> Result<T>) -> Result<T> {
    let include_private = private(st);
    let conn = st.db.lock().unwrap();
    f(&v3::Ctx {
        conn: &conn,
        include_private,
        tz: &chrono::Local,
        now: chrono::Utc::now(),
    })
}

#[tauri::command(async)]
pub fn get_period_report(st: St<'_>, period: PeriodQuery) -> Result<Covered<PeriodReport>> {
    v3(&st, |cx| report::report(cx, &period))
}

#[tauri::command(async)]
pub fn get_trends(st: St<'_>, measure: Measure) -> Result<Covered<Trends>> {
    v3(&st, |cx| history::trends(cx, measure))
}

#[tauri::command(async)]
pub fn get_discovery(st: St<'_>, period: PeriodQuery) -> Result<Covered<Discovery>> {
    v3(&st, |cx| habits::discovery(cx, &period))
}

#[tauri::command(async)]
pub fn get_listening_habits(st: St<'_>, period: PeriodQuery) -> Result<Covered<Habits>> {
    v3(&st, |cx| habits::habits(cx, &period))
}

/// Genres & decades (M10 WI 9, source M): labels from the `enrich` tables on S/E listening time.
#[tauri::command(async)]
pub fn get_genres_decades(
    st: St<'_>,
    period: PeriodQuery,
) -> Result<Covered<crate::enrich::genres::GenresDecades>> {
    v3(&st, |cx| crate::enrich::genres::genres_decades(cx, &period))
}

/// The music database switch (TSD §5.4): off stops lookups and deletes every `enrich` row.
#[tauri::command(async)]
pub fn set_music_db(st: St<'_>, on: bool) -> Result<Settings> {
    let (_, s) = st.edit_settings(|s| {
        s.music_db = on;
        Ok(())
    })?;
    crate::enrich::set_switch(&mut st.db.lock().unwrap(), on)?;
    Ok(s)
}

#[tauri::command(async)]
pub fn get_records(st: St<'_>) -> Result<Covered<Records>> {
    v3(&st, history::records)
}

#[tauri::command(async)]
pub fn explore(st: St<'_>, query: ExploreQuery) -> Result<Covered<ExploreResult>> {
    v3(&st, |cx| insights::explore::explore(cx, &query))
}

#[tauri::command(async)]
pub fn get_data_coverage(st: St<'_>) -> Result<Covered<DataCoverage>> {
    v3(&st, insights::explore::data_coverage)
}

#[tauri::command(async)]
pub fn list_explore_views(st: St<'_>) -> Result<Vec<ExploreView>> {
    insights::explore::list_explore_views(&st.db.lock().unwrap())
}

#[tauri::command(async)]
pub fn save_explore_view(st: St<'_>, name: String, query: ExploreQuery) -> Result<ExploreView> {
    insights::explore::save_explore_view(&st.db.lock().unwrap(), &name, &query)
}

#[tauri::command(async)]
pub fn delete_explore_view(st: St<'_>, id: i64) -> Result<()> {
    insights::explore::delete_explore_view(&st.db.lock().unwrap(), id)
}

/* ------------------------------------------------------------- R3 Spotify (API.md §5) */
// Every R3 command but status/connect/disconnect starts with `require_connected` (disconnected →
// `spotify_auth`, the rest of the app unaffected), then validates its args again (TSD §7).

type Sp<'a> = State<'a, Spotify>;

#[tauri::command]
pub async fn get_spotify_status(sp: Sp<'_>) -> Result<SpotifyStatus> {
    sp.status().await
}

/// Asks for `user-follow-read` only while the Inbox's followed-artists option is on; creates the
/// Inbox after the first successful sign-in (TSD §5.3). `client_id`: pasted in the Connect
/// dialog; saved first so no env var is needed.
#[tauri::command]
pub async fn connect_spotify(
    sp: Sp<'_>,
    st: St<'_>,
    client_id: Option<String>,
) -> Result<SpotifyStatus> {
    if let Some(id) = client_id {
        sp.save_client_id(&id)?;
    }
    let follow = st.settings.lock().unwrap().inbox.followed_artists;
    let s = sp.connect(follow).await?;
    sp_inbox::ensure_on_connect(&st, &sp).await;
    Ok(s)
}

#[tauri::command]
pub async fn disconnect_spotify(sp: Sp<'_>) -> Result<()> {
    sp.disconnect().await
}

const ITEM_URIS: &[&str] = &["album", "track", "artist"];

fn check_uris(uris: &[String], kinds: &[&str]) -> Result<Vec<String>> {
    uris.iter().map(|u| spotify::check_uri(u, kinds)).collect()
}

/// The playlist and every kept URI must be well-formed before anything reaches Spotify.
/// Validates the draft's playlist URI and keeps the trimmed value (the undo snapshot is keyed by
/// it), then the kept track URIs.
fn check_draft(draft: &mut Draft, keep: &[String]) -> Result<Vec<String>> {
    draft.playlist_uri = spotify::check_uri(&draft.playlist_uri, &["playlist"])?;
    check_uris(keep, &["track"])
}

fn playlist_name(name: &str) -> Result<String> {
    let n = clean_line(name, limit::PLAYLIST);
    if n.is_empty() {
        return Err(AppError::validation("Give the playlist a name."));
    }
    Ok(n)
}

#[tauri::command]
pub async fn list_spotify_playlists(sp: Sp<'_>) -> Result<Vec<SpotifyPlaylist>> {
    sp.require_connected()?;
    sp_studio::list_spotify_playlists(&sp).await
}

#[tauri::command]
pub async fn load_studio_draft(sp: Sp<'_>, st: St<'_>, playlist_uri: String) -> Result<Draft> {
    sp.require_connected()?;
    let uri = spotify::check_uri(&playlist_uri, &["playlist"])?;
    sp_studio::load_studio_draft(&sp, &st.db, &uri).await
}

#[tauri::command]
pub async fn preview_studio_apply(
    sp: Sp<'_>,
    mut draft: Draft,
    keep: Vec<String>,
) -> Result<StudioDiff> {
    sp.require_connected()?;
    let keep = check_draft(&mut draft, &keep)?;
    sp_studio::preview_studio_apply(&sp, &draft, &keep).await
}

#[tauri::command]
pub async fn apply_studio_draft(
    sp: Sp<'_>,
    mut draft: Draft,
    keep: Vec<String>,
) -> Result<Applied> {
    sp.require_connected()?;
    let keep = check_draft(&mut draft, &keep)?;
    sp_studio::apply_studio_draft(&sp, &draft, &keep).await
}

#[tauri::command]
pub async fn get_last_apply(sp: Sp<'_>, playlist_uri: String) -> Result<Option<ApplyRecord>> {
    sp.require_connected()?;
    let uri = spotify::check_uri(&playlist_uri, &["playlist"])?;
    sp_studio::get_last_apply(&sp, &uri).await
}

#[tauri::command]
pub async fn undo_last_apply(sp: Sp<'_>, playlist_uri: String) -> Result<()> {
    sp.require_connected()?;
    let uri = spotify::check_uri(&playlist_uri, &["playlist"])?;
    sp_studio::undo_last_apply(&sp, &uri).await
}

#[tauri::command]
pub async fn create_playlist_from_filter(
    sp: Sp<'_>,
    st: St<'_>,
    filter_id: i64,
    name: String,
    public: bool,
) -> Result<CreatedPlaylist> {
    sp.require_connected()?;
    let name = playlist_name(&name)?;
    sp_create::create_playlist_from_filter(&sp, &st.db, filter_id, &name, public).await
}

#[tauri::command]
pub async fn create_playlist_from_items(
    sp: Sp<'_>,
    st: St<'_>,
    item_ids: Vec<i64>,
    name: String,
    public: bool,
) -> Result<CreatedPlaylist> {
    sp.require_connected()?;
    let name = playlist_name(&name)?;
    sp_create::create_playlist_from_items(&sp, &st.db, &item_ids, &name, public).await
}

#[tauri::command]
pub async fn autofill(sp: Sp<'_>, uri: String) -> Result<Autofill> {
    sp.require_connected()?;
    let uri = spotify::check_uri(&uri, ITEM_URIS)?;
    sp_autofill::autofill(&sp, &uri).await
}

#[tauri::command]
pub async fn bulk_add_album_tracks(
    sp: Sp<'_>,
    st: St<'_>,
    album_id: i64,
    tracks: Vec<NewTrack>,
) -> Result<Vec<ItemSummary>> {
    sp.require_connected()?;
    let tracks = tracks
        .into_iter()
        .map(|t| {
            let name = clean_line(&t.name, limit::NAME);
            if name.is_empty() {
                return Err(AppError::validation("Every track needs a name."));
            }
            Ok(NewTrack {
                uri: spotify::check_uri(&t.uri, &["track"])?,
                name,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    sp_autofill::bulk_add_album_tracks(&sp, &st.db, album_id, tracks).await
}

#[tauri::command]
pub async fn get_top_items(
    sp: Sp<'_>,
    st: St<'_>,
    kind: TopKind,
    range: TopRange,
) -> Result<Vec<SpotifyEntry>> {
    sp.require_connected()?;
    sp_discover::get_top_items(&sp, &st.db, kind, range).await
}

#[tauri::command]
pub async fn get_recent_plays_api(sp: Sp<'_>, st: St<'_>) -> Result<Vec<RecentPlay>> {
    sp.require_connected()?;
    sp_discover::get_recent_plays_api(&sp, &st.db).await
}

#[tauri::command]
pub async fn library_contains(sp: Sp<'_>, uris: Vec<String>) -> Result<Vec<bool>> {
    sp.require_connected()?;
    // validated in library_sync: tracks and albums only (artist → validation, TSD §5.2)
    sp_library::library_contains(&sp, &uris).await
}

#[tauri::command]
pub async fn save_to_spotify_library(sp: Sp<'_>, uris: Vec<String>) -> Result<()> {
    sp.require_connected()?;
    // validated in library_sync: tracks and albums only (artist → validation, TSD §5.2)
    sp_library::save_to_spotify_library(&sp, &uris).await
}

#[tauri::command]
pub async fn remove_from_spotify_library(sp: Sp<'_>, uris: Vec<String>) -> Result<()> {
    sp.require_connected()?;
    // validated in library_sync: tracks and albums only (artist → validation, TSD §5.2)
    sp_library::remove_from_spotify_library(&sp, &uris).await
}

#[tauri::command]
pub async fn find_cross_playlist_dupes(
    sp: Sp<'_>,
    playlist_uri: String,
) -> Result<Vec<sp_studio::CrossDupe>> {
    sp.require_connected()?;
    let uri = spotify::check_uri(&playlist_uri, &["playlist"])?;
    sp_studio::find_cross_playlist_dupes(&sp, &uri).await
}

/* ------------------------------------------------------------- Runout Inbox (API.md §5, TSD §5.3) */

/// Settings only: no connection needed, no request.
#[tauri::command]
pub async fn get_inbox(sp: Sp<'_>, st: St<'_>) -> Result<InboxStatus> {
    Ok(sp_inbox::status(&st, &sp))
}

#[tauri::command]
pub async fn create_inbox(sp: Sp<'_>, st: St<'_>) -> Result<InboxStatus> {
    sp.require_connected()?;
    sp_inbox::create(&st, &sp).await
}

/// Check now; emits `inbox-imported` when something came in (as the background check does).
#[tauri::command]
pub async fn check_inbox<R: Runtime>(app: AppHandle<R>, sp: Sp<'_>, st: St<'_>) -> Result<Checked> {
    let got = sp_inbox::check(&st, &sp).await?;
    if got.imported > 0 {
        let _ = app.emit("inbox-imported", &got);
    }
    Ok(got)
}

#[tauri::command]
pub async fn set_inbox_options(
    sp: Sp<'_>,
    st: St<'_>,
    saved_albums: bool,
    followed_artists: bool,
) -> Result<InboxStatus> {
    sp_inbox::set_options(&st, &sp, saved_albums, followed_artists).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets};
    use tauri::{Manager, WebviewWindowBuilder};

    /// The draft's playlist URI is validated and trimmed, so apply and get/undo key the same snapshot.
    #[test]
    fn check_draft_keeps_the_trimmed_playlist_uri() {
        let pl = "spotify:playlist:PPPPPPPPPPPPPPPPPPPPPP";
        let mut d = Draft {
            playlist_uri: format!("  {pl} "),
            snapshot_id: "s1".into(),
            tracks: vec![],
        };
        check_draft(&mut d, &[]).unwrap();
        assert_eq!(d.playlist_uri, pl);
        d.playlist_uri = "spotify:album:PPPPPPPPPPPPPPPPPPPPPP".into();
        assert_eq!(
            check_draft(&mut d, &[]).unwrap_err().code,
            crate::error::Code::InvalidLink
        );
    }

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
        let stub = crate::spotify::testing::Stub::new(&[]);
        app.manage(crate::spotify::testing::spotify(&d, &stub));
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
        let lg = call(
            &w,
            "set_logger",
            json!({"patch": {"include_private": true}}),
        )
        .unwrap();
        assert_eq!(lg["include_private"], json!(true));
        let lg = call(&w, "resume_logger", json!({})).unwrap();
        assert_eq!(
            (&lg["state"], &lg["paused_until"]),
            (&json!("idle"), &Value::Null)
        );
        let e = call(&w, "set_logger", json!({"patch": {"nope": true}})).unwrap_err();
        assert_eq!(e["code"], json!("validation"), "unknown patch key");
        let e = call(&w, "pause_logger", json!({"minutes": 0})).unwrap_err();
        assert_eq!(e["code"], json!("validation"));
        let lg = call(&w, "get_logger_status", json!({})).unwrap();
        assert_eq!(
            (&lg["autostart"], &lg["now_playing"]),
            (&json!(false), &Value::Null)
        );
        let e = call(&w, "check_backup", json!({"path": "C:\\nope.txt"})).unwrap_err();
        assert_eq!(e["code"], json!("validation"));

        // import (API.md §4) with the synthetic export
        let path = json!(crate::importer::source::fixture().to_string_lossy());
        let p = call(&w, "preview_import", json!({ "path": path })).unwrap();
        assert_eq!(
            (p["music"].clone(), p["playlists"].clone()),
            (json!(16), json!(2))
        );
        let r = call(&w, "run_import", json!({ "path": path })).unwrap();
        assert_eq!(r["added"], json!(15));
        assert_eq!(
            call(&w, "list_imports", json!({})).unwrap()[0]["kind"],
            json!("extended")
        );
        let lists = call(&w, "list_exported_playlists", json!({})).unwrap();
        let id = lists[0]["id"].clone();
        let t = call(&w, "get_playlist_checkup", json!({ "id": id })).unwrap();
        assert!(t["tracks"].is_array());
        let e = call(&w, "preview_import", json!({"path": "C:\\nope.txt"})).unwrap_err();
        assert_eq!(e["code"], json!("validation"));

        // Insights v3 (API.md §4.1) with the keys api.ts sends; every payload carries `coverage`
        let period = json!({"period": {"grain": "month", "vs": "usual", "at": null}});
        let explore = json!({"query": {"series": [{"kind": "artist", "name": "x"}], "from": "2026-01-01",
                                        "to": "2026-01-31", "grain": "week", "measure": "hours"}});
        for (cmd, args) in [
            ("get_period_report", period.clone()),
            ("get_trends", json!({"measure": "artists"})),
            ("get_discovery", period.clone()),
            ("get_listening_habits", period.clone()),
            ("get_genres_decades", period.clone()),
            ("get_records", json!({})),
            ("explore", explore.clone()),
            ("get_data_coverage", json!({})),
        ] {
            let v = call(&w, cmd, args).unwrap();
            assert!(
                v["coverage"]["gaps"].is_array() && v["data"].is_object(),
                "{cmd}"
            );
        }
        let mut save = explore.clone();
        save["name"] = json!("Weekly x");
        let view = call(&w, "save_explore_view", save).unwrap();
        assert_eq!(view["query"], explore["query"]);
        let views = call(&w, "list_explore_views", json!({})).unwrap();
        assert_eq!(views, json!([view.clone()]));
        call(&w, "delete_explore_view", json!({"id": view["id"]})).unwrap();
        assert_eq!(
            call(&w, "list_explore_views", json!({})).unwrap(),
            json!([])
        );
        let custom =
            json!({"period": {"grain": "custom", "vs": "prev", "at": null, "from": "2026-02-01"}});
        let e = call(&w, "get_period_report", custom).unwrap_err();
        assert_eq!(e["code"], json!("validation"));
        let p = call(&w, "list_plays", json!({"q": {"tab": "all", "range": "7d", "offset": 0, "limit": 5, "date": "2026-10-25"}})).unwrap();
        assert!(p["rows"].is_array());
        let s = call(
            &w,
            "update_settings",
            json!({"patch": {"export_reminder": true}}),
        )
        .unwrap();
        assert_eq!(s["export_reminder"], json!(true));
        let e = call(&w, "update_settings", json!({"patch": {"music_db": false}})).unwrap_err();
        assert_eq!(e["code"], json!("validation"));
        let s = call(&w, "set_music_db", json!({"on": false})).unwrap();
        assert_eq!(s["music_db"], json!(false));

        // R3 (API.md §5): disconnected → status says so, every other command is spotify_auth
        // the Inbox state is settings only: answered disconnected, no request
        let i = call(&w, "get_inbox", json!({})).unwrap();
        assert_eq!(
            (&i["state"], &i["needs_reconnect"]),
            (&json!("none"), &json!(false))
        );
        let s = call(&w, "get_spotify_status", json!({})).unwrap();
        assert_eq!(
            s,
            json!({"state": "disconnected", "display_name": null, "renew_by": null})
        );
        let pl = json!("spotify:playlist:SyntheticPlaylist00001");
        let draft = json!({"playlist_uri": pl, "snapshot_id": "s", "tracks": []});
        for (cmd, args) in [
            ("list_spotify_playlists", json!({})),
            ("load_studio_draft", json!({"playlistUri": pl})),
            ("preview_studio_apply", json!({"draft": draft, "keep": []})),
            ("apply_studio_draft", json!({"draft": draft, "keep": []})),
            ("get_last_apply", json!({"playlistUri": pl})),
            ("undo_last_apply", json!({"playlistUri": pl})),
            (
                "create_playlist_from_filter",
                json!({"filterId": 1, "name": "x", "public": false}),
            ),
            (
                "create_playlist_from_items",
                json!({"itemIds": [1], "name": "x", "public": false}),
            ),
            (
                "autofill",
                json!({"uri": "spotify:album:SyntheticAlbumId000001"}),
            ),
            ("bulk_add_album_tracks", json!({"albumId": 1, "tracks": []})),
            ("get_top_items", json!({"kind": "tracks", "range": "4w"})),
            ("get_recent_plays_api", json!({})),
            ("library_contains", json!({"uris": []})),
            ("save_to_spotify_library", json!({"uris": []})),
            ("remove_from_spotify_library", json!({"uris": []})),
            ("find_cross_playlist_dupes", json!({"playlistUri": pl})),
            ("create_inbox", json!({})),
            ("check_inbox", json!({})),
            (
                "set_inbox_options",
                json!({"savedAlbums": true, "followedArtists": false}),
            ),
        ] {
            let e = call(&w, cmd, args).unwrap_err();
            assert_eq!(e["code"], json!("spotify_auth"), "{cmd}");
        }
        call(&w, "disconnect_spotify", json!({})).unwrap();
        drop(w);
        drop(app);
        let _ = std::fs::remove_dir_all(d);
    }
}
