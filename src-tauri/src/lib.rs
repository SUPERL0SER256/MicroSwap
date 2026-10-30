pub mod db;
pub mod models;
pub mod thumbnails;
pub mod wallpaper_os;
pub mod wave_physics;

use db::Database;
use models::{AppSettings, Collection, HistoryEntry, ImportSummary, Wallpaper};
use rand::seq::SliceRandom;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use thumbnails::{is_supported_image, process_and_thumbnail};
use walkdir::WalkDir;
use wallpaper_os::{apply_windows_wallpaper, ensure_windows_startup, get_current_windows_wallpaper};

pub struct AppState {
    pub db: Mutex<Database>,
    pub thumbs_dir: PathBuf,
    pub app_data_dir: PathBuf,
    pub last_switcher_toggle: Mutex<Instant>,
    pub dialog_open: Mutex<bool>,
}

fn import_single_path(
    db: &Database,
    thumbs_dir: &Path,
    path: &Path,
    mark_active: bool,
) -> Result<Option<Wallpaper>, String> {
    if !path.exists() || !path.is_file() {
        return Err("File does not exist".into());
    }

    let info = process_and_thumbnail(path, thumbs_dir)?;
    let path_str = path.to_string_lossy().to_string();

    if let Some(_existing_id) = db.find_by_hash_or_path(&info.content_hash, &path_str)? {
        return Ok(None);
    }

    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Wallpaper")
        .to_string();

    let wp = db.insert_wallpaper(
        &path_str,
        &file_name,
        &info.thumbnail_path,
        info.width,
        info.height,
        info.file_size,
        &info.content_hash,
        mark_active,
    )?;

    Ok(Some(wp))
}

#[tauri::command]
fn get_initial_state(
    state: State<'_, AppState>,
) -> Result<(Vec<Wallpaper>, Vec<Collection>, AppSettings), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;

    if db.get_wallpaper_count()? == 0 {
        if let Some(current_wp_path) = get_current_windows_wallpaper(&state.app_data_dir) {
            let _ = import_single_path(&db, &state.thumbs_dir, &current_wp_path, true);
        }
    }

    let wallpapers = db.list_wallpapers()?;
    let collections = db.list_collections()?;
    let settings = db.get_settings()?;
    Ok((wallpapers, collections, settings))
}

#[tauri::command]
fn set_dialog_open(state: State<'_, AppState>, open: bool) -> Result<(), String> {
    if let Ok(mut guard) = state.dialog_open.lock() {
        *guard = open;
    }
    if !open {
        if let Ok(mut last) = state.last_switcher_toggle.lock() {
            *last = Instant::now();
        }
    }
    Ok(())
}

pub const MAX_WALLPAPERS: usize = 30;

#[tauri::command]
async fn import_wallpapers(
    app: AppHandle,
    paths: Vec<String>,
) -> Result<ImportSummary, String> {
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app_clone.try_state::<AppState>() else {
            return Err("App state unavailable".to_string());
        };

        let mut candidate_files: Vec<PathBuf> = Vec::new();
        for p_str in paths {
            let p = PathBuf::from(&p_str);
            if p.is_dir() {
                for entry in WalkDir::new(&p).max_depth(3).into_iter().flatten() {
                    let ep = entry.path();
                    if ep.is_file() && is_supported_image(ep) {
                        candidate_files.push(ep.to_path_buf());
                    }
                }
            } else if p.is_file() && is_supported_image(&p) {
                candidate_files.push(p);
            }
        }

        let current_count = {
            let db = state.db.lock().map_err(|e| e.to_string())?;
            db.get_wallpaper_count()?
        };

        let mut remaining_slots = MAX_WALLPAPERS.saturating_sub(current_count);
        let mut imported = Vec::new();
        let mut duplicate_count = 0;
        let mut failed_count = 0;
        let mut skipped_limit_count = 0;

        for file_path in candidate_files {
            if remaining_slots == 0 {
                skipped_limit_count += 1;
                continue;
            }

            // Process image & thumbnail outside the DB lock so the app stays responsive
            let info = match process_and_thumbnail(&file_path, &state.thumbs_dir) {
                Ok(i) => i,
                Err(_) => {
                    failed_count += 1;
                    continue;
                }
            };

            let path_str = file_path.to_string_lossy().to_string();
            let file_name = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Wallpaper")
                .to_string();

            let db = state.db.lock().map_err(|e| e.to_string())?;
            if db.find_by_hash_or_path(&info.content_hash, &path_str)?.is_some() {
                duplicate_count += 1;
                continue;
            }

            match db.insert_wallpaper(
                &path_str,
                &file_name,
                &info.thumbnail_path,
                info.width,
                info.height,
                info.file_size,
                &info.content_hash,
                false,
            ) {
                Ok(wp) => {
                    imported.push(wp);
                    remaining_slots = remaining_slots.saturating_sub(1);
                }
                Err(_) => failed_count += 1,
            }
        }

        let _ = app_clone.emit("library-updated", ());

        Ok(ImportSummary {
            imported_count: imported.len(),
            duplicate_count,
            failed_count,
            skipped_limit_count,
            wallpapers: imported,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn apply_wallpaper(
    app: AppHandle,
    wallpaper_id: String,
    fit_mode: Option<String>,
) -> Result<Wallpaper, String> {
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app_clone.try_state::<AppState>() else {
            return Err("App state unavailable".to_string());
        };
        let (file_path, effective_fit) = {
            let db = state.db.lock().map_err(|e| e.to_string())?;
            let wp = db
                .get_wallpaper_by_id(&wallpaper_id)?
                .ok_or_else(|| "Wallpaper not found in library".to_string())?;

            if wp.is_missing {
                return Err("Source wallpaper file is missing from disk.".into());
            }
            let fit = fit_mode.unwrap_or_else(|| wp.fit_mode.clone());
            (wp.file_path, fit)
        };

        // Run heavy Win32 SPI_SETDESKWALLPAPER outside DB lock on background worker thread!
        apply_windows_wallpaper(&file_path, &effective_fit)?;

        let updated = {
            let db = state.db.lock().map_err(|e| e.to_string())?;
            db.set_active_wallpaper(&wallpaper_id, &effective_fit)?;
            db.get_wallpaper_by_id(&wallpaper_id)?
                .ok_or_else(|| "Failed to load updated wallpaper".to_string())?
        };

        let _ = app_clone.emit("library-updated", ());
        Ok(updated)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn set_wallpaper_fit(
    app: AppHandle,
    state: State<'_, AppState>,
    wallpaper_id: String,
    fit_mode: String,
    apply_immediately: bool,
) -> Result<Wallpaper, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.set_wallpaper_fit_mode(&wallpaper_id, &fit_mode)?;

    if apply_immediately {
        if let Some(wp) = db.get_wallpaper_by_id(&wallpaper_id)? {
            if !wp.is_missing {
                apply_windows_wallpaper(&wp.file_path, &fit_mode)?;
                db.set_active_wallpaper(&wallpaper_id, &fit_mode)?;
            }
        }
    }

    let updated = db
        .get_wallpaper_by_id(&wallpaper_id)?
        .ok_or_else(|| "Wallpaper not found".to_string())?;
    let _ = app.emit("library-updated", ());
    Ok(updated)
}

#[tauri::command]
fn reorder_wallpapers(
    app: AppHandle,
    state: State<'_, AppState>,
    ordered_ids: Vec<String>,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.reorder_wallpapers(&ordered_ids)?;
    let _ = app.emit("library-updated", ());
    Ok(())
}

#[tauri::command]
fn undo_last_wallpaper(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<Wallpaper>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    if let Some(prev_id) = db.get_previous_wallpaper_id()? {
        if let Some(wp) = db.get_wallpaper_by_id(&prev_id)? {
            if !wp.is_missing {
                apply_windows_wallpaper(&wp.file_path, &wp.fit_mode)?;
                db.set_active_wallpaper(&wp.id, &wp.fit_mode)?;
                let _ = app.emit("library-updated", ());
                return Ok(db.get_wallpaper_by_id(&prev_id)?);
            }
        }
    }
    Ok(None)
}

#[tauri::command]
fn toggle_favorite(
    app: AppHandle,
    state: State<'_, AppState>,
    wallpaper_id: String,
) -> Result<bool, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let res = db.toggle_favorite(&wallpaper_id)?;
    let _ = app.emit("library-updated", ());
    Ok(res)
}

#[tauri::command]
fn remove_wallpaper(
    app: AppHandle,
    state: State<'_, AppState>,
    wallpaper_id: String,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    if let Ok(Some(wp)) = db.get_wallpaper_by_id(&wallpaper_id) {
        let thumb = PathBuf::from(&wp.thumbnail_path);
        if thumb.starts_with(&state.thumbs_dir) && thumb.exists() {
            let _ = std::fs::remove_file(thumb);
        }
    }
    db.remove_wallpaper(&wallpaper_id)?;
    let _ = app.emit("library-updated", ());
    Ok(())
}

#[tauri::command]
fn relocate_wallpaper(
    app: AppHandle,
    state: State<'_, AppState>,
    wallpaper_id: String,
    new_path: String,
) -> Result<Wallpaper, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let wp = db.relocate_wallpaper(&wallpaper_id, &new_path)?;
    let _ = app.emit("library-updated", ());
    Ok(wp)
}

#[tauri::command]
fn create_collection(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<Collection, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let col = db.create_collection(&name)?;
    let _ = app.emit("library-updated", ());
    Ok(col)
}

#[tauri::command]
fn rename_collection(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    new_name: String,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.rename_collection(&id, &new_name)?;
    let _ = app.emit("library-updated", ());
    Ok(())
}

#[tauri::command]
fn delete_collection(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_collection(&id)?;
    let _ = app.emit("library-updated", ());
    Ok(())
}

#[tauri::command]
fn add_to_collection(
    app: AppHandle,
    state: State<'_, AppState>,
    collection_id: String,
    wallpaper_id: String,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.add_wallpaper_to_collection(&collection_id, &wallpaper_id)?;
    let _ = app.emit("library-updated", ());
    Ok(())
}

#[tauri::command]
fn remove_from_collection(
    app: AppHandle,
    state: State<'_, AppState>,
    collection_id: String,
    wallpaper_id: String,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.remove_wallpaper_from_collection(&collection_id, &wallpaper_id)?;
    let _ = app.emit("library-updated", ());
    Ok(())
}

#[tauri::command]
fn get_history(state: State<'_, AppState>) -> Result<Vec<HistoryEntry>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_history()
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<(), String> {
    let old_shortcut = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let old = db.get_settings()?.global_shortcut;
        db.save_settings(&settings)?;
        old
    };

    if old_shortcut != settings.global_shortcut {
        let _ = app.global_shortcut().unregister_all();
        let _ = register_switcher_shortcut(&app, &settings.global_shortcut);
    }

    let _ = app.emit("library-updated", ());
    Ok(())
}

#[tauri::command]
fn open_in_explorer(state: State<'_, AppState>, file_path: String) -> Result<(), String> {
    // Security: Only allow opening paths that exist in the user's registered wallpapers table
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let registered = db.find_by_hash_or_path("", &file_path)?;
    if registered.is_none() {
        return Err("Unauthorized file path".into());
    }

    let canonical = std::fs::canonicalize(&file_path).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(canonical.as_os_str());
        std::process::Command::new("explorer.exe")
            .arg(arg)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn toggle_switcher_window(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    toggle_switcher_internal(&app, &state);
    Ok(())
}

#[tauri::command]
fn hide_switcher_window(app: AppHandle) -> Result<(), String> {
    if let Some(sw) = app.get_webview_window("switcher") {
        let _ = sw.hide();
        let _ = app.emit("switcher-hidden", ());
    }
    Ok(())
}

#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn clear_thumbnail_cache(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let wallpapers = db.list_wallpapers()?;
    let mut rebuilt = 0;
    for wp in wallpapers {
        let p = PathBuf::from(&wp.file_path);
        if p.exists() {
            if process_and_thumbnail(&p, &state.thumbs_dir).is_ok() {
                rebuilt += 1;
            }
        }
    }
    let _ = app.emit("library-updated", ());
    Ok(rebuilt)
}

pub fn toggle_switcher_internal(app: &AppHandle, state: &AppState) {
    if let Ok(mut last) = state.last_switcher_toggle.lock() {
        *last = Instant::now();
    }

    if let Some(win) = app.get_webview_window("switcher") {
        let is_vis = win.is_visible().unwrap_or(false);
        if is_vis {
            let _ = win.hide();
            let _ = app.emit("switcher-hidden", ());
        } else {
            let _ = win.center();
            let _ = win.show();
            let _ = win.set_focus();
            let _ = app.emit("switcher-opened", ());
        }
    }
}

fn step_or_shuffle_wallpaper(app: &AppHandle, mode: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Ok(db) = state.db.lock() else {
        return;
    };
    let Ok(wallpapers) = db.list_wallpapers() else {
        return;
    };
    let valid: Vec<&Wallpaper> = wallpapers.iter().filter(|w| !w.is_missing).collect();
    if valid.is_empty() {
        return;
    }

    let current_idx = valid.iter().position(|w| w.is_active).unwrap_or(0);
    let target = match mode {
        "next" => valid[(current_idx + 1) % valid.len()],
        "prev" => {
            if current_idx == 0 {
                valid[valid.len() - 1]
            } else {
                valid[current_idx - 1]
            }
        }
        "shuffle" => {
            if valid.len() == 1 {
                valid[0]
            } else {
                let candidates: Vec<&&Wallpaper> =
                    valid.iter().filter(|w| !w.is_active).collect();
                let mut rng = rand::thread_rng();
                **candidates.choose(&mut rng).unwrap_or(&&valid[0])
            }
        }
        _ => valid[0],
    };

    if apply_windows_wallpaper(&target.file_path, &target.fit_mode).is_ok() {
        let _ = db.set_active_wallpaper(&target.id, &target.fit_mode);
        let _ = app.emit("library-updated", ());
    }
}

fn register_switcher_shortcut(app: &AppHandle, shortcut_str: &str) -> Result<(), String> {
    let handle = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut_str, move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                if let Some(st) = handle.try_state::<AppState>() {
                    toggle_switcher_internal(&handle, &st);
                }
            }
        })
        .map_err(|e| e.to_string())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            std::fs::create_dir_all(&app_data_dir).ok();
            let thumbs_dir = app_data_dir.join("thumbnails");
            std::fs::create_dir_all(&thumbs_dir).ok();
            let db_path = app_data_dir.join("microswap.db");

            let db = Database::new(&db_path).expect("Failed to initialize MicroSwap SQLite database");

            // Automatically capture current Windows desktop wallpaper on first launch
            if db.get_wallpaper_count().unwrap_or(0) == 0 {
                if let Some(current_wp_path) = get_current_windows_wallpaper(&app_data_dir) {
                    let _ = import_single_path(&db, &thumbs_dir, &current_wp_path, true);
                }
            }

            let shortcut_str = db
                .get_settings()
                .map(|s| s.global_shortcut)
                .unwrap_or_else(|_| "Alt+W".to_string());

            app.manage(AppState {
                db: Mutex::new(db),
                thumbs_dir,
                app_data_dir,
                last_switcher_toggle: Mutex::new(Instant::now()),
                dialog_open: Mutex::new(false),
            });

            // Ensure release build registers itself in Windows Startup and opens cleanly
            ensure_windows_startup();

            // Register global shortcut (default Alt+W)
            let _ = register_switcher_shortcut(app.handle(), &shortcut_str);

            // Configure System Tray (Carousel-Only Architecture)
            let switcher_i = MenuItem::with_id(
                app,
                "switcher",
                "Toggle MicroSwap Carousel (Alt+W)",
                true,
                None::<&str>,
            )?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let next_i = MenuItem::with_id(app, "next", "Next Wallpaper", true, None::<&str>)?;
            let prev_i = MenuItem::with_id(app, "prev", "Previous Wallpaper", true, None::<&str>)?;
            let shuffle_i =
                MenuItem::with_id(app, "shuffle", "Shuffle Wallpaper", true, None::<&str>)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "Exit MicroSwap", true, None::<&str>)?;

            let menu = Menu::with_items(
                app,
                &[
                    &switcher_i,
                    &sep1,
                    &next_i,
                    &prev_i,
                    &shuffle_i,
                    &sep2,
                    &quit_i,
                ],
            )?;

            let mut tray_builder = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("MicroSwap — Press Alt+W to switch wallpapers")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "switcher" => {
                        if let Some(st) = app.try_state::<AppState>() {
                            toggle_switcher_internal(app, &st);
                        }
                    }
                    "next" => step_or_shuffle_wallpaper(app, "next"),
                    "prev" => step_or_shuffle_wallpaper(app, "prev"),
                    "shuffle" => step_or_shuffle_wallpaper(app, "shuffle"),
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(st) = app.try_state::<AppState>() {
                            toggle_switcher_internal(app, &st);
                        }
                    }
                });

            if let Some(default_icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(default_icon.clone());
            }
            let _tray = tray_builder.build(app)?;

            // Switcher window hides on focus loss unless a file/folder picker dialog is currently open
            if let Some(switcher_win) = app.get_webview_window("switcher") {
                let _ = switcher_win.center();
                let _ = switcher_win.show();
                let _ = switcher_win.set_focus();

                let app_handle = app.handle().clone();
                switcher_win.on_window_event(move |event| {
                    if let WindowEvent::Focused(false) = event {
                        let should_hide = app_handle
                            .try_state::<AppState>()
                            .and_then(|st| {
                                let dialog_active = *st.dialog_open.lock().ok()?;
                                if dialog_active {
                                    return Some(false);
                                }
                                let last = st.last_switcher_toggle.lock().ok()?;
                                Some(last.elapsed() > Duration::from_millis(600))
                            })
                            .unwrap_or(true);

                        if should_hide {
                            if let Some(sw) = app_handle.get_webview_window("switcher") {
                                let _ = sw.hide();
                                let _ = app_handle.emit("switcher-hidden", ());
                            }
                        }
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_initial_state,
            set_dialog_open,
            import_wallpapers,
            apply_wallpaper,
            set_wallpaper_fit,
            reorder_wallpapers,
            undo_last_wallpaper,
            toggle_favorite,
            remove_wallpaper,
            relocate_wallpaper,
            create_collection,
            rename_collection,
            delete_collection,
            add_to_collection,
            remove_from_collection,
            get_history,
            save_settings,
            open_in_explorer,
            toggle_switcher_window,
            hide_switcher_window,
            exit_app,
            clear_thumbnail_cache
        ])
        .run(tauri::generate_context!())
        .expect("error while running MicroSwap application");
}
