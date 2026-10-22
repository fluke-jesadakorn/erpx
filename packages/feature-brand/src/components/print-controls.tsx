"use client";
import { Button, Field, Select } from "@erpx/ui/components/foundation";

import { useState } from "react";
import type { DocumentBrandMode } from "@erpx/server-brand/public/branding";
import styles from "@erpx/feature-brand/components/business-document.module.css";

export function PrintControls({
  defaultMode,
  hasBranch,
  printing,
}: {
  defaultMode: DocumentBrandMode;
  hasBranch: boolean;
  printing: boolean;
}) {
  const [mode, setMode] = useState<DocumentBrandMode>(hasBranch ? defaultMode : "holding");
  function openPrintView() {
    const url = new URL(window.location.href);
    url.searchParams.set("print", "1");
    url.searchParams.set("brand", mode);
    window.open(url, "_blank", "noopener,noreferrer");
  }
  return (
    <div className={styles.controls} data-print-hide>
      {!printing && hasBranch && (
        <Field appearance="standard">
          Document branding
          <Select appearance="standard"
            value={mode}
            onChange={(event) => setMode(event.target.value as DocumentBrandMode)}
          >
            <option value="branch">Branch only</option>
            <option value="holding">Holding only</option>
            <option value="both">Holding + Branch</option>
          </Select>
        </Field>
      )}
      <Button appearance="standard"
        variant="secondary"
        type="button"
        onClick={printing ? () => window.print() : openPrintView}
      >
        {printing ? "Print or save PDF" : "Open print view"}
      </Button>
    </div>
  );
}
