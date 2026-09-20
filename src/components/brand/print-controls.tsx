"use client";

import { useState } from "react";
import type { DocumentBrandMode } from "@/server/branding";
import styles from "./business-document.module.css";

export function PrintControls({ defaultMode, hasBranch, printing }: { defaultMode: DocumentBrandMode; hasBranch: boolean; printing: boolean }) {
  const [mode, setMode] = useState<DocumentBrandMode>(hasBranch ? defaultMode : "holding");
  function openPrintView() {
    const url = new URL(window.location.href);
    url.searchParams.set("print", "1");
    url.searchParams.set("brand", mode);
    window.open(url, "_blank", "noopener,noreferrer");
  }
  return <div className={styles.controls} data-print-hide>
    {!printing && hasBranch && <label>Document branding<select value={mode} onChange={(event) => setMode(event.target.value as DocumentBrandMode)}><option value="branch">Branch only</option><option value="holding">Holding only</option><option value="both">Holding + Branch</option></select></label>}
    <button type="button" className="button secondary" onClick={printing ? () => window.print() : openPrintView}>{printing ? "Print or save PDF" : "Open print view"}</button>
  </div>;
}
