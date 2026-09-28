import "server-only";

import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "../authorization";
import { assertion } from "../commands";
import { ApiError } from "../http";
import { readablePost } from "../social-source";
import type { SqlDatabase, SqlStatement } from "../runtime/types";
import type {
  AttachmentOperation,
  AttachmentPolicy,
  AttachmentTargetScope,
} from "./types";
import { ATTACHMENT_MAX_BYTES } from "./feature";

const documents = [
  "image/jpeg",
  "image/png",
  "image/webp",
  "image/heic",
  "image/heif",
  "application/pdf",
] as const;

export const portalCollaborationAttachmentPolicies: readonly AttachmentPolicy[] = Object.freeze([
  {
    targetType: "customer_access_binding",
    purposeKey: "customer_support_evidence",
    version: 1,
    componentKinds: ["action"],
    mediaTypes: documents,
    maxFiles: 10,
    maxBytes: ATTACHMENT_MAX_BYTES,
    requiredCount: 0,
    classification: "confidential",
    retentionPolicyKey: null,
    publication: false,
    portalUpload: true,
  },
  {
    targetType: "support_incident",
    purposeKey: "incident_evidence",
    version: 1,
    componentKinds: ["action"],
    mediaTypes: documents,
    maxFiles: 10,
    maxBytes: ATTACHMENT_MAX_BYTES,
    requiredCount: 0,
    classification: "restricted",
    retentionPolicyKey: null,
    publication: false,
    portalUpload: false,
  },
  {
    targetType: "social_post",
    purposeKey: "thread_file",
    version: 1,
    componentKinds: ["record"],
    mediaTypes: documents,
    maxFiles: 10,
    maxBytes: ATTACHMENT_MAX_BYTES,
    requiredCount: 0,
    classification: "internal",
    retentionPolicyKey: null,
    publication: false,
    portalUpload: false,
  },
  {
    targetType: "social_post",
    purposeKey: "handover_file",
    version: 1,
    componentKinds: ["record"],
    mediaTypes: documents,
    maxFiles: 10,
    maxBytes: ATTACHMENT_MAX_BYTES,
    requiredCount: 0,
    classification: "confidential",
    retentionPolicyKey: null,
    publication: false,
    portalUpload: false,
  },
]);

type PortalBindingRow = {
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  version: number;
};

async function portalBindingScope(
  db: SqlDatabase,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
): Promise<AttachmentTargetScope> {
  const external = actor.role === "external_customer" &&
    actor.assignmentId === `customer-portal:${targetId}`;
  if (!external && operation !== "read")
    throw new ApiError(403, "PORTAL_ATTACHMENT_FORBIDDEN", "Only the verified customer may add evidence to this request.");
  if (!external && !hasPermission(actor, "commerce.read") && !hasPermission(actor, "support.read"))
    throw new ApiError(403, "PORTAL_ATTACHMENT_FORBIDDEN", "This customer evidence is unavailable to the current role.");

  const now = Date.now();
  const row = await db.prepare(`SELECT b.company_id,o.branch_id,o.department_id,b.version
    FROM customer_access_bindings b
    JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id
    ${external ? "JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=? AND i.email=?" : ""}
    WHERE b.id=? AND b.company_id=? AND b.status='approved' AND b.expires_at>?
      AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')`)
    .bind(...(external
      ? [actor.userId, actor.email.toLowerCase(), targetId, actor.companyId, now]
      : [targetId, actor.companyId, now]))
    .first<PortalBindingRow>();
  if (!row)
    throw new ApiError(404, "PORTAL_ATTACHMENT_TARGET_NOT_FOUND", "Customer evidence is unavailable.");
  return {
    companyId: row.company_id,
    branchId: row.branch_id,
    departmentId: row.department_id,
    version: Number(row.version),
    editable: external && operation !== "read",
  };
}

async function incidentScope(
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
): Promise<AttachmentTargetScope> {
  const row = await (await import("../support-incidents")).getSupportIncident(actor, targetId) as Record<string, unknown>;
  const manager = hasPermission(actor, "support.manage") && Number(row.reviewer_eligible) > 0;
  const assigned = hasPermission(actor, "support.work") && row.owner_id === actor.userId && Number(row.owner_eligible) > 0;
  if (!manager && !assigned)
    throw new ApiError(403, "INCIDENT_ATTACHMENT_FORBIDDEN", "Incident files are limited to the current owner and eligible Support managers.");
  if (operation !== "read" && !["open", "investigating"].includes(String(row.status)))
    throw new ApiError(409, "INCIDENT_ATTACHMENT_LOCKED", "Incident evidence is frozen after mitigation is proposed.");
  return {
    companyId: String(row.company_id),
    branchId: row.branch_id ? String(row.branch_id) : null,
    departmentId: row.department_id ? String(row.department_id) : null,
    version: Number(row.version),
    editable: operation !== "read",
  };
}

async function socialPostScope(
  db: SqlDatabase,
  actor: WorkContext,
  policy: AttachmentPolicy,
  targetId: string,
  operation: AttachmentOperation,
): Promise<AttachmentTargetScope> {
  const row = await readablePost(db, actor, targetId);
  const expectedPurpose = row.purpose === "handover" ? "handover_file" : "thread_file";
  if (policy.purposeKey !== expectedPurpose)
    throw new ApiError(422, "SOCIAL_ATTACHMENT_PURPOSE", "Choose the file purpose that matches this conversation.");
  if (operation !== "read" && row.author_id !== actor.userId)
    throw new ApiError(403, "SOCIAL_ATTACHMENT_AUTHOR_REQUIRED", "Only the conversation author may add a file revision.");
  return {
    companyId: row.company_id,
    branchId: row.branch_id,
    departmentId: row.department_id,
    version: Number(row.version),
    editable: operation !== "read",
  };
}

export async function authorizePortalCollaborationAttachmentTarget(
  db: SqlDatabase,
  actor: WorkContext,
  policy: AttachmentPolicy,
  targetId: string,
  operation: AttachmentOperation,
): Promise<AttachmentTargetScope | null> {
  if (policy.targetType === "customer_access_binding")
    return portalBindingScope(db, actor, targetId, operation);
  if (policy.targetType === "support_incident")
    return incidentScope(actor, targetId, operation);
  if (policy.targetType === "social_post")
    return socialPostScope(db, actor, policy, targetId, operation);
  return null;
}

export function portalCollaborationAttachmentTargetVersionGuard(
  db: SqlDatabase,
  commandId: string,
  targetType: string,
  targetId: string,
  companyId: string,
  version: number,
  index: number,
): SqlStatement | null {
  if (targetType === "customer_access_binding")
    return assertion(db, commandId, `attachment-owner-${index}`,
      "EXISTS(SELECT 1 FROM customer_access_bindings WHERE id=? AND company_id=? AND version=? AND status='approved' AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)",
      [targetId, companyId, version]);
  const table = targetType === "support_incident"
    ? "support_incidents"
    : targetType === "social_post"
      ? "social_posts"
      : null;
  if (!table) return null;
  return assertion(db, commandId, `attachment-owner-${index}`,
    `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?${targetType === "social_post" ? " AND state='published'" : ""})`,
    [targetId, companyId, version]);
}

export function portalCollaborationAttachmentRecordVersionStep(
  db: SqlDatabase,
  targetType: string,
  targetId: string,
  expectedVersion: number,
  now = Date.now(),
): SqlStatement | null {
  if (targetType !== "social_post") return null;
  return db.prepare("UPDATE social_posts SET version=version+1,updated_at=? WHERE id=? AND version=? AND state='published'")
    .bind(now, targetId, expectedVersion);
}
