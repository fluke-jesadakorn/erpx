import type { SqlDatabase } from "./runtime/types";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { sha256 } from "./crypto";
import { ApiError } from "./http";
import { peopleRequestRoute } from "./department-authority";
import { assertion, executeCommand, replayCommand } from "./commands";
import {
  activeStaff,
  staffGuard,
  type RecordScope,
} from "./staff-policy";
import {
  approvalView,
  openApproval,
  prepareDecision,
  closeApprovals,
  participantSql,
  type ApprovalView,
} from "./approvals";
import { hasPermission } from "@/lib/permissions";
import { prepareAttachmentBinding } from "./attachments/service";
import {
  availableLeaveEvidence,
  leaveEvidenceManifest,
  parseLeaveEvidencePolicy,
  pendingLeaveEvidenceCount,
  type LeaveEvidencePolicy,
  type LeaveEvidencePurpose,
} from "./leave-evidence";

export { assertLeaveAttachmentAccess } from "./leave-evidence";

export const leaveDraftSchema = z.object({
  startDate: z.iso.date(),
  days: z.number().int().min(1).max(365),
  handoverNote: z.string().trim().max(2048),
  expectedVersion: z.number().int().positive(),
  saveOnly: z.boolean().default(false),
});
export const leaveDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  note: z.string().trim().max(2048).default(""),
  override: z.boolean().default(false),
});
export const newLeaveSchema = leaveDraftSchema.omit({
  expectedVersion: true,
  saveOnly: true,
});
export const leaveLifecycleSchema = leaveDecisionSchema.extend({
  action: z.enum(["reject", "cancel"]),
});
export type LeaveRequest = RecordScope & {
  id: string;
  number: string;
  employee_id: string;
  person_id: string | null;
  engagement_id: string | null;
  start_date: string;
  days: number;
  handover_note: string;
  state: string;
  version: number;
  created_by: string;
  prepared_by: string | null;
  decided_by: string | null;
  decision_note: string | null;
  updated_at: number;
  leave_type_id?: string | null;
  leave_type_name?: string | null;
  end_date?: string | null;
  calculated_units?: number | null;
  cancellation_state?: string | null;
  approval: ApprovalView;
  cancellationApproval?: ApprovalView;
  start_segment?: "full"|"first_half"|"second_half";
  end_segment?: "full"|"first_half"|"second_half";
  calculation_snapshot?: string;
  policy_version_id?: string | null;
  evidence_policy_snapshot?: unknown;
  evidence_due_at?: number | null;
  evidence_version?: number;
  submitted_at?: number | null;
  permittedActions: string[];
};

export async function getLeave(
  holdingId: string,
  id: string,
  context: WorkContext,
): Promise<LeaveRequest> {
  const { write } = await holdingResources(holdingId);
  // Initialize legacy routing before checking named participant access.
  const legacy = await write
    .prepare(
      "SELECT id FROM leave_requests WHERE id=? AND company_id=? AND state='pending_approval'",
    )
    .bind(id, context.companyId)
    .first();
  if (legacy) await approvalView(write, "leave", id, context);
  const evidenceReader = hasPermission(context, "leave.evidence.read");
  const row = await write
    .prepare(
      `SELECT l.*,t.name leave_type_name FROM leave_requests l LEFT JOIN leave_types t ON t.id=l.leave_type_id AND t.company_id=l.company_id WHERE l.id=? AND l.company_id=? AND (l.created_by=? OR l.decided_by=? OR ${participantSql("leave", "l", "?", "?")} OR ${participantSql("leave_cancellation", "l", "?", "?")} OR ?::boolean)`,
    )
    .bind(id, context.companyId, context.userId, context.userId, context.userId, context.assignmentId, context.userId, context.assignmentId, evidenceReader)
    .first<Omit<LeaveRequest, "approval" | "permittedActions">>();
  if (!row) throw new ApiError(404, "NOT_FOUND", "Leave request not found.");
  const approval = await approvalView(write, "leave", id, context);
  if (["draft", "rejected"].includes(row.state))
    approval.blockedReason = (
      await peopleRequestRoute(write, context.companyId, row.created_by)
    ).blockedReason;
  const cancellationApproval = row.cancellation_state === "pending_review" ? await approvalView(write,"leave_cancellation",id,context) : undefined;
  const own = row.created_by === context.userId;
  return {
    ...row,
    branch_id: row.branch_id || null,
    approval,
    cancellationApproval,
    permittedActions: [
      ...(cancellationApproval?.canDecide?["approve_cancellation","reject_cancellation"]:[]),
      ...(own && ["draft", "rejected"].includes(row.state)
        ? ["save", ...(!approval.blockedReason ? ["submit"] : [])]
        : []),
      ...(own && ["draft", "rejected", "pending_approval", "approved"].includes(row.state) && row.cancellation_state !== "pending_review"
        ? ["cancel"]
        : []),
      ...(approval.canDecide && row.state === "pending_approval"
        ? ["approve", "reject"]
        : []),
      ...(approval.canOverride && row.state === "pending_approval" ? ["override_approve","override_reject"] : []),
    ],
  };
}
export async function createLeave(
  context: WorkContext,
  input: z.infer<typeof newLeaveSchema>,
  key: string,
) {
  const { write } = await holdingResources(context.holdingId),
    id = `leave-${key}`;
  const previous = await replayCommand(
    write,
    context,
    key,
    "leave.create",
    id,
    input,
  );
  if (previous) return previous;
  const staff = await activeStaff(write, context.companyId, context.userId),
    now = Date.now();
  const employee = await write
    .prepare("SELECT id FROM employees WHERE person_id=? AND company_id=?")
    .bind(staff.person_id, context.companyId)
    .first<{ id: string }>();
  const employeeId = employee?.id ?? `employee-${staff.person_id}`;
  return executeCommand(
    write,
    context,
    key,
    "leave.create",
    input,
    id,
    1,
    { id, state: "draft", version: 1 },
    [
      staffGuard(write, key, context.companyId, context.userId),
      ...(!employee
        ? [
            write
              .prepare(
                "INSERT INTO employees(id,person_id,company_id,branch_id,department_id,display_name,status,created_at) VALUES(?,?,?,?,?,?,'active',?) ON CONFLICT DO NOTHING",
              )
              .bind(
                employeeId,
                staff.person_id,
                context.companyId,
                staff.branch_id ?? "",
                staff.department_id,
                staff.legal_name,
                now,
              ),
          ]
        : []),
      write
        .prepare(
          "INSERT INTO leave_requests(id,number,employee_id,person_id,engagement_id,company_id,branch_id,department_id,start_date,days,handover_note,created_by,created_at,updated_at) VALUES(?,?,(SELECT id FROM employees WHERE person_id=? AND company_id=?),?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          `LR-${key}`,
          staff.person_id,
          context.companyId,
          staff.person_id,
          staff.engagement_id,
          context.companyId,
          staff.branch_id ?? "",
          staff.department_id,
          input.startDate,
          input.days,
          input.handoverNote,
          context.userId,
          now,
          now,
        ),
    ],
    eventScope(staff),
  );
}
const eventScope = (row: RecordScope) => ({
  companyId: row.company_id,
  branchId: row.branch_id,
  departmentId: row.department_id,
});
function guard(
  db: SqlDatabase,
  key: string,
  id: string,
  version: number,
  states: string[],
) {
  return assertion(
    db,
    key,
    "leave-state",
    `EXISTS(SELECT 1 FROM leave_requests WHERE id=? AND version=? AND state IN (${states.map(() => "?").join(",")}))`,
    [id, version, ...states],
  );
}
export async function submitLeave(
  holdingId: string,
  id: string,
  input: z.infer<typeof leaveDraftSchema>,
  key: string,
  context: WorkContext,
) {
  const { write } = await holdingResources(holdingId),
    operation = input.saveOnly ? "leave.save" : "leave.submit";
  const prior = await replayCommand(write, context, key, operation, id, input);
  if (prior) return prior;
  const row = await getLeave(holdingId, id, context);
  if (row.created_by !== context.userId)
    throw new ApiError(
      403,
      "REQUESTER_REQUIRED",
      "Only the requester can submit or correct this request.",
    );
  const staff = await activeStaff(write, context.companyId, context.userId),
    route = await peopleRequestRoute(write, context.companyId, context.userId);
  const approval = input.saveOnly
    ? null
    : await openApproval(
        write,
        key,
        "leave",
        id,
        "request",
        context.userId,
        null,
        route,
        input.expectedVersion + 1,
      );
  const state = input.saveOnly ? "draft" : "pending_approval",
    now = Date.now();
  return executeCommand(
    write,
    context,
    key,
    operation,
    input,
    id,
    input.expectedVersion + 1,
    { id, state, version: input.expectedVersion + 1 },
    [
      guard(write, key, id, input.expectedVersion, ["draft", "rejected"]),
      staffGuard(write, key, context.companyId, context.userId),
      assertion(
        write,
        key,
        "placement",
        "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)",
        [staff.engagement_id, staff.engagement_version],
      ),
      ...(approval?.statements ?? []),
      write
        .prepare(
          "UPDATE leave_requests SET start_date=?,days=?,handover_note=?,state=?,person_id=?,engagement_id=?,branch_id=?,department_id=?,prepared_by=?,decided_by=NULL,decision_note=NULL,version=version+1,last_command_id=?,updated_at=? WHERE id=?",
        )
        .bind(
          input.startDate,
          input.days,
          input.handoverNote,
          state,
          staff.person_id,
          staff.engagement_id,
          staff.branch_id ?? "",
          staff.department_id,
          context.userId,
          key,
          now,
          id,
        ),
    ],
    eventScope(staff),
  );
}

async function decisionEvidencePolicy(
  db: SqlDatabase,
  row: LeaveRequest,
): Promise<LeaveEvidencePolicy> {
  const snapshot = parseLeaveEvidencePolicy(row.evidence_policy_snapshot);
  if (snapshot) return snapshot;

  const rule = row.policy_version_id && row.leave_type_id
    ? await db
        .prepare(
          `SELECT evidence_min_units,evidence_purpose,evidence_min_count,
             evidence_due_stage,evidence_grace_days,evidence_note,evidence_privacy
           FROM leave_policy_rules
           WHERE policy_version_id=? AND leave_type_id=?`,
        )
        .bind(row.policy_version_id, row.leave_type_id)
        .first<{
          evidence_min_units: number | null;
          evidence_purpose: LeaveEvidencePurpose | null;
          evidence_min_count: number;
          evidence_due_stage: "before_submit" | "before_final_approval";
          evidence_grace_days: number;
          evidence_note: string;
          evidence_privacy: "requester_approvers_hr";
        }>()
    : null;
  const minimumUnits = rule?.evidence_min_units === null || rule?.evidence_min_units === undefined
      ? null
      : Number(rule.evidence_min_units),
    units = Number(row.calculated_units ?? row.days),
    minimumCount = Number(rule?.evidence_min_count ?? 0);
  return {
    required: Boolean(rule?.evidence_purpose && minimumUnits !== null && units >= minimumUnits && minimumCount > 0),
    minimumUnits,
    purposeKey: rule?.evidence_purpose ?? null,
    minimumCount,
    dueStage: rule?.evidence_due_stage ?? "before_final_approval",
    graceDays: Number(rule?.evidence_grace_days ?? 0),
    privacy: rule?.evidence_privacy ?? "requester_approvers_hr",
    note: rule?.evidence_note ?? "",
  };
}

async function decide(
  context: WorkContext,
  id: string,
  input: z.infer<typeof leaveDecisionSchema>,
  key: string,
  approve: boolean,
) {
  const { write } = await holdingResources(context.holdingId),
    operation = approve ? "leave.approve" : "leave.reject";
  const prior = await replayCommand(write, context, key, operation, id, input);
  if (prior) return prior;
  const row = input.override ? await write.prepare("SELECT * FROM leave_requests WHERE id=? AND company_id=?").bind(id,context.companyId).first<LeaveRequest>() : await getLeave(context.holdingId, id, context),
    checked = row ?? (()=>{throw new ApiError(404,"NOT_FOUND","Leave request not found.");})(),
    decision = await prepareDecision(
      write,
      context,
      key,
      "leave",
      id,
      approve,
      input.note,
      input.expectedVersion,
      input.override,
    );
  const state = !approve
      ? "rejected"
      : decision.final
        ? "approved"
        : "pending_approval",
    now = Date.now();
  const evidencePolicy = await decisionEvidencePolicy(write, checked);
  const observedEvidence = await leaveEvidenceManifest(write, id);
  if (approve && decision.final) {
    const pendingEvidence = await pendingLeaveEvidenceCount(write, id);
    if (pendingEvidence)
      throw new ApiError(
        409,
        "LEAVE_EVIDENCE_SCANNING",
        "Wait for every leave evidence file to finish security scanning before final approval.",
      );
    if (evidencePolicy.required && evidencePolicy.purposeKey) {
      const available = await availableLeaveEvidence(write, id, evidencePolicy);
      if (available.count < evidencePolicy.minimumCount)
        throw new ApiError(
          409,
          "LEAVE_EVIDENCE_REQUIRED",
          `Final approval requires ${evidencePolicy.minimumCount} clean ${evidencePolicy.purposeKey.replaceAll("_", " ")} file${evidencePolicy.minimumCount === 1 ? "" : "s"}.`,
        );
    }
  }
  const binding = observedEvidence.draftSetIds.length
    ? await prepareAttachmentBinding(write, context, {
        attachmentSetIds: observedEvidence.draftSetIds,
        bindingContract: {
          targetType: "leave_request",
          bindingType: "leave_approval",
          contexts: ["medical_certificate", "leave_supporting_photo"].flatMap(purposeKey =>
            (["record", "action"] as const).map(componentKind => ({ purposeKey, componentKind, componentRef: `leave-evidence:${purposeKey}` }))),
        },
        targetId: id,
        expectedTargetVersion: input.expectedVersion,
        commandId: key,
        bindingId: decision.instance.id,
      })
    : null;
  // The query captures the complete manifest visible to this reviewer. Binding
  // freezes only newly staged sets; already-bound files stay in every later
  // decision manifest without being rebound.
  const evidenceManifest = observedEvidence.manifest;
  const evidenceSnapshotGuards = [
        assertion(
          write,
          key,
          "leave-evidence-canonical-count",
          `(
            SELECT count(*)
            FROM attachment_sets set_row
            JOIN attachment_links link ON link.set_id=set_row.id
            JOIN attachments source ON source.id=link.attachment_id
            WHERE set_row.target_type='leave_request' AND set_row.target_id=?
              AND set_row.company_id=? AND source.company_id=set_row.company_id
              AND set_row.state IN ('draft','bound')
              AND link.state IN ('draft','bound')
              AND source.source_state='available'
          )=?`,
          [id, checked.company_id, observedEvidence.canonicalManifest.length],
        ),
        ...observedEvidence.canonicalManifest.map((file, index) =>
          assertion(
            write,
            key,
            `leave-evidence-manifest-${index}`,
            `EXISTS(
              SELECT 1
              FROM attachment_sets set_row
              JOIN attachment_links link ON link.set_id=set_row.id
              JOIN attachments source ON source.id=link.attachment_id
              WHERE set_row.target_type='leave_request' AND set_row.target_id=?
                AND set_row.company_id=? AND source.company_id=set_row.company_id
                AND set_row.state IN ('draft','bound')
                AND link.state IN ('draft','bound')
                AND source.source_state='available'
                AND link.id=? AND source.id=? AND set_row.purpose_key=?
                AND source.checksum_sha256=?
                AND COALESCE(source.detected_media_type,source.media_type)=?
                AND source.bytes=?
            )`,
            [
              id,
              checked.company_id,
              file.linkId,
              file.sourceId,
              file.purposeKey,
              file.checksumSha256,
              file.mediaType,
              file.bytes,
            ],
          ),
        ),
        ...(observedEvidence.legacyIncluded
          ? [
              assertion(
                write,
                key,
                "leave-evidence-legacy-count",
                `(
                  SELECT count(*)
                  FROM attachments source
                  WHERE source.owner_type='leave_request' AND source.owner_id=?
                    AND source.company_id=? AND source.revoked_at IS NULL
                    AND source.source_state='available'
                    AND NOT EXISTS(
                      SELECT 1 FROM attachment_links link WHERE link.attachment_id=source.id
                    )
                )=?`,
                [id, checked.company_id, observedEvidence.legacyManifest.length],
              ),
              ...observedEvidence.legacyManifest.map((file, index) =>
                assertion(
                  write,
                  key,
                  `leave-evidence-legacy-${index}`,
                  `EXISTS(
                    SELECT 1
                    FROM attachments source
                    WHERE source.id=? AND source.owner_type='leave_request'
                      AND source.owner_id=? AND source.company_id=?
                      AND source.revoked_at IS NULL AND source.source_state='available'
                      AND CASE
                        WHEN source.purpose IN ('medical_certificate','leave_supporting_photo')
                          THEN source.purpose
                        ELSE 'leave_supporting_photo'
                      END=?
                      AND source.checksum_sha256=?
                      AND COALESCE(source.detected_media_type,source.media_type)=?
                      AND source.bytes=?
                      AND NOT EXISTS(
                        SELECT 1 FROM attachment_links link WHERE link.attachment_id=source.id
                      )
                  )`,
                  [
                    file.sourceId,
                    id,
                    checked.company_id,
                    file.purposeKey,
                    file.checksumSha256,
                    file.mediaType,
                    file.bytes,
                  ],
                ),
              ),
            ]
          : []),
        ...(approve && decision.final
          ? [
              assertion(
                write,
                key,
                "leave-evidence-no-pending",
                `NOT EXISTS(
                  SELECT 1
                  FROM attachment_sets set_row
                  JOIN attachment_links link ON link.set_id=set_row.id
                  LEFT JOIN attachments source ON source.id=link.attachment_id
                    AND source.company_id=set_row.company_id
                  LEFT JOIN upload_intents intent ON intent.id=link.upload_intent_id
                    AND intent.company_id=set_row.company_id
                  WHERE set_row.target_type='leave_request' AND set_row.target_id=?
                    AND set_row.company_id=? AND set_row.state='draft'
                    AND link.state='draft'
                    AND (
                      (link.attachment_id IS NULL AND intent.id IS NOT NULL)
                      OR (source.id IS NOT NULL AND source.source_state<>'available')
                    )
                )`,
                [id, checked.company_id],
              ),
              ...(evidencePolicy.required && evidencePolicy.purposeKey
                ? [
                    assertion(
                      write,
                      key,
                      "leave-evidence-required-count",
                      `(
                        SELECT count(DISTINCT evidence.source_id)
                        FROM (
                          SELECT source.id source_id
                          FROM attachment_sets set_row
                          JOIN attachment_links link ON link.set_id=set_row.id
                          JOIN attachments source ON source.id=link.attachment_id
                            AND source.company_id=set_row.company_id
                          WHERE set_row.target_type='leave_request' AND set_row.target_id=?
                            AND set_row.company_id=? AND set_row.purpose_key=?
                            AND set_row.state IN ('draft','bound')
                            AND link.state IN ('draft','bound')
                            AND source.source_state='available'
                          ${observedEvidence.legacyIncluded ? `UNION ALL
                          SELECT source.id source_id
                          FROM attachments source
                          WHERE source.owner_type='leave_request' AND source.owner_id=?
                            AND source.company_id=? AND source.purpose=?
                            AND source.revoked_at IS NULL AND source.source_state='available'
                            AND NOT EXISTS(
                              SELECT 1 FROM attachment_links link WHERE link.attachment_id=source.id
                            )` : ""}
                        ) evidence
                      )>=?`,
                      observedEvidence.legacyIncluded
                        ? [
                            id,
                            checked.company_id,
                            evidencePolicy.purposeKey,
                            id,
                            checked.company_id,
                            evidencePolicy.purposeKey,
                            evidencePolicy.minimumCount,
                          ]
                        : [
                            id,
                            checked.company_id,
                            evidencePolicy.purposeKey,
                            evidencePolicy.minimumCount,
                          ],
                    ),
                  ]
                : []),
            ]
          : []),
      ];
  const evidenceManifestStep = evidenceManifest.length
    ? [
        write
          .prepare(
            `INSERT INTO leave_evidence_manifests(
              id,request_id,company_id,approval_instance_id,decision_command_id,
              request_version,policy_version_id,reviewer_user_id,manifest_json,created_at
            ) VALUES(?,?,?,?,?,?,?,?,?::jsonb,?)`,
          )
          .bind(
            `leave-evidence-${key}`,
            id,
            checked.company_id,
            decision.instance.id,
            key,
            input.expectedVersion,
            checked.policy_version_id ?? null,
            context.userId,
            JSON.stringify({
              policy: evidencePolicy,
              files: evidenceManifest,
            }),
            now,
          ),
      ]
    : [];
  return executeCommand(
    write,
    context,
    key,
    operation,
    input,
    id,
    input.expectedVersion + 1,
    { id, state, version: input.expectedVersion + 1 },
    [
      // File mutations for Leave take this same row lock and reauthorize after
      // acquiring it. That makes the reviewed manifest and the decision one
      // serializable boundary without expanding the lock to unrelated records.
      write
        .prepare(
          "SELECT id FROM leave_requests WHERE id=? AND company_id=? FOR UPDATE",
        )
        .bind(id, checked.company_id),
      guard(write, key, id, input.expectedVersion, ["pending_approval"]),
      ...evidenceSnapshotGuards,
      ...(binding?.guards ?? []),
      ...decision.statements,
      ...(binding?.steps ?? []),
      write
        .prepare(
          "UPDATE leave_requests SET state=?,decision_note=?,decided_by=?,evidence_version=evidence_version+?,version=version+1,last_command_id=?,updated_at=? WHERE id=?",
        )
        .bind(state, input.note, context.userId, binding?.manifest.length ? 1 : 0, key, now, id),
      ...((decision.final || !approve) ? balanceTransition(write,checked,key,context.userId,approve && decision.final ? "use" : "release") : []),
      ...evidenceManifestStep,
      write
        .prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)")
        .bind(
          crypto.randomUUID(),
          key,
          id,
          operation,
          context.userId,
          context.assignmentId,
          input.expectedVersion,
          await sha256(JSON.stringify(input)),
          now,
        ),
    ],
    eventScope(checked),
  );
}
export async function approveLeave(
  _holdingId: string,
  id: string,
  input: z.infer<typeof leaveDecisionSchema>,
  key: string,
  context: WorkContext,
) {
  return decide(context, id, input, key, true);
}
export async function transitionLeave(
  context: WorkContext,
  id: string,
  input: z.infer<typeof leaveLifecycleSchema>,
  key: string,
) {
  if (input.action === "reject") return decide(context, id, input, key, false);
  const { write } = await holdingResources(context.holdingId);
  const prior = await replayCommand(
    write,
    context,
    key,
    "leave.cancellation",
    id,
    input,
  );
  if (prior) return prior;
  const row = await getLeave(context.holdingId, id, context);
  if (row.created_by !== context.userId)
    throw new ApiError(
      403,
      "REQUESTER_REQUIRED",
      "Only the requester can cancel this request.",
    );
  if (row.state === "approved") {
    const route = await peopleRequestRoute(write, context.companyId, context.userId), now = Date.now();
    if(row.cancellation_state === "pending_review") throw new ApiError(409,"CANCELLATION_PENDING","Cancellation is already in review.");
    if(!input.note.trim()) throw new ApiError(422,"REASON_REQUIRED","Explain why the approved leave should be cancelled.");
    if (route.blockedReason) throw new ApiError(409,"CANCELLATION_ROUTE_BLOCKED",route.blockedReason);
    const approval = await openApproval(write,key,"leave_cancellation",id,"request",context.userId,null,route,input.expectedVersion+1);
    return executeCommand(write,context,key,"leave.cancellation",input,id,input.expectedVersion+1,{id,state:"approved",cancellationState:"pending_review",version:input.expectedVersion+1},[
      guard(write,key,id,input.expectedVersion,["approved"]),
      ...approval.statements,
      write.prepare("INSERT INTO leave_cancellation_reviews(id,request_id,company_id,reason,requested_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?)").bind(`leave-cancel-${key}`,id,row.company_id,input.note,context.userId,now,now),
      write.prepare("UPDATE leave_requests SET cancellation_state='pending_review',version=version+1,last_command_id=?,updated_at=? WHERE id=?").bind(key,now,id),
    ],eventScope(row));
  }
  return executeCommand(
    write,
    context,
    key,
    "leave.cancellation",
    input,
    id,
    input.expectedVersion + 1,
    { id, state: "cancelled", version: input.expectedVersion + 1 },
    [
      guard(write, key, id, input.expectedVersion, [
        "draft",
        "rejected",
        "pending_approval",
      ]),
      ...closeApprovals(
        write,
        key,
        "leave",
        id,
        context.userId,
        "cancel",
        input.note,
        input.expectedVersion,
      ),
      write
        .prepare(
          "UPDATE leave_requests SET state='cancelled',decision_note=?,version=version+1,last_command_id=?,updated_at=? WHERE id=?",
        )
        .bind(input.note, key, Date.now(), id),
      ...(row.state === "pending_approval" ? balanceTransition(write,row,key,context.userId,"release") : []),
    ],
    eventScope(row),
  );
}

export async function decideLeaveCancellation(context: WorkContext,id:string,input:z.infer<typeof leaveDecisionSchema>,key:string,approve:boolean) {
  const {write}=await holdingResources(context.holdingId);
  const operation=approve?"leave.cancellation.approve":"leave.cancellation.reject";
  const prior=await replayCommand(write,context,key,operation,id,input);if(prior)return prior;
  const row=await write.prepare("SELECT * FROM leave_requests WHERE id=? AND company_id=? AND state='approved' AND cancellation_state='pending_review'").bind(id,context.companyId).first<LeaveRequest>();
  if(!row) throw new ApiError(404,"NOT_FOUND","Pending leave cancellation review not found.");
  const decision=await prepareDecision(write,context,key,"leave_cancellation",id,approve,input.note,input.expectedVersion,input.override);
  const final=approve&&decision.final, now=Date.now();
  return executeCommand(write,context,key,approve?"leave.cancellation.approve":"leave.cancellation.reject",input,id,input.expectedVersion+1,{id,state:final?"cancelled":"approved",cancellationState:final?"approved":approve?"pending_review":"rejected",version:input.expectedVersion+1},[
    guard(write,key,id,input.expectedVersion,["approved"]),...decision.statements,
    write.prepare("UPDATE leave_requests SET state=?,cancellation_state=?,version=version+1,last_command_id=?,updated_at=? WHERE id=?").bind(final?"cancelled":"approved",final?"approved":approve?"pending_review":"rejected",key,now,id),
    write.prepare("UPDATE leave_cancellation_reviews SET state=?,decided_by=?,decision_note=?,version=version+1,updated_at=? WHERE request_id=? AND state='pending_approval'").bind(final?"approved":approve?"pending_approval":"rejected",context.userId,input.note,now,id),
    ...(final?balanceTransition(write,row,key,context.userId,"reverse"):[]),
  ],eventScope(row));
}

/** Release only reservations actually recorded; rejection never creates usage. */
function balanceTransition(db:SqlDatabase,row:LeaveRequest,key:string,actor:string,action:"use"|"release"|"reverse") {
 const source=action==='reverse'?"'usage','usage_reversal'":"'reservation','reservation_release'";
 const insert=(kind:string,multiplier:number)=>db.prepare(`INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,request_id,kind,units,effective_date,entitlement_year,command_key,note,created_by,created_at)
 SELECT ?||':'||entitlement_year,company_id,engagement_id,leave_type_id,request_id,?,sum(units)*?,entitlement_year||'-01-01',entitlement_year,?||':'||entitlement_year,?,?,?
 FROM leave_balance_entries WHERE request_id=? AND company_id=? AND kind IN (${source}) GROUP BY company_id,engagement_id,leave_type_id,request_id,entitlement_year HAVING sum(units)<0`)
 .bind(`${key}:${kind}`,kind,multiplier,key,action==='reverse'?'Approved cancellation':action==='use'?'Approved leave':'Released reservation',actor,Date.now(),row.id,row.company_id);
 // Usage must read the reservation before its release is inserted.
 return action==='use'?[insert('usage',1),insert('reservation_release',-1)]:[insert(action==='reverse'?'usage_reversal':'reservation_release',-1)];
}
