import React, { useCallback, useEffect, useState } from "react";
import { AppSettings, Collection, Wallpaper } from "./types";
import { fetchInitialState } from "./api";
import { QuickSwitcher } from "./components/QuickSwitcher";

export const App: React.FC = () => {
  const [wallpapers, setWallpapers] = useState<Wallpaper[]>([]);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [settings, setSettings] = useState<AppSettings>({
    globalShortcut: "Alt+W",
    minimizeToTray: true,
    wallpaperScaling: "fill",
    defaultCollectionId: "all",
    historyLimit: 50,
    instantApplyShuffle: false,
    expandCenterOnHover: false,
  });

  const loadData = useCallback(async () => {
    try {
      const data = await fetchInitialState();
      setWallpapers(data.wallpapers);
      setCollections(data.collections);
      setSettings(data.settings);
    } catch (err) {
      console.error("Failed to fetch MicroSwap state:", err);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Subscribe to backend events so the Carousel stays in sync
  useEffect(() => {
    let unlistenLibrary: (() => void) | undefined;
    let unlistenSwitcher: (() => void) | undefined;

    const subscribe = async () => {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        const { listen } = await import("@tauri-apps/api/event");
        unlistenLibrary = await listen("library-updated", () => {
          loadData();
        });
        unlistenSwitcher = await listen("switcher-opened", () => {
          loadData();
        });
      }
    };

    subscribe();
    return () => {
      if (unlistenLibrary) unlistenLibrary();
      if (unlistenSwitcher) unlistenSwitcher();
    };
  }, [loadData]);

  return (
    <QuickSwitcher
      wallpapers={wallpapers}
      collections={collections}
      settings={settings}
      onRefresh={loadData}
    />
  );
};

export default App;
