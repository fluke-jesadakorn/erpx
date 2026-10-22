"use client";
import { useState, type ReactNode } from "react";
import { Dialog } from "@erpx/ui/components/dialog";
import { Button } from "@erpx/ui/components/button";
import type { UiAppearance } from "@erpx/ui/components/appearance";

/** The editor owns its values; dismissal never unmounts an unconfirmed draft. */
export function DraftFormDrawer({ open, title, children, dirty, onClose, finalFocus, onDiscard = onClose, dismissDisabled, appearance = "standard", confirmationTitle = "Discard changes?", confirmationDescription = "Your draft has unsaved changes.", discardLabel = "Discard draft", keepEditingLabel = "Keep editing", retainFormContents = false }: {
  retainFormContents?: boolean;
  finalFocus?: () => HTMLElement | null;
  open: boolean; title: string; children: ReactNode; dirty: boolean;
  onClose: () => void; onDiscard?: () => void; dismissDisabled?: boolean; appearance?: UiAppearance;
  confirmationTitle?: string; confirmationDescription?: string; discardLabel?: string; keepEditingLabel?: string;
}) {
  const [confirming, setConfirming] = useState(false);
  return <Dialog retainFormContents={retainFormContents} finalFocus={finalFocus} open={open} title={title} presentation="form-drawer" appearance={appearance} dismissDisabled={dismissDisabled} onClose={() => dirty ? setConfirming(true) : onClose()}>
    {children}
    <Dialog open={confirming} title={confirmationTitle} appearance={appearance} onClose={() => setConfirming(false)}>
      <p>{confirmationDescription}</p>
      <div className="ui-form-actions">
        <Button appearance={appearance} variant="secondary" onClick={() => setConfirming(false)}>{keepEditingLabel}</Button>
        <Button appearance={appearance} variant="danger" onClick={() => { setConfirming(false); onDiscard(); }}>{discardLabel}</Button>
      </div>
    </Dialog>
  </Dialog>;
}
