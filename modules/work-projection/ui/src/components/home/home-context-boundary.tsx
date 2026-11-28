"use client";
import { useEffect, useState, type ReactNode } from "react";

/** A committed context change drops old requests/editors before the new RSC payload arrives. */
export function HomeContextBoundary({ contextKey, children }: { contextKey: string; children: ReactNode }) {
  const [invalidated, setInvalidated] = useState(false);
  useEffect(() => {
    const invalidate = () => setInvalidated(true);
    window.addEventListener("erpx:work-context-changed", invalidate);
    return () => window.removeEventListener("erpx:work-context-changed", invalidate);
  }, [contextKey]);
  return invalidated ? <p role="status">Refreshing your workspace…</p> : children;
}
