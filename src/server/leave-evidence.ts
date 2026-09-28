import "server-only";

import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "./authorization";
import { approvalView } from "./approvals";
import { ApiError } from "./http";
import { attachmentsV2Enabled } from "./attachments/feature";
import type { SqlDatabase } from "./runtime/types";
import type { AttachmentManifestItem } from "./attachments/types";
import { z } from "zod";

export const leaveEvidencePurposes = [
  "medical_certificate",
  "leave_supporting_photo",
] as const;
export type LeaveEvidencePurpose = (typeof leaveEvidencePurposes)[number];

export type LeaveEvidencePolicy = {
  required: boolean;
  minimumUnits: number | null;
  purposeKey: LeaveEvidencePurpose | null;
  minimumCount: number;
  dueStage: "before_submit" | "before_final_approval";
  graceDays: number;
  privacy: "requester_approvers_hr";
  note: string;
};

const leaveEvidencePolicySchema = z.object({
  required: z.boolean(),
  minimumUnits: z.number().nullable(),
  purposeKey: z.enum(leaveEvidencePurposes).nullable(),
  minimumCount: z.number().int().min(0).max(100),
  dueStage: z.enum(["before_submit", "before_final_approval"]),
  graceDays: z.number().int().min(0).max(365),
  privacy: z.literal("requester_approvers_hr"),
  note: z.string(),
});

export function parseLeaveEvidencePolicy(value: unknown): LeaveEvidencePolicy | null {
  if (typeof value === "string") {
    try {
      value = JSON.parse(value);
    } catch {
      return null;
    }
  }
  const parsed = leaveEvidencePolicySchema.safeParse(value);
  return parsed.success ? parsed.data : null;
}

type LeaveAttachmentTarget = {
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  created_by: string;
  state: string;
  version: number;
};

async function leaveAttachmentTarget(
  db: SqlDatabase,
  actor: WorkContext,
  leaveRequestId: string,
) {
  const row = await db
    .prepare(
      `SELECT id,company_id,branch_id,department_id,created_by,state,version
       FROM leave_requests WHERE id=? AND company_id=?`,
    )
    .bind(leaveRequestId, actor.companyId)
    .first<LeaveAttachmentTarget>();
  if (!row) throw new ApiError(404, "NOT_FOUND", "Leave request not found.");
  return row;
}

async function recordedLeaveDecision(
  db: SqlDatabase,
  actor: WorkContext,
  leaveRequestId: string,
) {
  return Boolean(
    await db
      .prepare(
        `SELECT 1
         FROM approval_instances instance
         JOIN approval_decisions decision ON decision.instance_id=instance.id
         WHERE instance.company_id=? AND instance.record_type='leave'
           AND instance.record_id=?
           AND decision.action IN ('approve','reject','override')
           AND iam_same_person(decision.actor_user_id,?,instance.company_id)
         LIMIT 1`,
      )
      .bind(actor.companyId, leaveRequestId, actor.userId)
      .first(),
  );
}

/**
 * Purpose-aware boundary used by the central attachment registry. It is kept
 * separate from general leave visibility because future/replaced approvers may
 * still see workflow metadata while restricted evidence must disappear at once.
 */
export async function assertLeaveAttachmentAccess(
  db: SqlDatabase,
  actor: WorkContext,
  input: {
    leaveRequestId: string;
    purposeKey: LeaveEvidencePurpose;
    operation: "read" | "write" | "bind";
  },
): Promise<{
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  version: number;
  editable: boolean;
}> {
  if (!leaveEvidencePurposes.includes(input.purposeKey))
    throw new ApiError(422, "LEAVE_EVIDENCE_PURPOSE", "Choose a supported leave evidence purpose.");

  const row = await leaveAttachmentTarget(db, actor, input.leaveRequestId);
  const requester = row.created_by === actor.userId;
  const editable = requester && ["draft", "rejected", "pending_approval"].includes(row.state);

  if (input.operation === "write") {
    if (!editable)
      throw new ApiError(
        403,
        "LEAVE_EVIDENCE_REQUESTER_REQUIRED",
        "Only the requester can add or withdraw leave evidence before final approval.",
      );
  } else {
    const privileged = hasPermission(actor, "leave.evidence.read");
    const priorDecision = await recordedLeaveDecision(db, actor, row.id);
    const currentView = row.state === "pending_approval"
      ? await approvalView(db, "leave", row.id, actor)
      : null;
    const currentApprover = Boolean(currentView?.canDecide || currentView?.canOverride);
    const allowed = requester || privileged || priorDecision || currentApprover;
    if (!allowed)
      throw new ApiError(
        403,
        "LEAVE_EVIDENCE_FORBIDDEN",
        "Restricted leave evidence is available only to the requester, the current reviewer, recorded decision makers, and authorized HR.",
      );
    if (input.operation === "bind" && !currentApprover)
      throw new ApiError(
        403,
        "CURRENT_APPROVER_REQUIRED",
        "Only the current eligible approver can bind the reviewed leave evidence.",
      );
  }

  return {
    companyId: row.company_id,
    branchId: row.branch_id || null,
    departmentId: row.department_id || null,
    version: row.version,
    editable,
  };
}

export async function availableLeaveEvidence(
  db: SqlDatabase,
  leaveRequestId: string,
  policy: Pick<LeaveEvidencePolicy, "purposeKey" | "minimumCount">,
) {
  if (!policy.purposeKey)
    return { count: 0, setIds: [] as string[] };

  const rows = (
    await db
      .prepare(
	        `SELECT DISTINCT set_row.id,source.id source_id
	         FROM attachment_sets set_row
	         JOIN leave_requests request ON request.id=set_row.target_id
	           AND request.company_id=set_row.company_id
	         JOIN attachment_links link ON link.set_id=set_row.id
	           AND link.state IN ('draft','bound')
	         JOIN attachments source ON source.id=link.attachment_id
	           AND source.company_id=set_row.company_id
	           AND source.source_state='available'
         WHERE set_row.target_type='leave_request' AND set_row.target_id=?
           AND set_row.purpose_key=? AND set_row.state IN ('draft','bound')
         ORDER BY set_row.id,source.id`,
      )
      .bind(leaveRequestId, policy.purposeKey)
      .all<{ id: string; source_id: string }>()
  ).results;
  const sourceIds = new Set(rows.map((row) => row.source_id));
  if (!attachmentsV2Enabled()) {
    const legacy = (
      await db
        .prepare(
	          `SELECT source.id
	           FROM attachments source
	           JOIN leave_requests request ON request.id=source.owner_id
	             AND request.company_id=source.company_id
	           WHERE source.owner_type='leave_request' AND source.owner_id=?
             AND source.revoked_at IS NULL AND source.purpose=?
             AND source.source_state='available'
             AND NOT EXISTS(
               SELECT 1 FROM attachment_links link WHERE link.attachment_id=source.id
             )`,
        )
        .bind(leaveRequestId, policy.purposeKey)
        .all<{ id: string }>()
    ).results;
    for (const source of legacy) sourceIds.add(source.id);
  }
  return {
    // Canonical reuse must not let one physical source satisfy a multi-file
    // policy more than once, even if it has several links or revisions.
    count: sourceIds.size,
    setIds: [...new Set(rows.map((row) => row.id))],
  };
}

export async function leaveEvidenceManifest(
  db: SqlDatabase,
  leaveRequestId: string,
): Promise<{
  manifest: AttachmentManifestItem[];
  canonicalManifest: AttachmentManifestItem[];
  legacyManifest: AttachmentManifestItem[];
  legacyIncluded: boolean;
  draftSetIds: string[];
}> {
  const rows = (
    await db
      .prepare(
        `SELECT set_row.id set_id,set_row.state set_state,set_row.purpose_key,link.id link_id,
           source.id source_id,source.checksum_sha256,source.detected_media_type,
           source.media_type,source.bytes
	         FROM attachment_sets set_row
	         JOIN leave_requests request ON request.id=set_row.target_id
	           AND request.company_id=set_row.company_id
	         JOIN attachment_links link ON link.set_id=set_row.id
	         JOIN attachments source ON source.id=link.attachment_id
	           AND source.company_id=set_row.company_id
         WHERE set_row.target_type='leave_request' AND set_row.target_id=?
           AND set_row.state IN ('draft','bound')
           AND link.state IN ('draft','bound')
           AND source.source_state='available'
         ORDER BY set_row.created_at,link.created_at,link.id`,
      )
      .bind(leaveRequestId)
      .all<{
        set_id: string;
        set_state: "draft" | "bound";
        purpose_key: string;
        link_id: string;
        source_id: string;
        checksum_sha256: string;
        detected_media_type: string | null;
        media_type: string;
        bytes: number;
      }>()
  ).results;
  const canonicalManifest = rows.map((row) => ({
    linkId: row.link_id,
    sourceId: row.source_id,
    purposeKey: row.purpose_key,
    checksumSha256: row.checksum_sha256,
    mediaType: row.detected_media_type ?? row.media_type,
    bytes: Number(row.bytes),
  }));
  const legacyIncluded = !attachmentsV2Enabled();
  const legacyManifest: AttachmentManifestItem[] = [];
  if (legacyIncluded) {
    const legacy = (
      await db
        .prepare(
          `SELECT source.id,source.checksum_sha256,source.detected_media_type,
             source.media_type,source.bytes,source.purpose
	           FROM attachments source
	           JOIN leave_requests request ON request.id=source.owner_id
	             AND request.company_id=source.company_id
           WHERE source.owner_type='leave_request' AND source.owner_id=?
             AND source.revoked_at IS NULL AND source.source_state='available'
             AND NOT EXISTS(
               SELECT 1 FROM attachment_links link WHERE link.attachment_id=source.id
             )
           ORDER BY source.created_at,source.id`,
        )
        .bind(leaveRequestId)
        .all<{
          id: string;
          checksum_sha256: string;
          detected_media_type: string | null;
          media_type: string;
          bytes: number;
          purpose: string;
        }>()
    ).results;
    legacyManifest.push(...legacy.map((source) => ({
      linkId: source.id,
      sourceId: source.id,
      purposeKey: leaveEvidencePurposes.includes(source.purpose as LeaveEvidencePurpose)
        ? source.purpose
        : "leave_supporting_photo",
      checksumSha256: source.checksum_sha256,
      mediaType: source.detected_media_type ?? source.media_type,
      bytes: Number(source.bytes),
    })));
  }
  return {
    manifest: [...canonicalManifest, ...legacyManifest],
    canonicalManifest,
    legacyManifest,
    legacyIncluded,
    draftSetIds: [
      ...new Set(rows.filter((row) => row.set_state === "draft").map((row) => row.set_id)),
    ],
  };
}

export async function pendingLeaveEvidenceCount(
  db: SqlDatabase,
  leaveRequestId: string,
) {
  const row = await db
    .prepare(
      `SELECT count(*)::int count
	       FROM attachment_sets set_row
	       JOIN leave_requests request ON request.id=set_row.target_id
	         AND request.company_id=set_row.company_id
	       JOIN attachment_links link ON link.set_id=set_row.id
	       LEFT JOIN attachments source ON source.id=link.attachment_id
	         AND source.company_id=set_row.company_id
         LEFT JOIN upload_intents intent ON intent.id=link.upload_intent_id
           AND intent.company_id=set_row.company_id
       WHERE set_row.target_type='leave_request' AND set_row.target_id=?
         AND set_row.state='draft' AND link.state='draft'
         AND (
           (link.attachment_id IS NULL AND intent.id IS NOT NULL)
           OR (source.id IS NOT NULL AND source.source_state<>'available')
         )`,
    )
    .bind(leaveRequestId)
    .first<{ count: number }>();
  return Number(row?.count ?? 0);
}
