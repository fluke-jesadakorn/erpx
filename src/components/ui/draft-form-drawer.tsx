"use client";
import { useState, type ReactNode } from "react";
import { Dialog } from "./dialog";
import { Button } from "./button";
import type { UiAppearance } from "./appearance";

/** The editor owns its values; dismissal never unmounts an unconfirmed draft. */
export function DraftFormDrawer({ open, title, children, dirty, onClose, onDiscard = onClose, dismissDisabled, appearance = "standard", confirmationTitle = "Discard changes?", confirmationDescription = "Your draft has unsaved changes.", discardLabel = "Discard draft" }: {
  open: boolean; title: string; children: ReactNode; dirty: boolean;
  onClose: () => void; onDiscard?: () => void; dismissDisabled?: boolean; appearance?: UiAppearance;
  confirmationTitle?: string; confirmationDescription?: string; discardLabel?: string;
}) {
  const [confirming, setConfirming] = useState(false);
  return <Dialog open={open} title={title} presentation="form-drawer" appearance={appearance} dismissDisabled={dismissDisabled} onClose={() => dirty ? setConfirming(true) : onClose()}>
    {children}
    <Dialog open={confirming} title={confirmationTitle} appearance={appearance} onClose={() => setConfirming(false)}>
      <p>{confirmationDescription}</p>
      <div className="ui-form-actions">
        <Button appearance={appearance} variant="secondary" onClick={() => setConfirming(false)}>Keep editing</Button>
        <Button appearance={appearance} variant="danger" onClick={() => { setConfirming(false); onDiscard(); }}>{discardLabel}</Button>
      </div>
    </Dialog>
  </Dialog>;
}
