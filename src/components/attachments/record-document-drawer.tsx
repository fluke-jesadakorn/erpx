"use client";

import { Paperclip } from "lucide-react";
import { useState } from "react";
import { Dialog } from "@/components/ui/dialog";
import {
  RecordDocumentPanel,
  type RecordDocumentPurpose,
} from "./record-document-panel";
import { useAttachmentFeatureMode } from "./attachment-feature-context";

type RecordDocumentDrawerProps = {
  base: string;
  targetType: string;
  targetId: string;
  expectedTargetVersion: number;
  title: string;
  purposes: readonly RecordDocumentPurpose[];
  editable?: boolean;
  triggerLabel?: string;
};

/**
 * Reusable file surface for list and console records that do not warrant a
 * dedicated detail route. The actual attachment workflow remains the shared
 * RecordDocumentPanel; this component only gives it a responsive side drawer.
 */
export function RecordDocumentDrawer(props: RecordDocumentDrawerProps) {
  const mode = useAttachmentFeatureMode();
  if (mode === "disabled") return null;
  return (
    <RecordDocumentDrawerContent
      {...props}
      editable={props.editable !== false && mode !== "read-only"}
    />
  );
}

function RecordDocumentDrawerContent({
  base,
  targetType,
  targetId,
  expectedTargetVersion,
  title,
  purposes,
  editable = true,
  triggerLabel = "Files",
}: RecordDocumentDrawerProps) {
  const [open, setOpen] = useState(false);
  return <>
    <button
      type="button"
      className="button secondary"
      aria-haspopup="dialog"
      aria-expanded={open}
      onClick={() => setOpen(true)}
    >
      <Paperclip aria-hidden size={15} />
      {triggerLabel}
    </button>
    <Dialog
      open={open}
      onClose={() => setOpen(false)}
      title={title}
      presentation="people-drawer"
    >
      <RecordDocumentPanel
        base={base}
        targetType={targetType}
        targetId={targetId}
        expectedTargetVersion={expectedTargetVersion}
        title={title}
        purposes={purposes}
        editable={editable}
      />
    </Dialog>
  </>;
}
