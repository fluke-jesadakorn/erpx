import type {
  AttachmentLinkState,
  AttachmentSourceState,
} from "@/lib/attachment-client";

export type AttachmentDisplayState = {
  localState?: "reserving" | "uploading" | "waiting";
  sourceState: AttachmentSourceState | null;
  linkState: AttachmentLinkState;
};

export function attachmentStateLabel(link: AttachmentDisplayState) {
  // A closed logical link always wins over the physical source lifecycle. A
  // clean source can still exist after its draft link was withdrawn or expired.
  if (link.linkState === "withdrawn") return "Withdrawn";
  if (link.linkState === "expired") return "Upload expired";
  if (link.localState === "reserving") return "Reserving secure space";
  if (link.localState === "uploading") return "Uploading to quarantine";
  if (link.localState === "waiting" || link.sourceState === "quarantined")
    return "Waiting for security scan";
  if (link.sourceState === "scanning") return "Security scan in progress";
  if (link.sourceState === "available")
    return link.linkState === "bound"
      ? "Attached and locked"
      : "Ready to attach";
  if (link.sourceState === "legacy_unscanned")
    return "Legacy file — awaiting scan";
  if (link.sourceState === "scan_error")
    return "Scan unavailable — retry later";
  if (link.sourceState === "malware_rejected")
    return "Rejected by security scan";
  if (link.sourceState === "destroyed") return "File no longer retained";
  return link.sourceState ?? "Awaiting upload";
}

export function canReorderAttachmentPublications(input: {
  canCreate: boolean;
  canReview: boolean;
  sourceComponentKind: string;
  itemState: string;
  activeCount: number;
}) {
  return (
    (input.canCreate || input.canReview) &&
    input.sourceComponentKind !== "publication" &&
    input.itemState !== "revoked" &&
    input.activeCount > 1
  );
}

export function canMutateAttachmentDraft(input: {
  disabled: boolean;
  readOnly: boolean;
  frozen: boolean;
  locked: boolean;
}) {
  return !input.disabled && !input.readOnly && !input.frozen && !input.locked;
}

export function canRevokeAttachmentPublication(input: {
  canReview: boolean;
  itemState: string;
}) {
  return input.canReview && input.itemState === "approved";
}
