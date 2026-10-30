import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AppSettings,
  Collection,
  FitMode,
  HistoryEntry,
  ImportSummary,
  Wallpaper,
} from "./types";

const isTauri = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function getImageUrl(path: string): string {
  if (!path) return "";
  if (
    path.startsWith("http://") ||
    path.startsWith("https://") ||
    path.startsWith("data:")
  ) {
    return path;
  }
  if (isTauri()) {
    return convertFileSrc(path);
  }
  return path;
}

export async function fetchInitialState(): Promise<{
  wallpapers: Wallpaper[];
  collections: Collection[];
  settings: AppSettings;
}> {
  if (!isTauri()) {
    return {
      wallpapers: [
        {
          id: "current-os-wp",
          filePath: "C:\\Windows\\Web\\Wallpaper\\Windows\\img0.jpg",
          fileName: "Current Windows Wallpaper",
          thumbnailPath:
            "https://images.unsplash.com/photo-1506744038136-46273834b3fb?auto=format&fit=crop&w=1000&q=80",
          width: 3840,
          height: 2160,
          fileSize: 4210400,
          importDate: new Date().toISOString(),
          lastUsedDate: new Date().toISOString(),
          contentHash: "initial-hash",
          isFavorite: false,
          isActive: true,
          isMissing: false,
          fitMode: "fill",
          collections: [],
        },
      ],
      collections: [
        {
          id: "all",
          name: "All Wallpapers",
          createdAt: "",
          wallpaperCount: 1,
          lastSelectedWallpaperId: null,
          isSystem: true,
        },
        {
          id: "favorites",
          name: "Favorites",
          createdAt: "",
          wallpaperCount: 0,
          lastSelectedWallpaperId: null,
          isSystem: true,
        },
      ],
      settings: {
        globalShortcut: "Alt+W",
        minimizeToTray: true,
        wallpaperScaling: "fill",
        defaultCollectionId: "all",
        historyLimit: 50,
        instantApplyShuffle: false,
        expandCenterOnHover: false,
      },
    };
  }

  const [wallpapers, collections, settings] = await invoke<
    [Wallpaper[], Collection[], AppSettings]
  >("get_initial_state");
  return { wallpapers, collections, settings };
}

export async function pickAndImportFiles(): Promise<ImportSummary | null> {
  if (!isTauri()) return null;
  await invoke("set_dialog_open", { open: true });
  try {
    const selected = await open({
      multiple: true,
      directory: false,
      filters: [
        {
          name: "Wallpapers",
          extensions: ["jpg", "jpeg", "png", "webp", "bmp"],
        },
      ],
    });
    if (!selected) return null;
    const paths = Array.isArray(selected) ? selected : [selected];
    if (paths.length === 0) return null;
    return await invoke<ImportSummary>("import_wallpapers", { paths });
  } finally {
    await invoke("set_dialog_open", { open: false });
  }
}

export async function pickAndImportFolder(): Promise<ImportSummary | null> {
  if (!isTauri()) return null;
  await invoke("set_dialog_open", { open: true });
  try {
    const selected = await open({
      multiple: false,
      directory: true,
    });
    if (!selected) return null;
    const paths = Array.isArray(selected) ? selected : [selected];
    return await invoke<ImportSummary>("import_wallpapers", { paths });
  } finally {
    await invoke("set_dialog_open", { open: false });
  }
}

export async function importPathsDirect(
  paths: string[]
): Promise<ImportSummary | null> {
  if (!isTauri() || paths.length === 0) return null;
  return await invoke<ImportSummary>("import_wallpapers", { paths });
}

export async function applyWallpaper(
  wallpaperId: string,
  fitMode?: FitMode
): Promise<Wallpaper> {
  return await invoke<Wallpaper>("apply_wallpaper", {
    wallpaperId,
    fitMode: fitMode || null,
  });
}

export async function setWallpaperFit(
  wallpaperId: string,
  fitMode: FitMode,
  applyImmediately = false
): Promise<Wallpaper> {
  return await invoke<Wallpaper>("set_wallpaper_fit", {
    wallpaperId,
    fitMode,
    applyImmediately,
  });
}

export async function reorderWallpapers(orderedIds: string[]): Promise<void> {
  if (!isTauri()) return;
  return await invoke<void>("reorder_wallpapers", { orderedIds });
}

export async function undoLastWallpaper(): Promise<Wallpaper | null> {
  if (!isTauri()) return null;
  return await invoke<Wallpaper | null>("undo_last_wallpaper");
}

export async function toggleFavorite(wallpaperId: string): Promise<boolean> {
  return await invoke<boolean>("toggle_favorite", { wallpaperId });
}

export async function removeWallpaper(wallpaperId: string): Promise<void> {
  return await invoke<void>("remove_wallpaper", { wallpaperId });
}

export async function relocateWallpaper(
  wallpaperId: string
): Promise<Wallpaper | null> {
  if (!isTauri()) return null;
  await invoke("set_dialog_open", { open: true });
  try {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [
        {
          name: "Wallpapers",
          extensions: ["jpg", "jpeg", "png", "webp", "bmp"],
        },
      ],
    });
    if (!selected || Array.isArray(selected)) return null;
    return await invoke<Wallpaper>("relocate_wallpaper", {
      wallpaperId,
      newPath: selected,
    });
  } finally {
    await invoke("set_dialog_open", { open: false });
  }
}

export async function createCollection(name: string): Promise<Collection> {
  return await invoke<Collection>("create_collection", { name });
}

export async function deleteCollection(id: string): Promise<void> {
  return await invoke<void>("delete_collection", { id });
}

export async function addToCollection(
  collectionId: string,
  wallpaperId: string
): Promise<void> {
  return await invoke<void>("add_to_collection", { collectionId, wallpaperId });
}

export async function removeFromCollection(
  collectionId: string,
  wallpaperId: string
): Promise<void> {
  return await invoke<void>("remove_from_collection", {
    collectionId,
    wallpaperId,
  });
}

export async function fetchHistory(): Promise<HistoryEntry[]> {
  if (!isTauri()) return [];
  return await invoke<HistoryEntry[]>("get_history");
}

export async function saveSettings(settings: AppSettings): Promise<void> {
  if (!isTauri()) return;
  return await invoke<void>("save_settings", { settings });
}

export async function openInExplorer(filePath: string): Promise<void> {
  if (!isTauri()) return;
  return await invoke<void>("open_in_explorer", { filePath });
}

export async function hideSwitcherWindow(): Promise<void> {
  if (!isTauri()) return;
  return await invoke<void>("hide_switcher_window");
}

export async function exitApp(): Promise<void> {
  if (!isTauri()) return;
  return await invoke<void>("exit_app");
}
