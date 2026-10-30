import React, { useEffect, useMemo, useState, useCallback } from "react";
import {
  Settings,
  Monitor,
  LogOut,
  Plus,
  FolderPlus,
  Shuffle,
  Check,
  Maximize2,
  FolderOpen,
  Trash2,
  RotateCcw,
  Keyboard,
  Power,
  ChevronRight,
  ChevronLeft,
  SlidersHorizontal,
} from "lucide-react";
import {
  AppSettings,
  Collection,
  FitMode,
  ImportSummary,
  Wallpaper,
} from "../types";
import {
  applyWallpaper,
  createCollection,
  exitApp,
  getImageUrl,
  hideSwitcherWindow,
  importPathsDirect,
  openInExplorer,
  pickAndImportFiles,
  pickAndImportFolder,
  removeWallpaper,
  reorderWallpapers,
  saveSettings,
  setWallpaperFit,
  undoLastWallpaper,
} from "../api";

export const MAX_WALLPAPERS = 30;

interface QuickSwitcherProps {
  wallpapers: Wallpaper[];
  collections: Collection[];
  settings: AppSettings;
  onRefresh: () => void;
}

type CarouselItem =
  | { kind: "wallpaper"; wp: Wallpaper; id: string }
  | { kind: "add"; id: string };

const FIT_MODES: { value: FitMode; label: string; desc: string }[] = [
  { value: "fill", label: "Fill", desc: "Crop & fill screen (16:9)" },
  { value: "fit", label: "Fit", desc: "Letterbox whole image" },
  { value: "stretch", label: "Stretch", desc: "Stretch to edges" },
  { value: "tile", label: "Tile", desc: "Repeat pattern across screen" },
  { value: "center", label: "Center", desc: "Original size centered" },
  { value: "span", label: "Span", desc: "Span across monitors" },
];

/**
 * Computes scaled-up vertical slice metrics with bouncy/reactive response for the slimmer (unexpanded) slices,
 * top/bottom shear, and subtle tilt.
 */
function getSliceMetrics(
  offset: number,
  isExpandedCenter: boolean,
  isHovered: boolean,
  hoveredOffset: number | null,
  isBeingDragged: boolean
) {
  const abs = Math.abs(offset);

  if (abs === 0 && isExpandedCenter) {
    return {
      width: 780,
      height: 460,
      opacity: 1,
      brightness: 1.08,
      translateY: 0,
      scale: 1.03,
      skewX: -3,
      skewY: -1.5,
      rotateZ: -0.8,
      clipPath: "polygon(0% 2.5%, 100% 0%, 100% 97.5%, 0% 100%)",
    };
  }

  // Smooth wave silhouette
  const heightTable = [550, 504, 448, 392, 336, 282, 232, 188, 152, 120];
  const widthTable = [152, 132, 114, 98, 86, 74, 64, 54, 46, 38];

  const baseHeight = heightTable[Math.min(abs, heightTable.length - 1)];
  const baseWidth = widthTable[Math.min(abs, widthTable.length - 1)];

  // Proximity ripple: slices adjacent to the hovered slice also react!
  const distToHover =
    hoveredOffset !== null ? Math.abs(offset - hoveredOffset) : 99;
  const hoverHeightBoost =
    isHovered && abs !== 0
      ? 42
      : distToHover === 1 && abs !== 0
      ? 18
      : 0;
  const hoverWidthBoost =
    isHovered && abs !== 0
      ? 18
      : distToHover === 1 && abs !== 0
      ? 7
      : 0;

  const height = baseHeight + hoverHeightBoost;
  const width = baseWidth + hoverWidthBoost;
  const opacity = isBeingDragged
    ? 0.92
    : abs > 7
    ? 0.38
    : abs > 5
    ? 0.74
    : 1;
  const brightness =
    abs === 0
      ? 1.14
      : isHovered
      ? 1.1
      : distToHover === 1
      ? 0.98
      : Math.max(0.48, 1 - abs * 0.065);

  const translateY = isBeingDragged
    ? -18
    : abs === 0
    ? -10
    : isHovered
    ? -12
    : distToHover === 1
    ? -5
    : 0;
  const scale = isBeingDragged
    ? 1.08
    : abs === 0
    ? 1.045
    : isHovered
    ? 1.06
    : distToHover === 1
    ? 1.02
    : 1;

  // Dynamic tilt & top/bottom shear
  const skewX = -4.5;
  const skewY = offset === 0 ? -2.2 : offset < 0 ? -3.2 : 2.6;
  const rotateZ = isHovered
    ? offset * 0.2 - 0.5
    : offset * 0.35 - 1.2;

  // Slanted top and bottom edges (sheared parallelogram silhouette)
  const clipPath =
    offset < 0
      ? "polygon(0% 5.5%, 100% 0%, 100% 94.5%, 0% 100%)"
      : offset > 0
      ? "polygon(0% 0%, 100% 5.5%, 100% 100%, 0% 94.5%)"
      : "polygon(0% 3.5%, 100% 0%, 100% 96.5%, 0% 100%)";

  return {
    width,
    height,
    opacity,
    brightness,
    translateY,
    scale,
    skewX,
    skewY,
    rotateZ,
    clipPath,
  };
}

export const QuickSwitcher: React.FC<QuickSwitcherProps> = ({
  wallpapers,
  collections,
  settings,
  onRefresh,
}) => {
  const [selectedCollectionId, setSelectedCollectionId] =
    useState<string>("all");
  const [orderedWallpapers, setOrderedWallpapers] =
    useState<Wallpaper[]>(wallpapers);
  const [selectedIndex, setSelectedIndex] = useState<number>(0);
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [didDragRef] = useState<{ moved: boolean }>({ moved: false });
  const [expandedCenter, setExpandedCenter] = useState<boolean>(false);
  const [topBarExpanded, setTopBarExpanded] = useState<boolean>(false);
  const [applying, setApplying] = useState<boolean>(false);
  const [applyPhase, setApplyPhase] = useState<"idle" | "portal" | "dissolve">(
    "idle"
  );
  const [importing, setImporting] = useState<boolean>(false);
  const [statusToast, setStatusToast] = useState<string | null>(null);

  // Floating HUD popovers (Monochrome White Glass)
  const [showConfigPopover, setShowConfigPopover] = useState<boolean>(false);
  const [showFitPopover, setShowFitPopover] = useState<boolean>(false);
  const [shortcutInput, setShortcutInput] = useState<string>(
    settings.globalShortcut
  );
  const [newTagInput, setNewTagInput] = useState<string>("");
  const [isDraggingOver, setIsDraggingOver] = useState<boolean>(false);
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    wp: Wallpaper;
  } | null>(null);

  useEffect(() => {
    setShortcutInput(settings.globalShortcut);
  }, [settings.globalShortcut]);

  useEffect(() => {
    if (!draggingId) {
      setOrderedWallpapers(wallpapers);
    }
  }, [wallpapers, draggingId]);

  // Exclude Favorites from collections
  const nonFavoriteCollections = useMemo(
    () => collections.filter((c) => c.id !== "favorites"),
    [collections]
  );

  const filteredWallpapers = useMemo(() => {
    if (selectedCollectionId === "all") return orderedWallpapers;
    return orderedWallpapers.filter((w) =>
      w.collections.includes(selectedCollectionId)
    );
  }, [orderedWallpapers, selectedCollectionId]);

  const isAtCapacity = wallpapers.length >= MAX_WALLPAPERS;

  // Build unified items array where "+ Add Wallpaper" is a real selectable carousel item (unless at capacity)
  const carouselItems = useMemo<CarouselItem[]>(() => {
    const items: CarouselItem[] = filteredWallpapers.map((wp) => ({
      kind: "wallpaper",
      wp,
      id: wp.id,
    }));
    if (!isAtCapacity) {
      items.push({ kind: "add", id: "__add_wallpaper_slice__" });
    }
    return items;
  }, [filteredWallpapers, isAtCapacity]);

  useEffect(() => {
    setSelectedIndex((prev) =>
      Math.min(prev, Math.max(0, carouselItems.length - 1))
    );
  }, [selectedCollectionId, carouselItems.length]);

  const selectedItem = carouselItems[selectedIndex] || carouselItems[0];
  const selectedWallpaper =
    selectedItem?.kind === "wallpaper" ? selectedItem.wp : null;

  const currentFitMode: FitMode =
    selectedWallpaper?.fitMode || settings.wallpaperScaling || "fill";

  const showToast = (msg: string) => {
    setStatusToast(msg);
    setTimeout(() => setStatusToast(null), 2600);
  };

  const formatImportFeedback = (res: ImportSummary) => {
    if (res.skippedLimitCount && res.skippedLimitCount > 0) {
      return `Added ${res.importedCount} (Max limit of ${MAX_WALLPAPERS} reached; ${res.skippedLimitCount} skipped)`;
    }
    if (res.importedCount > 0) {
      return `Added ${res.importedCount} wallpaper(s) (${
        wallpapers.length + res.importedCount
      }/${MAX_WALLPAPERS})`;
    }
    if (res.duplicateCount > 0) {
      return `Skipped ${res.duplicateCount} duplicate wallpaper(s)`;
    }
    return "No supported wallpapers found";
  };

  // Drag-and-Drop directly onto the floating Carousel
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    const setupDrop = async () => {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        const { getCurrentWebview } = await import("@tauri-apps/api/webview");
        unlisten = await getCurrentWebview().onDragDropEvent(async (event) => {
          if (event.payload.type === "over") {
            setIsDraggingOver(true);
          } else if (event.payload.type === "leave") {
            setIsDraggingOver(false);
          } else if (event.payload.type === "drop") {
            setIsDraggingOver(false);
            const paths = event.payload.paths;
            if (paths && paths.length > 0) {
              if (wallpapers.length >= MAX_WALLPAPERS) {
                showToast(`Limit of ${MAX_WALLPAPERS} wallpapers reached`);
                return;
              }
              try {
                setImporting(true);
                showToast("Importing wallpapers...");
                const res = await importPathsDirect(paths);
                if (res) {
                  onRefresh();
                  showToast(formatImportFeedback(res));
                }
              } finally {
                setImporting(false);
              }
            }
          }
        });
      }
    };
    setupDrop();
    return () => {
      if (unlisten) unlisten();
    };
  }, [onRefresh, wallpapers.length]);

  const handleClose = useCallback(async () => {
    setExpandedCenter(false);
    setApplyPhase("idle");
    setShowConfigPopover(false);
    setShowFitPopover(false);
    setContextMenu(null);
    await hideSwitcherWindow();
  }, []);

  const handleAddFiles = useCallback(async () => {
    if (importing) return;
    if (wallpapers.length >= MAX_WALLPAPERS) {
      showToast(`Maximum limit of ${MAX_WALLPAPERS} wallpapers reached`);
      return;
    }
    setShowConfigPopover(false);
    try {
      setImporting(true);
      const res = await pickAndImportFiles();
      if (res) {
        onRefresh();
        showToast(formatImportFeedback(res));
      }
    } finally {
      setImporting(false);
    }
  }, [importing, wallpapers.length, onRefresh]);

  const handleAddFolder = useCallback(async () => {
    if (importing) return;
    if (wallpapers.length >= MAX_WALLPAPERS) {
      showToast(`Maximum limit of ${MAX_WALLPAPERS} wallpapers reached`);
      return;
    }
    setShowConfigPopover(false);
    try {
      setImporting(true);
      const res = await pickAndImportFolder();
      if (res) {
        onRefresh();
        showToast(formatImportFeedback(res));
      }
    } finally {
      setImporting(false);
    }
  }, [importing, wallpapers.length, onRefresh]);

  const handleActivateSelected = useCallback(
    async (overrideFit?: FitMode) => {
      if (!selectedItem) return;
      if (selectedItem.kind === "add") {
        await handleAddFiles();
        return;
      }

      const wp = selectedItem.wp;
      if (applying) return;
      if (wp.isMissing) {
        showToast("Source file is missing on disk");
        return;
      }
      const modeToUse = overrideFit || currentFitMode;
      try {
        setApplying(true);
        setShowConfigPopover(false);
        setShowFitPopover(false);
        setContextMenu(null);

        // Single continuous GPU keyframe animation (no mid-flight React re-render or layout thrash!)
        setApplyPhase("portal");
        const applyPromise = applyWallpaper(wp.id, modeToUse);

        // Wait for the 560ms continuous bloom + blur-dissolve keyframe to reach 0% opacity
        await new Promise((resolve) => setTimeout(resolve, 540));
        await applyPromise;
        await handleClose();
        onRefresh();
      } catch (err: any) {
        setApplyPhase("idle");
        showToast(typeof err === "string" ? err : "Failed to apply wallpaper");
      } finally {
        setApplying(false);
      }
    },
    [
      selectedItem,
      applying,
      currentFitMode,
      onRefresh,
      handleClose,
      handleAddFiles,
    ]
  );

  const handleSelectFitMode = useCallback(
    async (mode: FitMode, applyNow = false) => {
      if (!selectedWallpaper) return;
      try {
        await setWallpaperFit(
          selectedWallpaper.id,
          mode,
          applyNow || selectedWallpaper.isActive
        );
        onRefresh();
        showToast(
          selectedWallpaper.isActive || applyNow
            ? `Applied ${mode.toUpperCase()} mode`
            : `Fit mode: ${mode.toUpperCase()}`
        );
      } catch {
        showToast("Could not update fit mode");
      }
    },
    [selectedWallpaper, onRefresh]
  );

  const handleCycleFitMode = useCallback(async () => {
    const idx = FIT_MODES.findIndex((m) => m.value === currentFitMode);
    const next = FIT_MODES[(idx + 1) % FIT_MODES.length];
    await handleSelectFitMode(next.value, false);
  }, [currentFitMode, handleSelectFitMode]);

  const handleShuffle = useCallback(async () => {
    if (filteredWallpapers.length <= 1) return;
    const candidates = carouselItems
      .map((item, idx) => ({ item, idx }))
      .filter(
        ({ item, idx }) =>
          item.kind === "wallpaper" &&
          idx !== selectedIndex &&
          !item.wp.isMissing
      );
    if (candidates.length === 0) return;
    const randomPick =
      candidates[Math.floor(Math.random() * candidates.length)];
    setSelectedIndex(randomPick.idx);

    if (
      settings.instantApplyShuffle &&
      randomPick.item.kind === "wallpaper"
    ) {
      await applyWallpaper(
        randomPick.item.wp.id,
        randomPick.item.wp.fitMode
      );
      onRefresh();
      showToast(`Shuffled & applied "${randomPick.item.wp.fileName}"`);
    } else if (randomPick.item.kind === "wallpaper") {
      showToast(`Shuffled to "${randomPick.item.wp.fileName}"`);
    }
  }, [
    filteredWallpapers.length,
    carouselItems,
    selectedIndex,
    settings.instantApplyShuffle,
    onRefresh,
  ]);

  const handleCycleCollection = useCallback(() => {
    if (nonFavoriteCollections.length <= 1) return;
    const idx = nonFavoriteCollections.findIndex(
      (c) => c.id === selectedCollectionId
    );
    const next =
      nonFavoriteCollections[(idx + 1) % nonFavoriteCollections.length];
    setSelectedCollectionId(next.id);
    showToast(`View: ${next.name}`);
  }, [nonFavoriteCollections, selectedCollectionId]);

  const handleDeleteWallpaper = useCallback(
    async (wp: Wallpaper) => {
      await removeWallpaper(wp.id);
      setContextMenu(null);
      onRefresh();
      showToast(`Removed "${wp.fileName}"`);
    },
    [onRefresh]
  );

  const handleUndo = useCallback(async () => {
    const restored = await undoLastWallpaper();
    if (restored) {
      onRefresh();
      showToast(`Reverted to "${restored.fileName}"`);
    } else {
      showToast("No previous wallpaper in history");
    }
  }, [onRefresh]);

  // Finish pointer-based drag reordering and persist to SQLite
  useEffect(() => {
    if (!draggingId) return;
    const onWindowMouseUp = async () => {
      const finishedDrag = didDragRef.moved;
      setDraggingId(null);
      if (finishedDrag) {
        await reorderWallpapers(orderedWallpapers.map((w) => w.id));
        onRefresh();
        showToast("Order saved");
      }
      setTimeout(() => {
        didDragRef.moved = false;
      }, 60);
    };
    window.addEventListener("mouseup", onWindowMouseUp);
    return () => window.removeEventListener("mouseup", onWindowMouseUp);
  }, [draggingId, orderedWallpapers, didDragRef, onRefresh]);

  const handleDragHoverSwap = useCallback(
    (targetWallpaperId: string) => {
      if (!draggingId || draggingId === targetWallpaperId) return;
      didDragRef.moved = true;
      setOrderedWallpapers((prev) => {
        const fromIdx = prev.findIndex((w) => w.id === draggingId);
        const toIdx = prev.findIndex((w) => w.id === targetWallpaperId);
        if (fromIdx === -1 || toIdx === -1 || fromIdx === toIdx) return prev;
        const next = [...prev];
        const [moved] = next.splice(fromIdx, 1);
        next.splice(toIdx, 0, moved);
        setSelectedIndex(toIdx);
        return next;
      });
    },
    [draggingId, didDragRef]
  );

  // Keyboard navigation & shortcuts inside Carousel
  useEffect(() => {
    const onKeyDown = async (e: KeyboardEvent) => {
      if (
        document.activeElement &&
        ["INPUT", "TEXTAREA"].includes(document.activeElement.tagName)
      ) {
        return;
      }

      if (e.key === "ArrowLeft") {
        e.preventDefault();
        setSelectedIndex((prev) =>
          carouselItems.length > 0
            ? (prev - 1 + carouselItems.length) % carouselItems.length
            : 0
        );
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        setSelectedIndex((prev) =>
          carouselItems.length > 0 ? (prev + 1) % carouselItems.length : 0
        );
      } else if (e.key === "Enter") {
        e.preventDefault();
        await handleActivateSelected();
      } else if (e.key === "Escape") {
        e.preventDefault();
        if (showConfigPopover || showFitPopover || contextMenu) {
          setShowConfigPopover(false);
          setShowFitPopover(false);
          setContextMenu(null);
        } else {
          await handleClose();
        }
      } else if (e.key === " ") {
        e.preventDefault();
        if (selectedItem?.kind === "add") {
          await handleAddFiles();
        } else {
          setExpandedCenter((prev) => !prev);
        }
      } else if (e.key.toLowerCase() === "m") {
        e.preventDefault();
        await handleCycleFitMode();
      } else if (e.key.toLowerCase() === "s" && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        await handleShuffle();
      } else if (e.key === "Tab") {
        e.preventDefault();
        handleCycleCollection();
      } else if (
        (e.key === "Delete" || e.key === "Backspace") &&
        selectedWallpaper
      ) {
        e.preventDefault();
        await handleDeleteWallpaper(selectedWallpaper);
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
        e.preventDefault();
        await handleUndo();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "o") {
        e.preventDefault();
        await handleAddFiles();
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    carouselItems.length,
    selectedItem,
    selectedWallpaper,
    handleActivateSelected,
    handleClose,
    handleCycleFitMode,
    handleShuffle,
    handleCycleCollection,
    handleAddFiles,
    handleDeleteWallpaper,
    handleUndo,
    showConfigPopover,
    showFitPopover,
    contextMenu,
  ]);

  const handleWheel = (e: React.WheelEvent) => {
    if (carouselItems.length <= 1) return;
    if (e.deltaY > 10 || e.deltaX > 10) {
      setSelectedIndex((prev) => (prev + 1) % carouselItems.length);
    } else if (e.deltaY < -10 || e.deltaX < -10) {
      setSelectedIndex(
        (prev) => (prev - 1 + carouselItems.length) % carouselItems.length
      );
    }
  };

  // Build visible window of up to 17 slots (-8 to +8 around selectedIndex)
  const visibleSlots = useMemo(() => {
    const total = carouselItems.length;
    if (total === 0) return [];

    if (total <= 17) {
      return carouselItems.map((item, idx) => ({
        item,
        idx,
        offset: idx - selectedIndex,
        key: item.id,
      }));
    }

    const radius = 8;
    const slots = [];
    for (let offset = -radius; offset <= radius; offset++) {
      const idx = (selectedIndex + offset + total * 10) % total;
      const item = carouselItems[idx];
      slots.push({
        item,
        idx,
        offset,
        key: `${item.id}-slot-${offset}`,
      });
    }
    return slots;
  }, [carouselItems, selectedIndex]);

  const activeCollection =
    nonFavoriteCollections.find((c) => c.id === selectedCollectionId) ||
    nonFavoriteCollections[0];

  const getPreviewObjectStyle = (mode: FitMode): React.CSSProperties => {
    if (!expandedCenter) {
      return { objectFit: "cover", objectPosition: "center" };
    }
    switch (mode) {
      case "fit":
        return { objectFit: "contain", backgroundColor: "#050505" };
      case "stretch":
        return { objectFit: "fill" };
      case "center":
        return { objectFit: "none", backgroundColor: "#050505" };
      default:
        return { objectFit: "cover", objectPosition: "center" };
    }
  };

  const hoveredOffset =
    hoveredIndex !== null ? hoveredIndex - selectedIndex : null;

  // Snappy, extra-bouncy spring cubic-bezier for the slimmer slices
  const bouncyTransition =
    "width 340ms cubic-bezier(0.32, 1.75, 0.52, 1), height 340ms cubic-bezier(0.32, 1.75, 0.52, 1), transform 320ms cubic-bezier(0.32, 1.75, 0.52, 1), clip-path 340ms cubic-bezier(0.32, 1.75, 0.52, 1), filter 160ms ease, opacity 160ms ease";

  return (
    <div
      onWheel={handleWheel}
      onClick={() => {
        setContextMenu(null);
      }}
      className="relative w-full h-screen flex items-center justify-center select-none overflow-hidden bg-transparent"
    >
      {/* Drag-and-Drop Overlay (Pure White Glass) */}
      {isDraggingOver && (
        <div className="fixed inset-6 z-50 rounded-3xl bg-white/10 border border-dashed border-white/40 backdrop-blur-2xl flex flex-col items-center justify-center pointer-events-none">
          <Plus className="w-14 h-14 text-white mb-2 animate-bounce" />
          <p className="text-base font-medium text-white tracking-wide">
            Drop wallpapers or folders into MicroSwap
          </p>
        </div>
      )}

      {/* Centered Pitch-Black Capsule Top Bar (Fades out during portal apply) */}
      <div
        className={`absolute top-4 left-1/2 -translate-x-1/2 z-30 flex items-center justify-center transition-all duration-200 ${
          applyPhase !== "idle"
            ? "opacity-0 -translate-y-3 pointer-events-none"
            : "opacity-100"
        }`}
      >
        {!topBarExpanded ? (
          <button
            onClick={(e) => {
              e.stopPropagation();
              setTopBarExpanded(true);
            }}
            title="Expand Fit & Collection Bar"
            className="flex items-center gap-2.5 h-10 px-4 rounded-full bg-black hover:bg-[#0c0c0e] text-xs font-medium text-white tracking-wider transition-all duration-300 hover:scale-105 active:scale-95"
          >
            <SlidersHorizontal className="w-3.5 h-3.5 text-white/90 stroke-[2]" />
            <span className="uppercase font-semibold">{currentFitMode}</span>
            <ChevronRight className="w-3.5 h-3.5 text-white/65 stroke-[2]" />
          </button>
        ) : (
          <div
            onClick={(e) => e.stopPropagation()}
            className="flex items-center gap-2.5 transition-all duration-300"
          >
            {/* 1. Left Circular Collapse Button */}
            <button
              onClick={() => setTopBarExpanded(false)}
              title="Collapse Bar"
              className="w-10 h-10 rounded-full bg-black hover:bg-[#0c0c0e] flex items-center justify-center transition-colors"
            >
              <ChevronLeft className="w-4 h-4 text-white stroke-[2]" />
            </button>

            {/* 2. Center Segmented Pill (Fit Modes) */}
            <div className="h-10 px-3 rounded-full bg-black flex items-center">
              {FIT_MODES.map((m, i) => {
                const active = currentFitMode === m.value;
                return (
                  <React.Fragment key={m.value}>
                    {i > 0 && (
                      <div className="w-[1px] h-3 bg-white/[0.15] mx-1" />
                    )}
                    <button
                      onClick={() => handleSelectFitMode(m.value, false)}
                      title={`${m.label}: ${m.desc} (Shortcut: M)`}
                      className={`px-2.5 py-1 rounded-full text-[11px] font-medium tracking-wider transition-all ${
                        active
                          ? "text-white font-semibold bg-white/[0.16]"
                          : "text-white/65 hover:text-white"
                      }`}
                    >
                      {m.label}
                    </button>
                  </React.Fragment>
                );
              })}
            </div>

            {/* Optional 3rd Separate Capsule for Collection Switching */}
            {nonFavoriteCollections.length > 1 && (
              <button
                onClick={handleCycleCollection}
                title="Click or press Tab to switch collection"
                className="h-10 px-4 rounded-full bg-black hover:bg-[#0c0c0e] flex items-center gap-2 text-[11px] font-medium tracking-wider text-white transition-colors"
              >
                <span>{activeCollection?.name || "All"}</span>
              </button>
            )}
          </div>
        )}
      </div>

      {/* Status Toast */}
      {statusToast && (
        <div className="absolute top-4 right-16 z-40 px-4 py-1.5 rounded-full bg-black text-xs font-medium text-white">
          {statusToast}
        </div>
      )}

      {/* Main Stage: Scaled-Up Sheared/Tilted Wave Carousel + Closer Compact Right Circular Buttons */}
      <div className="relative flex items-center justify-center gap-6 px-8">
        {/* Wave Strip */}
        <div className="relative flex items-center justify-center gap-[10px] py-8">
          {visibleSlots.map(({ item, idx, offset, key }) => {
            const isSelected = offset === 0;
            const isHovered = hoveredIndex === idx;
            const isBeingDragged =
              item.kind === "wallpaper" && draggingId === item.wp.id;
            const {
              width,
              height,
              opacity,
              brightness,
              translateY,
              scale,
              skewX,
              skewY,
              rotateZ,
              clipPath,
            } = getSliceMetrics(
              offset,
              expandedCenter || settings.expandCenterOnHover,
              isHovered,
              hoveredOffset,
              isBeingDragged
            );

            const isApplyingPortal = applyPhase !== "idle";
            const sliceAnimClass = isApplyingPortal
              ? isSelected
                ? "portal-center-anim"
                : "portal-neighbor-anim"
              : "";

            const sliceTransform = `translateY(${translateY}px) skewX(${skewX}deg) skewY(${skewY}deg) rotateZ(${rotateZ}deg) scale(${scale})`;

            // Render selectable "+ Add Wallpaper" slice right inside the wave (scales proportionally with slice width!)
            if (item.kind === "add") {
              const contentScale = Math.max(0.32, Math.min(1, width / 135));
              const showAddLabels = isSelected || isHovered || Math.abs(offset) <= 1;

              return (
                <div
                  key={key}
                  onMouseEnter={() => setHoveredIndex(idx)}
                  onMouseLeave={() => setHoveredIndex(null)}
                  onClick={(e) => {
                    e.stopPropagation();
                    if (isSelected) {
                      handleAddFiles();
                    } else {
                      setSelectedIndex(idx);
                    }
                  }}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    handleAddFolder();
                  }}
                  style={{
                    width: `${isSelected && expandedCenter ? 240 : width}px`,
                    height: `${height}px`,
                    opacity,
                    clipPath,
                    transform: sliceTransform,
                    transition: bouncyTransition,
                  }}
                  title={`Select with Arrow Keys and press Enter to Add Wallpapers (${wallpapers.length}/${MAX_WALLPAPERS}) • Right-click to Add Folder`}
                  className={`group relative shrink-0 overflow-hidden cursor-pointer select-none bg-white/[0.09] hover:bg-white/[0.16] backdrop-blur-2xl flex flex-col items-center justify-center text-white ${sliceAnimClass}`}
                >
                  <div
                    style={{
                      transform: `skewX(${-skewX}deg) skewY(${-skewY}deg) scale(${contentScale})`,
                      transition: "transform 320ms cubic-bezier(0.32, 1.75, 0.52, 1)",
                    }}
                    className="flex flex-col items-center justify-center gap-2"
                  >
                    <div
                      className={`w-11 h-11 rounded-full bg-white/15 group-hover:bg-white/25 flex items-center justify-center transition-transform duration-300 ${
                        isSelected
                          ? "scale-110 bg-white text-black"
                          : "text-white"
                      }`}
                    >
                      <Plus className="w-5 h-5" />
                    </div>
                    {showAddLabels && (
                      <div className="flex flex-col items-center px-1 text-center whitespace-nowrap">
                        <span className="text-xs font-semibold tracking-wide text-white">
                          {importing ? "Adding..." : "Add"}
                        </span>
                        <span className="text-[10px] text-white/55 mt-0.5">
                          {wallpapers.length}/{MAX_WALLPAPERS}
                        </span>
                      </div>
                    )}
                  </div>
                </div>
              );
            }

            const wp = item.wp;
            const isTiledPreview =
              isSelected && expandedCenter && currentFitMode === "tile";

            return (
              <div
                key={key}
                onMouseDown={(e) => {
                  if (e.button === 0) {
                    didDragRef.moved = false;
                    setDraggingId(wp.id);
                  }
                }}
                onMouseEnter={() => {
                  setHoveredIndex(idx);
                  if (draggingId) {
                    handleDragHoverSwap(wp.id);
                  }
                }}
                onMouseLeave={() => setHoveredIndex(null)}
                onClick={(e) => {
                  e.stopPropagation();
                  if (didDragRef.moved) return;
                  setContextMenu(null);
                  if (isSelected) {
                    handleActivateSelected();
                  } else {
                    setSelectedIndex(idx);
                  }
                }}
                onContextMenu={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  setSelectedIndex(idx);
                  setContextMenu({
                    x: Math.min(e.clientX, window.innerWidth - 220),
                    y: Math.min(e.clientY, window.innerHeight - 230),
                    wp,
                  });
                }}
                style={{
                  width: `${width}px`,
                  height: `${height}px`,
                  opacity,
                  filter: `brightness(${brightness})`,
                  clipPath,
                  transform: sliceTransform,
                  transition: bouncyTransition,
                }}
                className={`group relative shrink-0 overflow-hidden select-none ${
                  isBeingDragged
                    ? "z-30 cursor-grabbing"
                    : isSelected
                    ? "z-20 cursor-grab"
                    : "z-10 cursor-grab"
                } ${sliceAnimClass}`}
                title={
                  isSelected
                    ? `${wp.fileName} (${currentFitMode.toUpperCase()}) — Click/Enter to apply • Drag to reorder`
                    : `${wp.fileName} — Drag to reorder`
                }
              >
                {isTiledPreview ? (
                  <div
                    className="w-full h-full scale-110"
                    style={{
                      backgroundImage: `url("${getImageUrl(
                        wp.thumbnailPath || wp.filePath
                      )}")`,
                      backgroundRepeat: "repeat",
                      backgroundSize: "160px auto",
                      transform: `skewX(${-skewX}deg)`,
                    }}
                  />
                ) : (
                  <img
                    src={getImageUrl(wp.thumbnailPath || wp.filePath)}
                    alt={wp.fileName}
                    decoding="async"
                    loading="lazy"
                    draggable={false}
                    style={{
                      ...(isSelected
                        ? getPreviewObjectStyle(currentFitMode)
                        : {}),
                      transform: `skewX(${-skewX * 0.7}deg) scale(1.16)`,
                    }}
                    className={`w-full h-full object-cover object-center transition-transform duration-500 ease-out group-hover:scale-125 ${
                      isApplyingPortal && isSelected
                        ? "portal-center-img-anim"
                        : ""
                    }`}
                  />
                )}

                {/* Subtle top/bottom vignette on slices */}
                <div className="pointer-events-none absolute inset-0 bg-gradient-to-b from-black/15 via-transparent to-black/60" />

                {/* Active desktop wallpaper white dot */}
                {wp.isActive && (
                  <div
                    title="Currently applied desktop wallpaper"
                    className="absolute top-4 left-1/2 -translate-x-1/2 w-2 h-2 rounded-full bg-white shadow-[0_0_10px_rgba(255,255,255,0.95)]"
                  />
                )}
              </div>
            );
          })}
        </div>

        {/* Right-Side Pitch-Black Circular HUD (Fades out during portal apply) */}
        <div
          className={`relative z-40 flex flex-col items-center gap-3 ml-2 transition-all duration-200 ${
            applyPhase !== "idle"
              ? "opacity-0 translate-x-4 pointer-events-none"
              : "opacity-100"
          }`}
        >
          {/* 1. Top Circular Button: Settings & Upload Popover */}
          <button
            onClick={(e) => {
              e.stopPropagation();
              setShowFitPopover(false);
              setShowConfigPopover((prev) => !prev);
            }}
            title="Settings, Hotkey & Add Wallpapers"
            className={`w-11 h-11 rounded-full flex items-center justify-center transition-all duration-300 hover:scale-105 active:scale-95 ${
              showConfigPopover
                ? "bg-white text-black"
                : "bg-black hover:bg-[#0c0c0e] text-white"
            }`}
          >
            <Settings className="w-4 h-4 stroke-[2]" />
          </button>

          {/* 2. Middle Circular Button: Windows Fit Mode & Apply */}
          <button
            onClick={(e) => {
              e.stopPropagation();
              setShowConfigPopover(false);
              setShowFitPopover((prev) => !prev);
            }}
            onDoubleClick={() => handleActivateSelected()}
            title={`Fit Mode: ${currentFitMode.toUpperCase()} (Click to choose Fill / Fit / Stretch / Tile / Center / Span • Double-click to Apply)`}
            className={`w-13 h-13 p-3 rounded-full flex flex-col items-center justify-center transition-all duration-300 hover:scale-105 active:scale-95 ${
              showFitPopover
                ? "bg-white text-black"
                : "bg-black hover:bg-[#0c0c0e] text-white"
            }`}
          >
            <Monitor className="w-4 h-4 stroke-[2]" />
            <span className="text-[8px] font-semibold uppercase tracking-widest mt-0.5">
              {currentFitMode}
            </span>
          </button>

          {/* 3. Bottom Circular Button: Close Carousel (Right-click to Quit MicroSwap) */}
          <button
            onClick={handleClose}
            onContextMenu={async (e) => {
              e.preventDefault();
              await exitApp();
            }}
            title="Close Carousel (Esc) • Right-click to Exit MicroSwap"
            className="w-11 h-11 rounded-full bg-black hover:bg-[#0c0c0e] flex items-center justify-center text-white transition-all duration-300 hover:scale-105 active:scale-95"
          >
            <LogOut className="w-4 h-4 stroke-[2]" />
          </button>

          {/* Floating Popover 1: Top Gear Button (Pitch-black, no outline, no shadow, medium text) */}
          {showConfigPopover && (
            <div
              onClick={(e) => e.stopPropagation()}
              className="absolute right-full mr-4 top-1/2 -translate-y-1/2 z-50 w-72 p-5 rounded-3xl bg-black space-y-4 text-xs font-medium text-white"
            >
              <div className="flex items-center justify-between border-b border-white/[0.1] pb-2.5">
                <span className="text-sm font-semibold tracking-wider">
                  MicroSwap
                </span>
                <span className="text-[10px] text-white/65">
                  {wallpapers.length} / {MAX_WALLPAPERS} slots
                </span>
              </div>

              <div className="grid grid-cols-2 gap-2">
                <button
                  onClick={handleAddFiles}
                  disabled={isAtCapacity || importing}
                  className="flex items-center justify-center gap-1.5 py-2 rounded-full bg-white text-black hover:bg-white/90 disabled:opacity-40 font-semibold transition"
                >
                  <Plus className="w-3.5 h-3.5 stroke-[2]" />
                  Add Files
                </button>
                <button
                  onClick={handleAddFolder}
                  disabled={isAtCapacity || importing}
                  className="flex items-center justify-center gap-1.5 py-2 rounded-full bg-[#111114] hover:bg-[#1a1a1f] disabled:opacity-40 font-medium text-white transition"
                >
                  <FolderPlus className="w-3.5 h-3.5 stroke-[2]" />
                  Add Folder
                </button>
              </div>

              <div className="space-y-1.5">
                <label className="flex items-center gap-1.5 text-[11px] text-white/70">
                  <Keyboard className="w-3.5 h-3.5 stroke-[2]" />
                  Global Summon Hotkey
                </label>
                <div className="flex gap-1.5">
                  <input
                    type="text"
                    value={shortcutInput}
                    onChange={(e) => setShortcutInput(e.target.value)}
                    placeholder="e.g. Alt+W"
                    className="flex-1 px-3 py-1.5 rounded-full bg-[#111114] text-xs font-medium text-white focus:outline-none"
                  />
                  <button
                    onClick={async () => {
                      await saveSettings({
                        ...settings,
                        globalShortcut: shortcutInput.trim() || "Alt+W",
                      });
                      onRefresh();
                      showToast(`Hotkey updated to ${shortcutInput}`);
                    }}
                    className="px-3.5 py-1.5 rounded-full bg-[#111114] hover:bg-[#1a1a1f] font-medium transition"
                  >
                    Save
                  </button>
                </div>
              </div>

              <form
                onSubmit={async (e) => {
                  e.preventDefault();
                  if (!newTagInput.trim()) return;
                  const col = await createCollection(newTagInput.trim());
                  setNewTagInput("");
                  onRefresh();
                  setSelectedCollectionId(col.id);
                  showToast(`Created collection "${col.name}"`);
                }}
                className="space-y-1.5"
              >
                <label className="text-[11px] text-white/70">
                  New Collection Filter
                </label>
                <div className="flex gap-1.5">
                  <input
                    type="text"
                    value={newTagInput}
                    onChange={(e) => setNewTagInput(e.target.value)}
                    placeholder="e.g. Minimal, Dark..."
                    className="flex-1 px-3 py-1.5 rounded-full bg-[#111114] text-xs font-medium text-white focus:outline-none"
                  />
                  <button
                    type="submit"
                    className="px-3.5 py-1.5 rounded-full bg-[#111114] hover:bg-[#1a1a1f] font-medium transition"
                  >
                    Add
                  </button>
                </div>
              </form>

              <div className="pt-2 border-t border-white/[0.1] flex items-center justify-between">
                <button
                  onClick={handleUndo}
                  className="inline-flex items-center gap-1.5 text-white/80 hover:text-white transition"
                >
                  <RotateCcw className="w-3.5 h-3.5 stroke-[2]" />
                  Undo Last Swap
                </button>
                <button
                  onClick={() => exitApp()}
                  className="inline-flex items-center gap-1 text-white/80 hover:text-white transition"
                >
                  <Power className="w-3.5 h-3.5 stroke-[2]" />
                  Quit App
                </button>
              </div>
            </div>
          )}

          {/* Floating Popover 2: Middle Monitor Button (Pitch-black, no outline, no shadow) */}
          {showFitPopover && (
            <div
              onClick={(e) => e.stopPropagation()}
              className="absolute right-full mr-4 top-1/2 -translate-y-1/2 z-50 w-64 p-4 rounded-3xl bg-black space-y-2 text-xs font-medium text-white"
            >
              <div className="px-1 pb-1.5 border-b border-white/[0.1] flex items-center justify-between">
                <span className="font-semibold tracking-wider">
                  Windows Desktop Fit
                </span>
                <span className="text-[10px] text-white/60">Key: M</span>
              </div>
              <div className="space-y-1">
                {FIT_MODES.map((m) => {
                  const isSelected = currentFitMode === m.value;
                  return (
                    <button
                      key={m.value}
                      onClick={async () => {
                        await handleSelectFitMode(m.value, true);
                        setShowFitPopover(false);
                      }}
                      className={`w-full px-3 py-2 rounded-2xl flex items-center justify-between transition ${
                        isSelected
                          ? "bg-white text-black font-semibold"
                          : "hover:bg-[#111114] text-white/90"
                      }`}
                    >
                      <div className="text-left">
                        <div>{m.label}</div>
                        <div
                          className={`text-[10px] ${
                            isSelected ? "text-black/70" : "text-white/55"
                          }`}
                        >
                          {m.desc}
                        </div>
                      </div>
                      {isSelected && <Check className="w-4 h-4 shrink-0" />}
                    </button>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Right-Click Context Menu on any Slice */}
      {contextMenu && (
        <div
          onClick={(e) => e.stopPropagation()}
          style={{ left: contextMenu.x, top: contextMenu.y }}
          className="fixed z-50 w-52 py-2 rounded-2xl bg-black text-xs font-medium text-white"
        >
          <div className="px-3.5 py-1 border-b border-white/10 text-[10px] text-white/60 truncate">
            {contextMenu.wp.fileName}
          </div>
          <div className="px-2.5 py-2 border-b border-white/10">
            <p className="px-1 text-[10px] text-white/60 mb-1.5">
              Apply with Windows Fit:
            </p>
            <div className="grid grid-cols-3 gap-1">
              {FIT_MODES.map((m) => (
                <button
                  key={m.value}
                  onClick={async () => {
                    setContextMenu(null);
                    await applyWallpaper(contextMenu.wp.id, m.value);
                    onRefresh();
                    showToast(
                      `Applied "${contextMenu.wp.fileName}" (${m.label})`
                    );
                  }}
                  className={`py-1 rounded-lg text-[10px] font-semibold transition ${
                    contextMenu.wp.fitMode === m.value
                      ? "bg-white text-black"
                      : "bg-white/10 hover:bg-white/20 text-white/90"
                  }`}
                >
                  {m.label}
                </button>
              ))}
            </div>
          </div>

          <button
            onClick={async () => {
              const p = contextMenu.wp.filePath;
              setContextMenu(null);
              await openInExplorer(p);
            }}
            className="w-full px-3.5 py-1.5 text-left hover:bg-white/15 flex items-center gap-2"
          >
            <FolderOpen className="w-3.5 h-3.5 text-white/80" />
            Show in Explorer
          </button>

          <button
            onClick={() => handleDeleteWallpaper(contextMenu.wp)}
            className="w-full px-3.5 py-1.5 text-left text-white/85 hover:text-white hover:bg-white/15 flex items-center gap-2"
          >
            <Trash2 className="w-3.5 h-3.5" />
            Remove from Carousel
          </button>
        </div>
      )}

      {/* Bottom Pitch-Black Pill (Fades out during portal apply) */}
      <div
        className={`absolute bottom-3 left-1/2 -translate-x-1/2 z-30 flex items-center gap-2.5 transition-all duration-200 ${
          applyPhase !== "idle"
            ? "opacity-0 translate-y-3 pointer-events-none"
            : "opacity-100"
        }`}
      >
        {/* Main Segmented Capsule */}
        <div className="h-10 px-4 rounded-full bg-black flex items-center gap-3 text-[11px] font-medium tracking-wider text-white/85">
          <span>
            <strong className="text-white font-semibold">Enter</strong>{" "}
            {selectedItem?.kind === "add" ? "Add Wallpapers" : "Apply"}
          </span>

          <div className="w-[1px] h-3 bg-white/[0.15]" />

          <button
            onClick={handleCycleFitMode}
            className="hover:text-white transition"
          >
            <strong className="text-white font-semibold">M</strong>{" "}
            <span className="uppercase">{currentFitMode}</span>
          </button>

          <div className="w-[1px] h-3 bg-white/[0.15]" />

          <button
            onClick={() => setExpandedCenter((p) => !p)}
            className="hover:text-white transition flex items-center gap-1"
          >
            <Maximize2 className="w-3 h-3 stroke-[2]" />
            <strong className="text-white font-semibold">Space</strong>{" "}
            {expandedCenter ? "Collapse" : "16:9"}
          </button>
        </div>

        {/* Separate Delete Capsule */}
        {selectedWallpaper && (
          <button
            onClick={() => handleDeleteWallpaper(selectedWallpaper)}
            title="Remove selected wallpaper (Delete)"
            className="h-10 px-4 rounded-full bg-black hover:bg-[#0c0c0e] flex items-center gap-1.5 text-[11px] font-medium tracking-wider text-white/85 hover:text-white transition-colors"
          >
            <Trash2 className="w-3 h-3 stroke-[2]" />
            <span>Del</span>
          </button>
        )}
      </div>
    </div>
  );
};
