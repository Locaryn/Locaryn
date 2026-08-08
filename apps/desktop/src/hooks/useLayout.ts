import { useCallback, useEffect, useRef, useState } from "react";

/**
 * Resizable panel layout — left/right widths and bottom height, persisted
 * to localStorage. Values are applied as CSS variables on the app root so
 * the grid template reacts without re-rendering children.
 */

export interface LayoutState {
  leftW: number;
  rightW: number;
  bottomH: number;
}

const DEFAULT_LAYOUT: LayoutState = { leftW: 248, rightW: 320, bottomH: 190 };
const LIMITS = {
  leftW: [184, 400] as const,
  rightW: [240, 560] as const,
  bottomH: [120, 440] as const,
};
const STORAGE_KEY = "lochor:layout";

function clamp(v: number, [min, max]: readonly [number, number]) {
  return Math.min(max, Math.max(min, v));
}

function load(): LayoutState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_LAYOUT;
    const parsed = { ...DEFAULT_LAYOUT, ...(JSON.parse(raw) as Partial<LayoutState>) };
    return {
      leftW: clamp(parsed.leftW, LIMITS.leftW),
      rightW: clamp(parsed.rightW, LIMITS.rightW),
      bottomH: clamp(parsed.bottomH, LIMITS.bottomH),
    };
  } catch {
    return DEFAULT_LAYOUT;
  }
}

export function useLayout() {
  const [layout, setLayout] = useState<LayoutState>(load);
  const dragRef = useRef<{
    key: keyof LayoutState;
    startPos: number;
    startVal: number;
    horizontal: boolean;
    invert: boolean;
  } | null>(null);

  useEffect(() => {
    const root = document.documentElement;
    root.style.setProperty("--layout-left", `${layout.leftW}px`);
    root.style.setProperty("--layout-right", `${layout.rightW}px`);
    root.style.setProperty("--layout-bottom", `${layout.bottomH}px`);
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(layout));
    } catch {
      // Ignore quota errors.
    }
  }, [layout]);

  useEffect(() => {
    function onMove(e: PointerEvent) {
      const drag = dragRef.current;
      if (!drag) return;
      const pos = drag.horizontal ? e.clientX : e.clientY;
      const delta = (pos - drag.startPos) * (drag.invert ? -1 : 1);
      setLayout((l) => ({
        ...l,
        [drag.key]: clamp(drag.startVal + delta, LIMITS[drag.key]),
      }));
    }
    function onUp() {
      if (dragRef.current) {
        dragRef.current = null;
        document.body.classList.remove("lochor-resizing");
      }
    }
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
  }, []);

  const startDrag = useCallback(
    (key: keyof LayoutState) => (e: React.PointerEvent) => {
      e.preventDefault();
      const horizontal = key !== "bottomH";
      dragRef.current = {
        key,
        startPos: horizontal ? e.clientX : e.clientY,
        startVal: layout[key],
        horizontal,
        // Right panel and bottom panel grow when dragging toward the center.
        invert: key === "rightW" || key === "bottomH",
      };
      document.body.classList.add("lochor-resizing");
    },
    [layout],
  );

  return { layout, startDrag };
}

export type UseLayoutReturn = ReturnType<typeof useLayout>;
