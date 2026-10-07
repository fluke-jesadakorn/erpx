
export const attachmentComponentKinds = [
  "record",
  "action",
  "line",
  "event",
  "message",
  "publication",
] as const;
export type AttachmentComponentKind = (typeof attachmentComponentKinds)[number];

export const attachmentSourceStates = [
  "legacy_unscanned",
  "quarantined",
  "scanning",
  "available",
  "scan_error",
  "malware_rejected",
  "destroyed",
] as const;
export type AttachmentSourceState = (typeof attachmentSourceStates)[number];

export const attachmentLinkStates = ["draft", "bound", "withdrawn", "expired"] as const;
export type AttachmentLinkState = (typeof attachmentLinkStates)[number];

export type AttachmentClassification = "public" | "internal" | "confidential" | "restricted";

export type AttachmentManifestItem = {
  linkId: string;
  sourceId: string;
  purposeKey: string;
  checksumSha256: string;
  mediaType: string;
  bytes: number;
};

export type AttachmentLinkDto = {
  id: string;
  sourceId: string | null;
  filename: string;
  mediaType: string;
  bytes: number | null;
  sourceState: AttachmentSourceState | null;
  linkState: AttachmentLinkState;
  caption: string | null;
  previewAvailable: boolean;
  createdAt: number;
  expiresAt: number;
  boundAt: number | null;
};

export type AttachmentSetDto = {
  id: string;
  targetType: string;
  targetId: string;
  componentKind: AttachmentComponentKind;
  componentRef: string | null;
  purposeKey: string;
  expectedTargetVersion: number;
  policyVersion: number;
  classification: AttachmentClassification;
  requiredCount: number;
  maxFiles: number;
  state: "draft" | "bound" | "withdrawn" | "expired";
  createdAt: number;
  updatedAt: number;
  sanitizationAttested?: boolean;
  links: AttachmentLinkDto[];
};

export type PreparedAttachmentBinding<Effect> = {
  manifest: AttachmentManifestItem[];
  guards: Effect[];
  steps: Effect[];
};

export type AttachmentBindingContext = {
  purposeKey: string;
  componentKind: AttachmentComponentKind;
  componentRef: string | null;
};

/**
 * Server-owned, exact contexts accepted by one domain command. There are no
 * wildcard purposes or component references: action/event/message evidence
 * must name the exact context that the command is about to freeze.
 */
export type AttachmentBindingContract = {
  targetType: string;
  bindingType: string;
  contexts: readonly AttachmentBindingContext[];
};

export type AttachmentTargetScope = {
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  version: number;
  editable: boolean;
};

export type AttachmentOperation = "read" | "write" | "bind" | "publish";

export type AttachmentPolicy = {
  targetType: string;
  purposeKey: string;
  version: number;
  componentKinds: readonly AttachmentComponentKind[];
  mediaTypes: readonly string[];
  maxFiles: number;
  maxBytes: number;
  requiredCount: number;
  classification: AttachmentClassification;
  retentionPolicyKey: string | null;
  publication: boolean;
  portalUpload: boolean;
};
