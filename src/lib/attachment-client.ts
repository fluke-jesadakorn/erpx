import { workFetch } from "@/lib/work-fetch";

export type AttachmentComponentKind =
  | "record"
  | "action"
  | "line"
  | "event"
  | "message"
  | "publication";

export type AttachmentSourceState =
  | "legacy_unscanned"
  | "quarantined"
  | "scanning"
  | "available"
  | "scan_error"
  | "malware_rejected"
  | "destroyed";

export type AttachmentLinkState = "draft" | "bound" | "withdrawn" | "expired";

export type AttachmentLinkDto = {
  id: string;
  sourceId?: string | null;
  filename: string;
  mediaType: string;
  bytes: number | null;
  sourceState: AttachmentSourceState | null;
  linkState: AttachmentLinkState;
  caption?: string | null;
  previewAvailable: boolean;
  createdAt: number;
  boundAt?: number | null;
};

export type AttachmentSetDto = {
  id: string;
  targetType: string;
  targetId: string;
  componentKind: AttachmentComponentKind;
  componentRef?: string | null;
  purposeKey: string;
  expectedTargetVersion?: number;
  state: string;
  requiredCount: number;
  maxFiles: number;
  policyVersion: number;
  sanitizationAttested?: boolean;
  links: AttachmentLinkDto[];
};

export type AttachmentStatusDto = {
  linkId: string;
  setId: string;
  sourceId?: string | null;
  sourceState: AttachmentSourceState | null;
  linkState: AttachmentLinkState;
  scan: {
    attempts: number;
    lastResult?: string | null;
    lastError?: string | null;
    scannedAt?: number | null;
  };
  previewAvailable: boolean;
  bindable: boolean;
  expiresAt?: number | null;
  boundAt?: number | null;
  withdrawnAt?: number | null;
};

export type MediaPublicationDto = {
  id: string;
  source_link_id: string;
  target_type: string;
  target_id: string;
  derivative_media_type: string;
  derivative_checksum_sha256: string;
  derivative_bytes: number;
  alt_text: string;
  ordinal: number;
  revision: number;
  state: "draft" | "approved" | "revoked";
  created_by: string;
  created_at: number;
  approved_by?: string | null;
  approved_at?: number | null;
  revoked_by?: string | null;
  revoked_at?: number | null;
  revocation_reason?: string | null;
};

export type MediaPublicationListDto = {
  targetType: string;
  targetId: string;
  orderVersion: number;
  items: MediaPublicationDto[];
};

type ApiErrorEnvelope = { error?: { code?: string; message?: string } };

export class AttachmentApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly code?: string,
  ) {
    super(message);
    this.name = "AttachmentApiError";
  }
}

export const ATTACHMENT_MAX_BYTES = 10 * 1024 * 1024;

const GENERIC_MEDIA_TYPES = new Set([
  "image/jpeg",
  "image/png",
  "image/webp",
  "image/heic",
  "image/heif",
  "application/pdf",
]);

const EXTENSION_MEDIA_TYPES: Record<string, string> = {
  ".jpeg": "image/jpeg",
  ".jpg": "image/jpeg",
  ".png": "image/png",
  ".webp": "image/webp",
  ".heic": "image/heic",
  ".heif": "image/heif",
  ".pdf": "application/pdf",
};

export function attachmentApiBase(base: string) {
  if (base.startsWith("/api/v1/holdings/") || /^\/api\/customer-portal\/[^/]+\/attachments\/?$/.test(base))
    return base.replace(/\/$/, "");
  if (!base.startsWith("/holdings/"))
    throw new Error("AttachmentPanel requires a holding workspace base path.");
  return `/api/v1${base.replace(/\/$/, "")}`;
}

export function attachmentMediaType(file: Pick<File, "name" | "type">) {
  if (GENERIC_MEDIA_TYPES.has(file.type.toLocaleLowerCase()))
    return file.type.toLocaleLowerCase();
  const extension = file.name.toLocaleLowerCase().match(/\.[^.]+$/)?.[0] ?? "";
  return EXTENSION_MEDIA_TYPES[extension] ?? file.type.toLocaleLowerCase();
}

export function validateGenericAttachment(
  file: Pick<File, "name" | "type" | "size">,
  maxBytes = ATTACHMENT_MAX_BYTES,
) {
  if (file.size < 1) return "The selected file is empty.";
  if (file.size > maxBytes)
    return `Each file must be ${formatAttachmentBytes(maxBytes)} or smaller.`;
  if (!GENERIC_MEDIA_TYPES.has(attachmentMediaType(file)))
    return "Attach a JPEG, PNG, WebP, HEIC, HEIF, or PDF file.";
  return null;
}

export function formatAttachmentBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.ceil(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(bytes < 10 * 1024 * 1024 ? 1 : 0)} MB`;
}

async function envelope<T>(response: Response): Promise<T> {
  const body = (await response.json().catch(() => ({}))) as ApiErrorEnvelope & {
    data?: T;
  };
  if (!response.ok || body.data === undefined)
    throw new AttachmentApiError(
      body.error?.message ?? "The attachment service could not complete this request.",
      response.status,
      body.error?.code,
    );
  return body.data;
}

export async function listAttachmentSets(
  base: string,
  query: {
    targetType: string;
    targetId: string;
    purposeKey?: string;
    componentKind?: AttachmentComponentKind;
    expectedTargetVersion?: number;
  },
) {
  const search = new URLSearchParams(
    Object.entries(query).map(([key, value]) => [key, String(value)]),
  );
  const response = await workFetch(
    `${attachmentApiBase(base)}/attachment-sets?${search.toString()}`,
    { cache: "no-store" },
  );
  return envelope<AttachmentSetDto[]>(response);
}

export async function createAttachmentSet(
  base: string,
  input: {
    targetType: string;
    targetId: string;
    componentKind: AttachmentComponentKind;
    componentRef?: string;
    purposeKey: string;
    expectedTargetVersion: number;
  },
  idempotencyKey: string,
) {
  const response = await workFetch(`${attachmentApiBase(base)}/attachment-sets`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": idempotencyKey,
    },
    body: JSON.stringify(input),
  });
  return envelope<AttachmentSetDto>(response);
}

export async function reserveAttachmentFile(
  base: string,
  setId: string,
  input: { filename: string; mediaType: string; bytes: number; caption?: string },
  idempotencyKey: string,
) {
  const response = await workFetch(
    `${attachmentApiBase(base)}/attachment-sets/${encodeURIComponent(setId)}/files`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": idempotencyKey,
      },
      body: JSON.stringify(input),
    },
  );
  return envelope<{
    uploadIntentId: string;
    attachmentLinkId: string;
    uploadUrl: string;
    expiresAt: number;
  }>(response);
}

export async function attestAttachmentSetSanitized(
  base: string,
  setId: string,
  input: { confirmed: true; note: string },
  idempotencyKey: string,
) {
  const response = await workFetch(
    `${attachmentApiBase(base)}/attachment-sets/${encodeURIComponent(setId)}/sanitization-attestation`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": idempotencyKey,
      },
      body: JSON.stringify(input),
    },
  );
  return envelope<AttachmentSetDto>(response);
}

export async function uploadAttachmentBytes(
  uploadUrl: string,
  file: File,
  mediaType: string,
) {
  const response = await workFetch(uploadUrl, {
    method: "PUT",
    headers: { "Content-Type": mediaType },
    body: file,
  });
  return envelope<{
    uploadIntentId: string;
    attachmentLinkId: string;
    sourceId: string;
    state: AttachmentSourceState;
    status: "scan_pending";
  }>(response);
}

export async function bindRecordAttachmentSet(
  base: string,
  setId: string,
  expectedTargetVersion: number,
  idempotencyKey: string,
) {
  const response = await workFetch(
    `${attachmentApiBase(base)}/attachment-sets/${encodeURIComponent(setId)}/bind`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": idempotencyKey,
      },
      body: JSON.stringify({ expectedTargetVersion }),
    },
  );
  return envelope<{
    setId: string;
    version: number;
    manifest: Array<{
      linkId: string;
      sourceId: string;
      purposeKey: string;
      checksumSha256: string;
      mediaType: string;
      bytes: number;
    }>;
  }>(response);
}

export async function attachmentStatus(base: string, linkId: string) {
  const response = await workFetch(
    `${attachmentApiBase(base)}/attachment-links/${encodeURIComponent(linkId)}/status`,
    { cache: "no-store" },
  );
  return envelope<AttachmentStatusDto>(response);
}

export async function withdrawAttachmentLink(
  base: string,
  linkId: string,
  reason: string,
  idempotencyKey: string,
) {
  const response = await workFetch(
    `${attachmentApiBase(base)}/attachment-links/${encodeURIComponent(linkId)}/withdraw`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": idempotencyKey,
      },
      body: JSON.stringify({ reason }),
    },
  );
  return envelope<AttachmentStatusDto>(response);
}

export function attachmentContentUrl(
  base: string,
  linkId: string,
  variant: "original" | "preview",
) {
  return `${attachmentApiBase(base)}/attachment-links/${encodeURIComponent(linkId)}/content?variant=${variant}`;
}

export async function listMediaPublications(
  base: string,
  targetType: string,
  targetId: string,
) {
  const query = new URLSearchParams({ targetType, targetId });
  const response = await workFetch(
    `${attachmentApiBase(base)}/media-publications?${query.toString()}`,
    { cache: "no-store" },
  );
  return envelope<MediaPublicationListDto>(response);
}

export async function createMediaPublication(
  base: string,
  input: {
    sourceLinkId: string;
    targetType: string;
    targetId: string;
    altText: string;
    ordinal: number;
  },
  idempotencyKey: string,
) {
  const response = await workFetch(`${attachmentApiBase(base)}/media-publications`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": idempotencyKey,
    },
    body: JSON.stringify(input),
  });
  return envelope<MediaPublicationDto>(response);
}

export async function approveMediaPublication(
  base: string,
  publicationId: string,
  idempotencyKey: string,
) {
  const response = await workFetch(
    `${attachmentApiBase(base)}/media-publications/${encodeURIComponent(publicationId)}/approve`,
    {
      method: "POST",
      headers: { "Idempotency-Key": idempotencyKey },
    },
  );
  return envelope<MediaPublicationDto>(response);
}

export async function revokeMediaPublication(
  base: string,
  publicationId: string,
  reason: string,
  idempotencyKey: string,
) {
  const response = await workFetch(
    `${attachmentApiBase(base)}/media-publications/${encodeURIComponent(publicationId)}/revoke`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": idempotencyKey,
      },
      body: JSON.stringify({ reason }),
    },
  );
  return envelope<MediaPublicationDto>(response);
}

export async function reorderMediaPublications(
  base: string,
  input: {
    targetType: string;
    targetId: string;
    expectedVersion: number;
    items: Array<{ id: string; ordinal: number }>;
  },
  idempotencyKey: string,
) {
  const response = await workFetch(`${attachmentApiBase(base)}/media-publications/order`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": idempotencyKey,
    },
    body: JSON.stringify(input),
  });
  return envelope<{ targetType: string; targetId: string; orderVersion: number }>(response);
}

export function publicMediaPublicationUrl(base: string, publicationId: string) {
  const holdingSlug = base.match(/^\/holdings\/([^/]+)/)?.[1];
  if (!holdingSlug)
    throw new Error("Public media requires a holding workspace base path.");
  return `/api/public/holdings/${encodeURIComponent(decodeURIComponent(holdingSlug))}/media-publications/${encodeURIComponent(publicationId)}/content`;
}
