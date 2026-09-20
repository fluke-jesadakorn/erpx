import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { Row } from "./commerce";
import type { WorkItem } from "@/lib/work-item";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { hasPermission } from "@/lib/permissions";
import { hasModuleAccess } from "@/data/module-access";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { decimalToMinor } from "@/lib/finance-money";
import type { SqlStatement } from "./runtime/types";
const reason = z.string().trim().min(3).max(3000),
  version = z.number().int().positive(),
  date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
const reqSchema = z.object({
  jobTitle: z.string().trim().min(2).max(160),
  openings: z.number().int().positive().max(1000),
  targetStartDate: date,
  description: reason,
});
const candidateSchema = z.object({
  legalName: z.string().trim().min(2).max(200),
  email: z.string().email(),
  phone: z.string().trim().max(60).optional(),
  consentEvidence: reason,
  retentionUntil: date,
});
const appSchema = z.object({
  requisitionId: z.string().min(1),
  requisitionVersion: version,
  candidateId: z.string().min(1),
  ownerId: z.string().min(1),
});
const proposalSchema = z.object({
  applicationId: z.string().min(1),
  applicationVersion: version,
  recommendation: z.enum(["hire", "hold", "reject"]),
  scoreBps: z.number().int().min(0).max(10000),
  evidence: reason,
});
const offerSchema = z.object({
  applicationId: z.string().min(1),
  applicationVersion: version,
  currency: z
    .string()
    .length(3)
    .transform((x) => x.toUpperCase()),
  baseAmount: z.string().regex(/^\d+(\.\d{1,2})?$/),
  startDate: date,
  expiresOn: date,
  terms: reason,
});
const actionSchema = z.object({
  action: z.enum([
    "approve",
    "reject",
    "open",
    "close",
    "cancel",
    "interview",
    "select",
    "withdraw",
    "accept-proposal",
    "reject-proposal",
    "approve-offer",
    "send",
    "accept-offer",
    "decline",
    "reassign",
  ]),
  expectedVersion: version,
  reason,
  ownerId: z.string().min(1).optional(),
  acceptanceEvidence: reason.optional(),
});
function access(a: WorkContext, write = false) {
  requirePermission(a, write ? "records.write" : "records.read");
  if (!hasModuleAccess(a.modules, "talent"))
    throw new ApiError(
      403,
      "MODULE_FORBIDDEN",
      "Your current role cannot access Talent.",
    );
  if (!hasPermission(a, "records.review"))
    throw new ApiError(
      403,
      "TALENT_PRIVACY_FORBIDDEN",
      "Candidate data requires Talent reviewer access.",
    );
}
const eligibleOwner = `EXISTS(SELECT 1 FROM assignments x WHERE x.company_id=a.company_id AND x.user_id=a.owner_id AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=r.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=r.department_id)))`;
export async function talentWorkspace(a: WorkContext) {
  access(a);
  const { write } = await holdingResources(a.holdingId),
    [requisitions, candidates, applications, people] = await Promise.all([
      write
        .prepare(
          "SELECT * FROM talent_requisitions WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY updated_at DESC",
        )
        .bind(
          a.companyId,
          a.branchId,
          a.branchId,
          a.departmentId,
          a.departmentId,
        )
        .all<Row>(),
      write
        .prepare(
          "SELECT id,legal_name,email,phone,retention_until,status,version,updated_at FROM talent_candidates WHERE company_id=? ORDER BY updated_at DESC",
        )
        .bind(a.companyId)
        .all<Row>(),
      write
        .prepare(
          `SELECT a.*,r.job_title,c.legal_name candidate_name,CASE WHEN ${eligibleOwner} THEN 1 ELSE 0 END owner_eligible FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id JOIN talent_candidates c ON c.id=a.candidate_id WHERE a.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) ORDER BY a.updated_at DESC`,
        )
        .bind(
          a.companyId,
          a.branchId,
          a.branchId,
          a.departmentId,
          a.departmentId,
        )
        .all<Row>(),
      write
        .prepare(
          "SELECT DISTINCT x.user_id id,COALESCE(p.legal_name,x.user_id) name FROM assignments x LEFT JOIN hr_people p ON p.company_id=x.company_id AND p.user_id=x.user_id AND p.merged_into_id IS NULL WHERE x.company_id=? AND iam_assignment_active(x.id,?) AND iam_principal_active(x.user_id,x.company_id,?) ORDER BY 2",
        )
        .bind(a.companyId, Date.now(), Date.now())
        .all<Row>(),
    ]);
  return {
    requisitions: requisitions.results,
    candidates: candidates.results,
    applications: applications.results,
    people: people.results,
    actorId: a.userId,
    canReview: true,
  };
}
export async function createTalentRequisition(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = reqSchema.parse(raw);
  if (!a.branchId)
    throw new ApiError(
      422,
      "BRANCH_SCOPE_REQUIRED",
      "Choose a branch assignment.",
    );
  const { write } = await holdingResources(a.holdingId),
    id = `talent-req-${key}`,
    op = "talent.requisition.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const now = Date.now(),
    result = { id, status: "draft", version: 1 };
  return executeCommand(write, a, key, op, i, id, 1, result, [
    write
      .prepare(
        "INSERT INTO talent_requisitions VALUES(?,?,?,?,?,?,?,?,'draft',?,NULL,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        a.branchId,
        a.departmentId,
        i.jobTitle,
        i.openings,
        i.targetStartDate,
        i.description,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO talent_requisition_events VALUES(?,?,1,'create',?,'Hiring need drafted','draft',?)",
      )
      .bind(`talent-req-event-${key}`, id, a.userId, now),
  ]);
}
export async function createTalentCandidate(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = candidateSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    id = `talent-candidate-${key}`,
    op = "talent.candidate.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  if (i.retentionUntil <= new Date().toISOString().slice(0, 10))
    throw new ApiError(
      422,
      "INVALID_RETENTION",
      "Retention date must be in the future.",
    );
  const now = Date.now(),
    result = { id, status: "active", version: 1 };
  return executeCommand(write, a, key, op, i, id, 1, result, [
    write
      .prepare(
        "INSERT INTO talent_candidates VALUES(?,?,?,?,?,?,?,?,'active',?,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        i.legalName,
        i.email.toLowerCase(),
        i.phone ?? null,
        i.consentEvidence,
        now,
        i.retentionUntil,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO talent_candidate_events VALUES(?,?,1,'consent',?,'Candidate consent recorded','active',?)",
      )
      .bind(`talent-candidate-event-${key}`, id, a.userId, now),
  ]);
}
export async function createTalentApplication(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = appSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    req = await write
      .prepare(
        "SELECT * FROM talent_requisitions WHERE id=? AND company_id=? AND version=? AND status='open'",
      )
      .bind(i.requisitionId, a.companyId, i.requisitionVersion)
      .first<Row>();
  if (!req)
    throw new ApiError(
      409,
      "TALENT_SOURCE_CHANGED",
      "Choose a current open requisition.",
    );
  const id = `talent-application-${key}`,
    op = "talent.application.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const now = Date.now(),
    result = { id, status: "applied", version: 1 };
  return executeCommand(write, a, key, op, i, id, 1, result, [
    assertion(
      write,
      key,
      "candidate",
      "EXISTS(SELECT 1 FROM talent_candidates WHERE id=? AND company_id=? AND status='active' AND retention_until>=?)",
      [i.candidateId, a.companyId, new Date().toISOString().slice(0, 10)],
    ),
    assertion(
      write,
      key,
      "owner",
      "EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?))",
      [a.companyId, i.ownerId, now, now],
    ),
    write
      .prepare(
        "INSERT INTO talent_applications VALUES(?,?,?,?,?,'applied',?,?,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        i.requisitionId,
        i.requisitionVersion,
        i.candidateId,
        i.ownerId,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO talent_application_events VALUES(?,?,1,'apply',?,'Candidate applied to approved requisition','applied',?)",
      )
      .bind(`talent-app-event-${key}`, id, a.userId, now),
  ]);
}
export async function createTalentProposal(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = proposalSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    id = `talent-proposal-${key}`,
    op = "talent.proposal.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const now = Date.now(),
    result = { id, status: "submitted", version: 1 };
  return executeCommand(write, a, key, op, i, id, 1, result, [
    assertion(
      write,
      key,
      "application",
      "EXISTS(SELECT 1 FROM talent_applications WHERE id=? AND company_id=? AND version=? AND status='interview' AND owner_id=?)",
      [i.applicationId, a.companyId, i.applicationVersion, a.userId],
    ),
    write
      .prepare(
        "INSERT INTO talent_proposals VALUES(?,?,?,?,?,?,'submitted',?,NULL,1,?,?)",
      )
      .bind(
        id,
        i.applicationId,
        i.applicationVersion,
        i.recommendation,
        i.scoreBps,
        i.evidence,
        a.userId,
        now,
        now,
      ),
  ]);
}
export async function createTalentOffer(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = offerSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    id = `talent-offer-${key}`,
    op = "talent.offer.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  if (i.expiresOn < i.startDate)
    throw new ApiError(
      422,
      "INVALID_OFFER_DATES",
      "Offer expiry must not precede start date.",
    );
  const now = Date.now(),
    result = { id, status: "draft", version: 1 };
  return executeCommand(write, a, key, op, i, id, 1, result, [
    assertion(
      write,
      key,
      "application",
      "EXISTS(SELECT 1 FROM talent_applications a JOIN talent_proposals p ON p.application_id=a.id WHERE a.id=? AND a.company_id=? AND a.version=? AND a.status='selected' AND p.status='accepted')",
      [i.applicationId, a.companyId, i.applicationVersion],
    ),
    write
      .prepare(
        "INSERT INTO talent_offers VALUES(?,?,?,?,?,?,?,?,?,'draft',?,NULL,NULL,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        i.applicationId,
        i.applicationVersion,
        i.currency,
        decimalToMinor(i.baseAmount),
        i.startDate,
        i.expiresOn,
        i.terms,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO talent_offer_events VALUES(?,?,1,'create',?,'Offer drafted from accepted proposal','draft',?)",
      )
      .bind(`talent-offer-event-${key}`, id, a.userId, now),
  ]);
}
export async function changeTalentRecord(
  a: WorkContext,
  kind: "requisition" | "application" | "proposal" | "offer",
  id: string,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = actionSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    table =
      kind === "requisition"
        ? "talent_requisitions"
        : kind === "application"
          ? "talent_applications"
          : kind === "proposal"
            ? "talent_proposals"
            : "talent_offers",
    row = await write
      .prepare(
        `SELECT * FROM ${table} WHERE id=? ${kind === "proposal" ? "" : "AND company_id=?"}`,
      )
      .bind(...(kind === "proposal" ? [id] : [id, a.companyId]))
      .first<Row>();
  if (!row)
    throw new ApiError(
      404,
      "TALENT_RECORD_NOT_FOUND",
      "Talent record not found.",
    );
  const rules: Record<string, [string, string]> =
    kind === "requisition"
      ? {
          approve: ["draft", "approved"],
          reject: ["draft", "rejected"],
          open: ["approved", "open"],
          close: ["open", "closed"],
          cancel: ["draft", "cancelled"],
        }
      : kind === "application"
        ? {
            interview: ["applied", "interview"],
            select: ["interview", "selected"],
            reject: ["applied", "rejected"],
            withdraw: ["applied", "withdrawn"],
            reassign: [String(row.status), String(row.status)],
          }
        : kind === "proposal"
          ? {
              "accept-proposal": ["submitted", "accepted"],
              "reject-proposal": ["submitted", "rejected"],
            }
          : {
              "approve-offer": ["draft", "approved"],
              send: ["approved", "sent"],
              "accept-offer": ["sent", "accepted"],
              decline: ["sent", "declined"],
              withdraw: ["approved", "withdrawn"],
            };
  const rule = rules[i.action];
  if (
    !rule ||
    row.status !== rule[0] ||
    Number(row.version) !== i.expectedVersion
  )
    throw new ApiError(
      409,
      "CONFLICT",
      "Record changed or transition is invalid.",
    );
  if (
    [
      "approve",
      "reject",
      "accept-proposal",
      "reject-proposal",
      "approve-offer",
    ].includes(i.action) &&
    (row.created_by === a.userId || row.submitted_by === a.userId)
  )
    throw new ApiError(409, "CONFLICT", "Independent review is required.");
  if (i.action === "reassign" && !i.ownerId)
    throw new ApiError(422, "OWNER_REQUIRED", "Choose an eligible owner.");
  if (i.action === "accept-offer" && !i.acceptanceEvidence)
    throw new ApiError(
      422,
      "ACCEPTANCE_EVIDENCE_REQUIRED",
      "Record candidate acceptance evidence.",
    );
  const op = `talent.${kind}.${i.action}`,
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const now = Date.now(),
    next = i.expectedVersion + 1,
    sets = ["status=?", "version=version+1", "updated_at=?"],
    bind: (string | number | null)[] = [rule[1], now];
  if (i.action === "reassign") {
    sets.push("owner_id=?");
    bind.push(i.ownerId ?? null);
  }
  if (["approve", "reject", "approve-offer"].includes(i.action)) {
    sets.push(kind === "offer" ? "approved_by=?" : "reviewed_by=?");
    bind.push(a.userId);
  }
  if (
    kind === "proposal" &&
    ["accept-proposal", "reject-proposal"].includes(i.action)
  ) {
    sets.push("reviewed_by=?");
    bind.push(a.userId);
  }
  if (i.action === "accept-offer") {
    sets.push("accepted_evidence=?");
    bind.push(i.acceptanceEvidence ?? null);
  }
  bind.push(id);
  const statements: SqlStatement[] = [
    assertion(
      write,
      key,
      "record",
      `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=? AND status=?)`,
      [id, i.expectedVersion, rule[0]],
    ),
    ...(i.action === "reassign"
      ? [
          assertion(
            write,
            key,
            "owner",
            "EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?))",
            [a.companyId, i.ownerId!, now, now],
          ),
        ]
      : []),
    write
      .prepare(`UPDATE ${table} SET ${sets.join(",")} WHERE id=?`)
      .bind(...bind),
  ];
  if (kind === "requisition")
    statements.push(
      write
        .prepare(
          "INSERT INTO talent_requisition_events VALUES(?,?,?,?,?,?,?,?)",
        )
        .bind(
          `talent-req-event-${key}`,
          id,
          next,
          i.action,
          a.userId,
          i.reason,
          rule[1],
          now,
        ),
    );
  if (kind === "application")
    statements.push(
      write
        .prepare(
          "INSERT INTO talent_application_events VALUES(?,?,?,?,?,?,?,?)",
        )
        .bind(
          `talent-app-event-${key}`,
          id,
          next,
          i.action,
          a.userId,
          i.reason,
          rule[1],
          now,
        ),
    );
  if (kind === "offer")
    statements.push(
      write
        .prepare("INSERT INTO talent_offer_events VALUES(?,?,?,?,?,?,?,?)")
        .bind(
          `talent-offer-event-${key}`,
          id,
          next,
          i.action,
          a.userId,
          i.reason,
          rule[1],
          now,
        ),
    );
  if (kind === "application" && i.action === "select")
    statements.unshift(
      assertion(
        write,
        key,
        "proposal",
        "EXISTS(SELECT 1 FROM talent_proposals WHERE application_id=? AND status='accepted')",
        [id],
      ),
    );
  if (kind === "offer" && i.action === "accept-offer") {
    statements.push(
      write
        .prepare(
          "UPDATE talent_applications SET status='accepted',version=version+1,updated_at=? WHERE id=? AND status='offered'",
        )
        .bind(now, row.application_id),
    );
    statements.push(
      write
        .prepare(
          "INSERT INTO talent_hire_handoffs(id,offer_id,company_id,branch_id,department_id,candidate_id,source_offer_version,status,created_at) SELECT ?,o.id,o.company_id,r.branch_id,r.department_id,a.candidate_id,?,'open',? FROM talent_offers o JOIN talent_applications a ON a.id=o.application_id JOIN talent_requisitions r ON r.id=a.requisition_id WHERE o.id=?",
        )
        .bind(`talent-hire-${key}`, next, now, id),
    );
  }
  if (kind === "offer" && i.action === "send")
    statements.push(
      write
        .prepare(
          "UPDATE talent_applications SET status='offered',version=version+1,updated_at=? WHERE id=? AND status='selected'",
        )
        .bind(now, row.application_id),
    );
  return executeCommand(
    write,
    a,
    key,
    op,
    i,
    id,
    next,
    { id, status: rule[1], version: next },
    statements,
  );
}
export async function talentDetail(a: WorkContext, id: string) {
  access(a);
  const { write } = await holdingResources(a.holdingId),
    application = await write
      .prepare(
        `SELECT a.*,r.job_title,r.branch_id,r.department_id,c.legal_name candidate_name,c.email candidate_email,c.retention_until,CASE WHEN ${eligibleOwner} THEN 1 ELSE 0 END owner_eligible FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id JOIN talent_candidates c ON c.id=a.candidate_id WHERE a.id=? AND a.company_id=?`,
      )
      .bind(id, a.companyId)
      .first<Row>();
  if (!application)
    throw new ApiError(
      404,
      "TALENT_APPLICATION_NOT_FOUND",
      "Application not found.",
    );
  const history = (
      await write
        .prepare(
          "SELECT * FROM talent_application_events WHERE application_id=? ORDER BY version",
        )
        .bind(id)
        .all<Row>()
    ).results,
    proposal = await write
      .prepare("SELECT * FROM talent_proposals WHERE application_id=?")
      .bind(id)
      .first<Row>(),
    offer = await write
      .prepare("SELECT * FROM talent_offers WHERE application_id=?")
      .bind(id)
      .first<Row>(),
    handoff = offer
      ? await write
          .prepare("SELECT * FROM talent_hire_handoffs WHERE offer_id=?")
          .bind(offer.id)
          .first<Row>()
      : null;
  return {
    application,
    history,
    proposal,
    offer,
    handoff,
    actorId: a.userId,
    canReview: true,
  };
}
export async function talentWork(a: WorkContext): Promise<WorkItem[]> {
  try {
    access(a);
  } catch {
    return [];
  }
  const { write } = await holdingResources(a.holdingId),
    rows = (
      await write
        .prepare(
          `SELECT a.*,r.job_title,c.legal_name candidate_name,CASE WHEN ${eligibleOwner} THEN 1 ELSE 0 END owner_eligible FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id JOIN talent_candidates c ON c.id=a.candidate_id WHERE a.company_id=? AND a.status NOT IN ('accepted','rejected','withdrawn') AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) ORDER BY a.updated_at DESC`,
        )
        .bind(
          a.companyId,
          a.branchId,
          a.branchId,
          a.departmentId,
          a.departmentId,
        )
        .all<Row>()
    ).results;
  return rows.map((x) => ({
    id: String(x.id),
    type: "talent_application",
    title: `Talent · ${x.candidate_name}`,
    detail: `${x.job_title} · ${x.status}`,
    href: `/talent/${x.id}`,
    status: String(x.status),
    updatedAt: Number(x.updated_at),
    queue: x.owner_id === a.userId ? "todo" : "waiting",
    canAct: x.owner_id === a.userId && Boolean(x.owner_eligible),
    blocker: Boolean(x.owner_eligible)
      ? null
      : "Owner is no longer eligible; a Talent reviewer must reassign this application.",
    nextAction:
      String(x.status) === "applied"
        ? "Schedule interview"
        : String(x.status) === "interview"
          ? "Record proposal"
          : "Complete offer",
    relationships: [x.owner_id === a.userId ? "assigned" : "team"],
  }));
}
