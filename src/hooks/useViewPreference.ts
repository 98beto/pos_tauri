import { useEffect, useState } from "react";

export type CollectionView = "list" | "cards";

export function useViewPreference(key: string, defaultView: CollectionView): [CollectionView, (view: CollectionView) => void] {
  const [view, setView] = useState<CollectionView>(() => {
    try {
      const stored = window.localStorage.getItem(key);
      return stored === "list" || stored === "cards" ? stored : defaultView;
    } catch {
      return defaultView;
    }
  });

  useEffect(() => {
    try {
      window.localStorage.setItem(key, view);
    } catch {
      // Local preference is optional; the in-memory choice still works.
    }
  }, [key, view]);

  return [view, setView];
}
