use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wallpaper {
    pub id: String,
    pub file_path: String,
    pub file_name: String,
    pub thumbnail_path: String,
    pub width: u32,
    pub height: u32,
    pub file_size: u64,
    pub import_date: String,
    pub last_used_date: Option<String>,
    pub content_hash: String,
    pub is_favorite: bool,
    pub is_active: bool,
    pub is_missing: bool,
    pub fit_mode: String, // "fill" | "fit" | "stretch" | "tile" | "center" | "span"
    pub collections: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub wallpaper_count: usize,
    pub last_selected_wallpaper_id: Option<String>,
    pub is_system: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub global_shortcut: String,
    pub minimize_to_tray: bool,
    pub wallpaper_scaling: String, // "fill" | "fit" | "stretch" | "tile" | "center" | "span"
    pub default_collection_id: String,
    pub history_limit: usize,
    pub instant_apply_shuffle: bool,
    pub expand_center_on_hover: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            global_shortcut: "Alt+W".to_string(),
            minimize_to_tray: true,
            wallpaper_scaling: "fill".to_string(),
            default_collection_id: "all".to_string(),
            history_limit: 50,
            instant_apply_shuffle: false,
            expand_center_on_hover: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: i64,
    pub wallpaper_id: String,
    pub applied_at: String,
    pub wallpaper: Option<Wallpaper>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub imported_count: usize,
    pub duplicate_count: usize,
    pub failed_count: usize,
    pub skipped_limit_count: usize,
    pub wallpapers: Vec<Wallpaper>,
}

pub fn check_file_missing(path_str: &str) -> bool {
    !Path::new(path_str).exists()
}
