export type FitMode = "fill" | "fit" | "stretch" | "tile" | "center" | "span";

export interface Wallpaper {
  id: string;
  filePath: string;
  fileName: string;
  thumbnailPath: string;
  width: number;
  height: number;
  fileSize: number;
  importDate: string;
  lastUsedDate: string | null;
  contentHash: string;
  isFavorite: boolean;
  isActive: boolean;
  isMissing: boolean;
  fitMode: FitMode;
  collections: string[];
}

export interface Collection {
  id: string;
  name: string;
  createdAt: string;
  wallpaperCount: number;
  lastSelectedWallpaperId: string | null;
  isSystem: boolean;
}

export interface AppSettings {
  globalShortcut: string;
  minimizeToTray: boolean;
  wallpaperScaling: FitMode;
  defaultCollectionId: string;
  historyLimit: number;
  instantApplyShuffle: boolean;
  expandCenterOnHover: boolean;
}

export interface HistoryEntry {
  id: number;
  wallpaperId: string;
  appliedAt: string;
  wallpaper: Wallpaper | null;
}

export interface ImportSummary {
  importedCount: number;
  duplicateCount: number;
  failedCount: number;
  skippedLimitCount?: number;
  wallpapers: Wallpaper[];
}
