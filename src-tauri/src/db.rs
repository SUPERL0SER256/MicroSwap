use crate::models::{check_file_missing, AppSettings, Collection, HistoryEntry, Wallpaper};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use uuid::Uuid;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(db_path: &Path) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<(), String> {
        self.conn
            .execute_batch(
                "
            CREATE TABLE IF NOT EXISTS wallpapers (
                id TEXT PRIMARY KEY,
                file_path TEXT NOT NULL,
                file_name TEXT NOT NULL,
                thumbnail_path TEXT NOT NULL,
                width INTEGER NOT NULL,
                height INTEGER NOT NULL,
                file_size INTEGER NOT NULL,
                import_date TEXT NOT NULL,
                last_used_date TEXT,
                content_hash TEXT NOT NULL UNIQUE,
                is_favorite INTEGER NOT NULL DEFAULT 0,
                is_active INTEGER NOT NULL DEFAULT 0,
                fit_mode TEXT NOT NULL DEFAULT 'fill'
            );

            CREATE TABLE IF NOT EXISTS collections (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL,
                last_selected_wallpaper_id TEXT
            );

            CREATE TABLE IF NOT EXISTS collection_wallpapers (
                collection_id TEXT NOT NULL,
                wallpaper_id TEXT NOT NULL,
                PRIMARY KEY (collection_id, wallpaper_id),
                FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE,
                FOREIGN KEY (wallpaper_id) REFERENCES wallpapers(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS wallpaper_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                wallpaper_id TEXT NOT NULL,
                applied_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            ",
            )
            .map_err(|e| e.to_string())?;

        // Ensure fit_mode and sort_order columns exist if migrating an older db
        let _ = self.conn.execute(
            "ALTER TABLE wallpapers ADD COLUMN fit_mode TEXT NOT NULL DEFAULT 'fill'",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE wallpapers ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0",
            [],
        );

        Ok(())
    }

    pub fn get_wallpaper_count(&self) -> Result<usize, String> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM wallpapers", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        Ok(count as usize)
    }

    pub fn find_by_hash_or_path(
        &self,
        content_hash: &str,
        file_path: &str,
    ) -> Result<Option<String>, String> {
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM wallpapers WHERE content_hash = ?1 OR file_path = ?2 LIMIT 1",
                params![content_hash, file_path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(existing)
    }

    pub fn insert_wallpaper(
        &self,
        file_path: &str,
        file_name: &str,
        thumbnail_path: &str,
        width: u32,
        height: u32,
        file_size: u64,
        content_hash: &str,
        is_active: bool,
    ) -> Result<Wallpaper, String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let last_used = if is_active { Some(now.clone()) } else { None };
        let default_fit = self
            .get_settings()
            .map(|s| s.wallpaper_scaling)
            .unwrap_or_else(|_| "fill".to_string());

        if is_active {
            let _ = self.conn.execute("UPDATE wallpapers SET is_active = 0", []);
        }

        let next_order: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM wallpapers",
                [],
                |row| row.get(0),
            )
            .unwrap_or(1);

        self.conn
            .execute(
                "INSERT INTO wallpapers (
                    id, file_path, file_name, thumbnail_path, width, height,
                    file_size, import_date, last_used_date, content_hash, is_favorite, is_active, fit_mode, sort_order
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, ?11, ?12, ?13)",
                params![
                    id,
                    file_path,
                    file_name,
                    thumbnail_path,
                    width,
                    height,
                    file_size as i64,
                    now,
                    last_used,
                    content_hash,
                    if is_active { 1 } else { 0 },
                    default_fit,
                    next_order
                ],
            )
            .map_err(|e| e.to_string())?;

        if is_active {
            let _ = self.record_history(&id);
        }

        self.get_wallpaper_by_id(&id)?
            .ok_or_else(|| "Failed to retrieve inserted wallpaper".to_string())
    }

    pub fn get_collections_for_wallpaper(&self, wallpaper_id: &str) -> Result<Vec<String>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT collection_id FROM collection_wallpapers WHERE wallpaper_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![wallpaper_id], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        let mut cols = Vec::new();
        for r in rows {
            cols.push(r.map_err(|e| e.to_string())?);
        }
        Ok(cols)
    }

    pub fn get_wallpaper_by_id(&self, id: &str) -> Result<Option<Wallpaper>, String> {
        let wp_opt = self
            .conn
            .query_row(
                "SELECT id, file_path, file_name, thumbnail_path, width, height, file_size,
                        import_date, last_used_date, content_hash, is_favorite, is_active, fit_mode
                 FROM wallpapers WHERE id = ?1",
                params![id],
                |row| {
                    let file_path: String = row.get(1)?;
                    Ok((
                        row.get::<_, String>(0)?,
                        file_path.clone(),
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, u32>(4)?,
                        row.get::<_, u32>(5)?,
                        row.get::<_, i64>(6)? as u64,
                        row.get::<_, String>(7)?,
                        row.get::<_, Option<String>>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, i32>(10)? != 0,
                        row.get::<_, i32>(11)? != 0,
                        row.get::<_, String>(12).unwrap_or_else(|_| "fill".to_string()),
                        check_file_missing(&file_path),
                    ))
                },
            )
            .optional()
            .map_err(|e| e.to_string())?;

        if let Some((
            wid,
            file_path,
            file_name,
            thumbnail_path,
            width,
            height,
            file_size,
            import_date,
            last_used_date,
            content_hash,
            is_favorite,
            is_active,
            fit_mode,
            is_missing,
        )) = wp_opt
        {
            let collections = self.get_collections_for_wallpaper(&wid)?;
            Ok(Some(Wallpaper {
                id: wid,
                file_path,
                file_name,
                thumbnail_path,
                width,
                height,
                file_size,
                import_date,
                last_used_date,
                content_hash,
                is_favorite,
                is_active,
                is_missing,
                fit_mode,
                collections,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_wallpapers(&self) -> Result<Vec<Wallpaper>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, file_path, file_name, thumbnail_path, width, height, file_size,
                        import_date, last_used_date, content_hash, is_favorite, is_active, fit_mode
                 FROM wallpapers
                 ORDER BY sort_order ASC, import_date ASC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                let file_path: String = row.get(1)?;
                Ok((
                    row.get::<_, String>(0)?,
                    file_path.clone(),
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, u32>(5)?,
                    row.get::<_, i64>(6)? as u64,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, i32>(10)? != 0,
                    row.get::<_, i32>(11)? != 0,
                    row.get::<_, String>(12).unwrap_or_else(|_| "fill".to_string()),
                    check_file_missing(&file_path),
                ))
            })
            .map_err(|e| e.to_string())?;

        let mut list = Vec::new();
        for r in rows {
            let (
                wid,
                file_path,
                file_name,
                thumbnail_path,
                width,
                height,
                file_size,
                import_date,
                last_used_date,
                content_hash,
                is_favorite,
                is_active,
                fit_mode,
                is_missing,
            ) = r.map_err(|e| e.to_string())?;
            let collections = self.get_collections_for_wallpaper(&wid)?;
            list.push(Wallpaper {
                id: wid,
                file_path,
                file_name,
                thumbnail_path,
                width,
                height,
                file_size,
                import_date,
                last_used_date,
                content_hash,
                is_favorite,
                is_active,
                is_missing,
                fit_mode,
                collections,
            });
        }
        Ok(list)
    }

    pub fn set_wallpaper_fit_mode(&self, wallpaper_id: &str, fit_mode: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE wallpapers SET fit_mode = ?1 WHERE id = ?2",
                params![fit_mode, wallpaper_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn reorder_wallpapers(&self, ordered_ids: &[String]) -> Result<(), String> {
        for (idx, wid) in ordered_ids.iter().enumerate() {
            self.conn
                .execute(
                    "UPDATE wallpapers SET sort_order = ?1 WHERE id = ?2",
                    params![(idx + 1) as i64, wid],
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn set_active_wallpaper(&self, wallpaper_id: &str, fit_mode: &str) -> Result<(), String> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute("UPDATE wallpapers SET is_active = 0", [])
            .map_err(|e| e.to_string())?;
        self.conn
            .execute(
                "UPDATE wallpapers SET is_active = 1, last_used_date = ?1, fit_mode = ?2 WHERE id = ?3",
                params![now, fit_mode, wallpaper_id],
            )
            .map_err(|e| e.to_string())?;
        self.record_history(wallpaper_id)?;
        Ok(())
    }

    pub fn get_previous_wallpaper_id(&self) -> Result<Option<String>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT wallpaper_id FROM wallpaper_history ORDER BY id DESC LIMIT 2")
            .map_err(|e| e.to_string())?;
        let ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .flatten()
            .collect();
        if ids.len() >= 2 {
            Ok(Some(ids[1].clone()))
        } else {
            Ok(None)
        }
    }

    pub fn record_history(&self, wallpaper_id: &str) -> Result<(), String> {
        let last_id: Option<String> = self
            .conn
            .query_row(
                "SELECT wallpaper_id FROM wallpaper_history ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        if last_id.as_deref() == Some(wallpaper_id) {
            return Ok(());
        }

        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "INSERT INTO wallpaper_history (wallpaper_id, applied_at) VALUES (?1, ?2)",
                params![wallpaper_id, now],
            )
            .map_err(|e| e.to_string())?;

        let settings = self.get_settings()?;
        let limit = settings.history_limit.max(10) as i64;
        let _ = self.conn.execute(
            "DELETE FROM wallpaper_history WHERE id NOT IN (
                SELECT id FROM wallpaper_history ORDER BY id DESC LIMIT ?1
            )",
            params![limit],
        );
        Ok(())
    }

    pub fn get_history(&self) -> Result<Vec<HistoryEntry>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, wallpaper_id, applied_at
                 FROM wallpaper_history
                 ORDER BY id DESC LIMIT 50",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        let mut entries = Vec::new();
        for r in rows {
            let (id, wid, applied_at) = r.map_err(|e| e.to_string())?;
            let wp = self.get_wallpaper_by_id(&wid)?;
            if wp.is_some() {
                entries.push(HistoryEntry {
                    id,
                    wallpaper_id: wid,
                    applied_at,
                    wallpaper: wp,
                });
            }
        }
        Ok(entries)
    }

    pub fn toggle_favorite(&self, wallpaper_id: &str) -> Result<bool, String> {
        let current: i32 = self
            .conn
            .query_row(
                "SELECT is_favorite FROM wallpapers WHERE id = ?1",
                params![wallpaper_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let next = if current == 0 { 1 } else { 0 };
        self.conn
            .execute(
                "UPDATE wallpapers SET is_favorite = ?1 WHERE id = ?2",
                params![next, wallpaper_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(next != 0)
    }

    pub fn remove_wallpaper(&self, wallpaper_id: &str) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM collection_wallpapers WHERE wallpaper_id = ?1",
                params![wallpaper_id],
            )
            .map_err(|e| e.to_string())?;
        self.conn
            .execute("DELETE FROM wallpapers WHERE id = ?1", params![wallpaper_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn relocate_wallpaper(
        &self,
        wallpaper_id: &str,
        new_path: &str,
    ) -> Result<Wallpaper, String> {
        let file_name = Path::new(new_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Wallpaper")
            .to_string();
        self.conn
            .execute(
                "UPDATE wallpapers SET file_path = ?1, file_name = ?2 WHERE id = ?3",
                params![new_path, file_name, wallpaper_id],
            )
            .map_err(|e| e.to_string())?;
        self.get_wallpaper_by_id(wallpaper_id)?
            .ok_or_else(|| "Wallpaper not found".into())
    }

    pub fn list_collections(&self) -> Result<Vec<Collection>, String> {
        let total_count = self.get_wallpaper_count()?;

        let mut result = vec![Collection {
            id: "all".to_string(),
            name: "All Wallpapers".to_string(),
            created_at: "".to_string(),
            wallpaper_count: total_count,
            last_selected_wallpaper_id: None,
            is_system: true,
        }];

        let mut stmt = self
            .conn
            .prepare(
                "SELECT c.id, c.name, c.created_at, c.last_selected_wallpaper_id,
                        (SELECT COUNT(*) FROM collection_wallpapers cw WHERE cw.collection_id = c.id)
                 FROM collections c
                 ORDER BY c.name ASC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                Ok(Collection {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    created_at: row.get(2)?,
                    last_selected_wallpaper_id: row.get(3)?,
                    wallpaper_count: row.get::<_, i64>(4)? as usize,
                    is_system: false,
                })
            })
            .map_err(|e| e.to_string())?;

        for r in rows {
            result.push(r.map_err(|e| e.to_string())?);
        }

        Ok(result)
    }

    pub fn create_collection(&self, name: &str) -> Result<Collection, String> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err("Collection name cannot be empty".into());
        }
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "INSERT INTO collections (id, name, created_at) VALUES (?1, ?2, ?3)",
                params![id, trimmed, now],
            )
            .map_err(|e| format!("Failed to create collection: {}", e))?;

        Ok(Collection {
            id,
            name: trimmed.to_string(),
            created_at: now,
            wallpaper_count: 0,
            last_selected_wallpaper_id: None,
            is_system: false,
        })
    }

    pub fn rename_collection(&self, id: &str, new_name: &str) -> Result<(), String> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return Err("Collection name cannot be empty".into());
        }
        self.conn
            .execute(
                "UPDATE collections SET name = ?1 WHERE id = ?2",
                params![trimmed, id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_collection(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM collection_wallpapers WHERE collection_id = ?1",
                params![id],
            )
            .map_err(|e| e.to_string())?;
        self.conn
            .execute("DELETE FROM collections WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn add_wallpaper_to_collection(
        &self,
        collection_id: &str,
        wallpaper_id: &str,
    ) -> Result<(), String> {
        if collection_id == "favorites" {
            self.conn
                .execute(
                    "UPDATE wallpapers SET is_favorite = 1 WHERE id = ?1",
                    params![wallpaper_id],
                )
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        self.conn
            .execute(
                "INSERT OR IGNORE INTO collection_wallpapers (collection_id, wallpaper_id) VALUES (?1, ?2)",
                params![collection_id, wallpaper_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn remove_wallpaper_from_collection(
        &self,
        collection_id: &str,
        wallpaper_id: &str,
    ) -> Result<(), String> {
        if collection_id == "favorites" {
            self.conn
                .execute(
                    "UPDATE wallpapers SET is_favorite = 0 WHERE id = ?1",
                    params![wallpaper_id],
                )
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        self.conn
            .execute(
                "DELETE FROM collection_wallpapers WHERE collection_id = ?1 AND wallpaper_id = ?2",
                params![collection_id, wallpaper_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_settings(&self) -> Result<AppSettings, String> {
        let json_opt: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'app_settings'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        if let Some(json_str) = json_opt {
            serde_json::from_str(&json_str).map_err(|e| e.to_string())
        } else {
            Ok(AppSettings::default())
        }
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), String> {
        let json_str = serde_json::to_string(settings).map_err(|e| e.to_string())?;
        self.conn
            .execute(
                "INSERT INTO settings (key, value) VALUES ('app_settings', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![json_str],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
