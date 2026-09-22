import { z } from "zod";
import { samePerson } from "@/lib/iam";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";
import { companyAuthority } from "./staff-policy";
import { assertion, executeCommand, replayCommand } from "./commands";
import { hasPermission } from "@/lib/permissions";
import type { WorkItem } from "@/lib/work-item";

export const companyAcceptanceSubmitSchema = z.object({
  reason: z.string().trim().min(3).max(2000),
});
export const companyAcceptanceDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  decision: z.enum(["accept", "reject"]),
  reason: z.string().trim().min(3).max(2000),
});
export const companyAcceptanceRevokeSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});

type AcceptanceRow = {
  company_id: string;
  status: "draft" | "submitted" | "rejected" | "accepted" | "revoked";
  version: number;
  requested_by: string;
  reviewed_by: string | null;
  requested_at: number;
  reviewed_at: number | null;
  reason: string;
  review_reason: string | null;
  updated_at: number;
};
type EventRow = { id: string; action: string; actor_id: string; version: number; reason: string; created_at: number };

type Check = { id: string; label: string; ready: boolean; owner: string; detail: string };

function requireCompanyScope(actor: WorkContext) {
  if (actor.branchId || actor.departmentId || actor.scope.kind !== "holding")
    throw new ApiError(403, "COMPANY_AUTHORITY_REQUIRED", "Open the Company workspace to review Company acceptance.");
  return actor;
}

async function readChecks(db: SqlDatabase, actor: WorkContext): Promise<Check[]> {
  const company = await db.prepare("SELECT id,display_name,status FROM organization_nodes WHERE id=? AND company_id=? AND kind='company'").bind(actor.companyId, actor.companyId).first<{ id: string; display_name: string; status: string }>();
  const branches = await db.prepare("SELECT count(*) count FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active'").bind(actor.companyId).first<{ count: number }>();
  const levels = await db.prepare("SELECT count(*) count FROM staff_levels WHERE company_id=? AND active=1 AND level_number BETWEEN 1 AND 6").bind(actor.companyId).first<{ count: number }>();
  const ceo = await db.prepare("SELECT count(*) count FROM organization_positions WHERE company_id=? AND active=1 AND is_ceo=1").bind(actor.companyId).first<{ count: number }>();
  const incomplete = await db.prepare(`SELECT count(*) count FROM hr_engagements e
    JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id
    LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id
    WHERE e.company_id=? AND e.status='active' AND p.status='active' AND (l.id IS NULL OR l.active!=1 OR e.position_id IS NULL)`)
    .bind(actor.companyId).first<{ count: number }>();
  return [
    { id: "company-active", label: "Company identity is active", ready: company?.status === "active", owner: "Company administrator", detail: company?.display_name ?? "Company record is unavailable." },
    { id: "branch-ready", label: "At least one active branch", ready: Number(branches?.count ?? 0) > 0, owner: "Company administrator", detail: Number(branches?.count ?? 0) > 0 ? `${branches?.count} active branch(es)` : "Create and approve the first branch." },
    { id: "levels-ready", label: "Six numbered staff levels", ready: Number(levels?.count ?? 0) === 6, owner: "Company HR administrator", detail: Number(levels?.count ?? 0) === 6 ? "Levels 1–6 are installed." : "Install all six numbered levels." },
    { id: "ceo-ready", label: "Active CEO position", ready: Number(ceo?.count ?? 0) === 1, owner: "People administrator", detail: Number(ceo?.count ?? 0) === 1 ? "One active CEO position is configured." : "Create exactly one active CEO position." },
    { id: "staff-ready", label: "Active staff have an organization placement", ready: Number(incomplete?.count ?? 0) === 0, owner: "People manager", detail: Number(incomplete?.count ?? 0) === 0 ? "All active staff have a level and position." : `${incomplete?.count} active staff record(s) need a level or position.` },
  ];
}

export async function companyBusinessAcceptance(db: SqlDatabase, actor: WorkContext) {
  requireCompanyScope(actor);
  if (!(await companyAuthority(db, actor))) throw new ApiError(403, "COMPANY_AUTHORITY_REQUIRED", "An active Company organization-management assignment is required.");
  const [checks, acceptance, eventRows] = await Promise.all([
    readChecks(db, actor),
    db.prepare("SELECT * FROM company_business_acceptance WHERE company_id=?").bind(actor.companyId).first<AcceptanceRow>(),
    db.prepare("SELECT id,action,actor_id,version,reason,created_at FROM company_business_acceptance_events WHERE company_id=? ORDER BY version,created_at,id").bind(actor.companyId).all<EventRow>(),
  ]);
  const independent = acceptance ? !samePerson(actor, acceptance.requested_by) : false;
  const canReview = !!acceptance && acceptance.status === "submitted" && independent && hasPermission(actor, "organization.manage");
  const canSubmit = checks.every(check => check.ready) && (!acceptance || ["draft", "rejected", "revoked"].includes(acceptance.status));
  const canRevoke = acceptance?.status === "accepted" && samePerson(actor, acceptance.requested_by);
  return {
    companyId: actor.companyId,
    status: acceptance?.status ?? "draft",
    version: acceptance?.version ?? 0,
    checks,
    blockers: checks.filter(check => !check.ready).map(check => check.detail),
    canSubmit,
    canReview,
    canRevoke,
    requestedBy: acceptance?.requested_by ?? null,
    reviewedBy: acceptance?.reviewed_by ?? null,
    reason: acceptance?.reason ?? null,
    reviewReason: acceptance?.review_reason ?? null,
    updatedAt: acceptance?.updated_at ?? null,
    events: eventRows.results,
  };
}

export async function submitCompanyBusinessAcceptance(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof companyAcceptanceSubmitSchema>, key: string) {
  requireCompanyScope(actor);
  if (!(await companyAuthority(db, actor))) throw new ApiError(403, "COMPANY_AUTHORITY_REQUIRED", "An active Company organization-management assignment is required.");
  input = companyAcceptanceSubmitSchema.parse(input);
  const checks = await readChecks(db, actor);
  if (checks.some(check => !check.ready)) throw new ApiError(409, "COMPANY_SETUP_INCOMPLETE", `Complete Company setup before acceptance: ${checks.filter(check => !check.ready).map(check => check.label).join(", ")}.`);
  const current = await db.prepare("SELECT * FROM company_business_acceptance WHERE company_id=?").bind(actor.companyId).first<AcceptanceRow>();
  if (current && !["draft", "rejected", "revoked"].includes(current.status)) throw new ApiError(409, "COMPANY_ACCEPTANCE_LOCKED", "This Company acceptance is already submitted or accepted.");
  const recordVersion = (current?.version ?? 0) + 1, now = Date.now(), operation = "organization.company-acceptance.submit", recordId = actor.companyId;
  const prior = await replayCommand(db, actor, key, operation, recordId, input); if (prior) return prior;
  const result = { companyId: actor.companyId, status: "submitted" as const, version: recordVersion };
  return executeCommand(db, actor, key, operation, input, recordId, recordVersion, result, [
    ...(current ? [assertion(db, key, "acceptance-version", "EXISTS(SELECT 1 FROM company_business_acceptance WHERE company_id=? AND version=? AND status IN ('draft','rejected','revoked'))", [actor.companyId, current.version])] : []),
    db.prepare(`INSERT INTO company_business_acceptance(company_id,status,version,requested_by,requested_at,reason,updated_at)
      VALUES(?,'submitted',?,?,?, ?,?) ON CONFLICT(company_id) DO UPDATE SET status='submitted',version=excluded.version,requested_by=excluded.requested_by,reviewed_by=NULL,requested_at=excluded.requested_at,reviewed_at=NULL,reason=excluded.reason,review_reason=NULL,updated_at=excluded.updated_at`).bind(actor.companyId, recordVersion, actor.userId, now, input.reason, now),
    db.prepare("INSERT INTO company_business_acceptance_events(id,company_id,action,actor_id,version,reason,created_at) VALUES(?,?, 'submitted',?,?,?,?)").bind(`company-acceptance-event-${key}`, actor.companyId, actor.userId, recordVersion, input.reason, now),
    db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      SELECT 'company-acceptance-notice-'||?||'-'||a.user_id,?,a.user_id,'company_business_acceptance',?,?, 'Company setup acceptance is ready for independent review',? FROM assignments a
      WHERE a.company_id=? AND a.user_id<>? AND a.scope_kind='holding' AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?) AND iam_role_allows(a.role,a.company_id,'organization.manage') ON CONFLICT DO NOTHING`).bind(key, actor.companyId, actor.companyId, recordVersion, now, actor.companyId, actor.userId, now, now),
  ]);
}

export async function decideCompanyBusinessAcceptance(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof companyAcceptanceDecisionSchema>, key: string) {
  requireCompanyScope(actor);
  if (!(await companyAuthority(db, actor))) throw new ApiError(403, "COMPANY_AUTHORITY_REQUIRED", "An active Company organization-management assignment is required.");
  input = companyAcceptanceDecisionSchema.parse(input);
  const current = await db.prepare("SELECT * FROM company_business_acceptance WHERE company_id=?").bind(actor.companyId).first<AcceptanceRow>();
  if (!current) throw new ApiError(404, "COMPANY_ACCEPTANCE_NOT_FOUND", "Submit Company setup for review first.");
  if (current.status !== "submitted") throw new ApiError(409, "COMPANY_ACCEPTANCE_NOT_SUBMITTED", "Only submitted Company setup can be reviewed.");
  if (samePerson(actor, current.requested_by)) throw new ApiError(403, "INDEPENDENT_REVIEW_REQUIRED", "A different Company administrator must review this acceptance.");
  const operation = "organization.company-acceptance.review", prior = await replayCommand(db, actor, key, operation, actor.companyId, input); if (prior) return prior;
  if (current.version !== input.expectedVersion) throw new ApiError(409, "STALE_VERSION", "Company acceptance changed. Reload and review it again.");
  const now = Date.now(), version = current.version + 1, status = input.decision === "accept" ? "accepted" : "rejected";
  const result = { companyId: actor.companyId, status, version };
  return executeCommand(db, actor, key, operation, input, actor.companyId, version, result, [
    assertion(db, key, "acceptance-version", "EXISTS(SELECT 1 FROM company_business_acceptance WHERE company_id=? AND version=? AND status='submitted')", [actor.companyId, input.expectedVersion]),
    assertion(db, key, "independent-review", "NOT iam_same_person(?,?,?)", [actor.userId, current.requested_by, actor.companyId]),
    db.prepare("UPDATE company_business_acceptance SET status=?,version=?,reviewed_by=?,reviewed_at=?,review_reason=?,updated_at=? WHERE company_id=?").bind(status, version, actor.userId, now, input.reason, now, actor.companyId),
    db.prepare("INSERT INTO company_business_acceptance_events(id,company_id,action,actor_id,version,reason,created_at) VALUES(?,?, ?,?,?,?,?)").bind(`company-acceptance-event-${key}`, actor.companyId, status, actor.userId, version, input.reason, now),
    db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT 'company-acceptance-result-'||?||'-'||requested_by,company_id,requested_by,'company_business_acceptance',company_id,version,?,? FROM company_business_acceptance WHERE company_id=? ON CONFLICT DO NOTHING").bind(key, status === "accepted" ? "Company setup acceptance was approved" : "Company setup acceptance was returned for correction", now, actor.companyId),
  ]);
}

export async function revokeCompanyBusinessAcceptance(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof companyAcceptanceRevokeSchema>, key: string) {
  requireCompanyScope(actor);
  if (!(await companyAuthority(db, actor))) throw new ApiError(403, "COMPANY_AUTHORITY_REQUIRED", "An active Company organization-management assignment is required.");
  input = companyAcceptanceRevokeSchema.parse(input);
  const current = await db.prepare("SELECT * FROM company_business_acceptance WHERE company_id=?").bind(actor.companyId).first<AcceptanceRow>();
  if (!current || current.status !== "accepted") throw new ApiError(409, "COMPANY_ACCEPTANCE_NOT_ACTIVE", "Only an accepted Company setup can be revoked.");
  if (!samePerson(actor, current.requested_by)) throw new ApiError(403, "ACCEPTANCE_OWNER_REQUIRED", "Only the Company acceptance owner can revoke it.");
  const operation = "organization.company-acceptance.revoke", prior = await replayCommand(db, actor, key, operation, actor.companyId, input); if (prior) return prior;
  if (current.version !== input.expectedVersion) throw new ApiError(409, "STALE_VERSION", "Company acceptance changed. Reload and review it again.");
  const now = Date.now(), version = current.version + 1, result = { companyId: actor.companyId, status: "revoked" as const, version };
  return executeCommand(db, actor, key, operation, input, actor.companyId, version, result, [
    assertion(db, key, "acceptance-version", "EXISTS(SELECT 1 FROM company_business_acceptance WHERE company_id=? AND version=? AND status='accepted')", [actor.companyId, input.expectedVersion]),
    db.prepare("UPDATE company_business_acceptance SET status='revoked',version=?,reviewed_by=NULL,reviewed_at=?,review_reason=?,updated_at=? WHERE company_id=?").bind(version, now, input.reason, now, actor.companyId),
    db.prepare("INSERT INTO company_business_acceptance_events(id,company_id,action,actor_id,version,reason,created_at) VALUES(?,?, 'revoked',?,?,?,?)").bind(`company-acceptance-event-${key}`, actor.companyId, actor.userId, version, input.reason, now),
  ]);
}

export async function companyBusinessAcceptanceWork(db: SqlDatabase, actor: WorkContext): Promise<WorkItem[]> {
  if (actor.beta || actor.branchId || actor.departmentId || actor.scope.kind !== "holding" || !hasPermission(actor, "organization.manage")) return [];
  if (!(await companyAuthority(db, actor))) return [];
  const row = await db.prepare("SELECT * FROM company_business_acceptance WHERE company_id=?").bind(actor.companyId).first<AcceptanceRow>();
  if (!row || row.status === "draft" || row.status === "revoked") return [];
  const requester = samePerson(actor, row.requested_by), review = row.status === "submitted" && !requester && hasPermission(actor, "organization.manage");
  return [{ id: actor.companyId, type: "company_business_acceptance", title: "Company setup acceptance", detail: row.status === "submitted" ? (review ? "Review the Company setup evidence and accept or return it." : "Waiting for an independent Company administrator review.") : row.review_reason ?? "Company setup acceptance is complete.", href: "/settings/business-readiness#company-setup-acceptance", status: row.status, updatedAt: Number(row.updated_at), queue: review ? "todo" : requester && row.status === "rejected" ? "todo" : "waiting", responsibleQueue: review ? "Company administrator" : "Requester", nextAction: review ? "Accept or return Company setup" : row.status === "rejected" ? "Correct the Company setup evidence" : "Review the retained acceptance record", relationships: [ ...(requester ? ["requested" as const] : []), ...(review ? ["approval" as const] : []) ], canAct: review || (requester && row.status === "rejected") }];
}
