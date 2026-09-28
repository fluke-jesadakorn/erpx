import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { Row } from "./commerce";
import type { SqlDatabase } from "./runtime/types";
import type { WorkItem } from "@/lib/work-item";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { hasModuleAccess } from "@/data/module-access";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { createCRMLead } from "./crm";
import { marketingCampaigns, marketingCostSources } from './marketing-campaigns';
import {hasPermission} from '@/lib/permissions';
const reason = z.string().trim().min(3).max(3000),
  version = z.number().int().positive();
const assetSchema = z.object({
  kind: z.enum(["form", "content", "domain", "provider"]),
  name: z.string().trim().min(2).max(160),
  configuration: z.record(z.string(), z.unknown()),
  preview: z.record(z.string(), z.unknown()),
});
const candidateSchema = z.object({
  formId: z.string().min(1),
  formVersion: version,
  branchId: z.string().min(1),
  contactName: z.string().trim().min(2).max(160),
  contactDetail: reason,
  consentEvidence: reason,
  attribution: z.record(z.string(), z.string().max(300)),
  brief: reason,
});
const actionSchema = z.object({
  action: z.enum([
    "activate",
    "reject",
    "retire",
    "handoff",
    "record-engagement",
  ]),
  expectedVersion: version,
  reason,
  eventType: z.string().trim().min(2).max(80).optional(),
  channel: z.string().trim().min(2).max(80).optional(),
  dueDate: z.string().date().optional(),
});
function access(a: WorkContext, write = false) {
  requirePermission(a, write ? "records.write" : "records.read");
  if (!hasModuleAccess(a.modules, "marketing"))
    throw new ApiError(
      403,
      "MODULE_FORBIDDEN",
      "Your current role cannot access Marketing.",
    );
}
function formRequiresConsent(configuration: unknown): boolean {
  let value = configuration;
  if (typeof value === "string") {
    try {
      value = JSON.parse(value);
    } catch {
      return false;
    }
  }
  return (
    typeof value === "object" &&
    value !== null &&
    (value as Record<string, unknown>).consentRequired === true
  );
}
export async function marketingWorkspace(a: WorkContext) {
  access(a);
  const { write } = await holdingResources(a.holdingId),
    [assets, candidates, branches, campaignSummary] = await Promise.all([
      write
        .prepare(
          "SELECT * FROM marketing_assets WHERE company_id=? ORDER BY updated_at DESC",
        )
        .bind(a.companyId)
        .all<Row>(),
      write
        .prepare(
          "SELECT c.*,a.name form_name FROM marketing_lead_candidates c JOIN marketing_assets a ON a.id=c.form_id WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?) ORDER BY c.updated_at DESC",
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
          "SELECT id,display_name name FROM organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters') AND status='active' AND (?::text IS NULL OR id=?) ORDER BY display_name",
        )
        .bind(a.companyId, a.branchId, a.branchId)
        .all<Row>(),
      write
        .prepare(
          `SELECT COALESCE(NULLIF(BTRIM(attribution_json::jsonb->>'campaign'),''),'Unspecified') campaign,
            COALESCE(NULLIF(BTRIM(attribution_json::jsonb->>'source'),''),NULLIF(BTRIM(attribution_json::jsonb->>'utmSource'),''),'Unspecified') source,
            COUNT(*)::int captured,
            COUNT(*) FILTER (WHERE status='pending')::int pending,
            COUNT(*) FILTER (WHERE status='handed_off')::int handed_off,
            COUNT(*) FILTER (WHERE status='rejected')::int rejected
          FROM marketing_lead_candidates
          WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)
          GROUP BY 1,2 ORDER BY COUNT(*) DESC, campaign, source`,
        )
        .bind(
          a.companyId,
          a.branchId,
          a.branchId,
          a.departmentId,
          a.departmentId,
        )
        .all<Row>(),
    ]);
  const canLinkCost = hasPermission(a,'finance.read')&&hasPermission(a,'records.write');
  return {
    assets: assets.results,
    candidates: candidates.results,
    branches: branches.results,
    campaignSummary: campaignSummary.results,
    campaigns: await marketingCampaigns(a),
    canLinkCost,
    costSources: canLinkCost ? await marketingCostSources(a) : [],
  };
}
export async function createMarketingAsset(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = assetSchema.parse(raw);
  if (i.kind === "form" && !formRequiresConsent(i.configuration))
    throw new ApiError(
      422,
      "MARKETING_CONSENT_REQUIRED",
      "Lead capture forms must require consent before they can be saved.",
    );
  const { write } = await holdingResources(a.holdingId),
    id = `marketing-${i.kind}-${key}`,
    op = "marketing.asset.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const now = Date.now(),
    result = { id, status: "draft", version: 1 };
  return executeCommand(write, a, key, op, i, id, 1, result, [
    write
      .prepare(
        "INSERT INTO marketing_assets VALUES(?,?,?,?,?,?,'draft',?,NULL,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        i.kind,
        i.name,
        JSON.stringify(i.configuration),
        JSON.stringify(i.preview),
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO marketing_asset_events VALUES(?,?,1,'create',?,'Asset drafted for independent review','draft',?)",
      )
      .bind(`marketing-asset-event-${key}`, id, a.userId, now),
  ]);
}
export async function createMarketingCandidate(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = candidateSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    id = `marketing-candidate-${key}`,
    op = "marketing.candidate.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const now = Date.now(),
    result = { id, status: "pending", version: 1 };
  return executeCommand(write, a, key, op, i, id, 1, result, [
    assertion(
      write,
      key,
      "form",
      "EXISTS(SELECT 1 FROM marketing_assets WHERE id=? AND company_id=? AND kind='form' AND status='active' AND version=? AND configuration_json::jsonb->'consentRequired'='true'::jsonb)",
      [i.formId, a.companyId, i.formVersion],
    ),
    assertion(
      write,
      key,
      "branch",
      "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind IN ('branch','headquarters') AND status='active' AND (?::text IS NULL OR id=?))",
      [i.branchId, a.companyId, a.branchId, a.branchId],
    ),
    write
      .prepare(
        "INSERT INTO marketing_lead_candidates VALUES(?,?,?,?,?,?,?,?,?,?,?,'pending',NULL,?,NULL,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        i.branchId,
        a.departmentId,
        i.formId,
        i.formVersion,
        i.contactName,
        i.contactDetail,
        i.consentEvidence,
        JSON.stringify(i.attribution),
        i.brief,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO marketing_engagement_events VALUES(?,?,1,'form_submit','web',?,?,?)",
      )
      .bind(
        `marketing-engagement-${key}`,
        id,
        i.consentEvidence,
        a.userId,
        now,
      ),
  ]);
}
export async function changeMarketing(
  a: WorkContext,
  kind: "asset" | "candidate",
  id: string,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = actionSchema.parse(raw),
    { write } = await holdingResources(a.holdingId);
  if (kind === "asset") {
    const row = await write
      .prepare("SELECT * FROM marketing_assets WHERE id=? AND company_id=?")
      .bind(id, a.companyId)
      .first<Row>();
    if (!row)
      throw new ApiError(404, "MARKETING_ASSET_NOT_FOUND", "Asset not found.");
    if (
      i.action === "activate" &&
      row.kind === "form" &&
      !formRequiresConsent(row.configuration_json)
    )
      throw new ApiError(
        422,
        "MARKETING_CONSENT_REQUIRED",
        "Lead capture forms must require consent before activation.",
      );
    const expected = i.action === "retire" ? "active" : "draft",
      status =
        i.action === "activate"
          ? "active"
          : i.action === "reject"
            ? "rejected"
            : "retired";
    if (
      !["activate", "reject", "retire"].includes(i.action) ||
      row.status !== expected ||
      Number(row.version) !== i.expectedVersion
    )
      throw new ApiError(
        409,
        "CONFLICT",
        "Asset changed or transition is invalid.",
      );
    if (row.created_by === a.userId && i.action !== "retire")
      throw new ApiError(409, "CONFLICT", "Independent review is required.");
    const op = `marketing.asset.${i.action}`,
      prior = await replayCommand(write, a, key, op, id, i);
    if (prior) return prior;
    const now = Date.now();
    return executeCommand(
      write,
      a,
      key,
      op,
      i,
      id,
      i.expectedVersion + 1,
      { id, status, version: i.expectedVersion + 1 },
      [
        assertion(
          write,
          key,
          "asset",
          "EXISTS(SELECT 1 FROM marketing_assets WHERE id=? AND version=? AND status=?)",
          [id, i.expectedVersion, expected],
        ),
        write
          .prepare(
            "UPDATE marketing_assets SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?",
          )
          .bind(status, a.userId, now, id),
        write
          .prepare("INSERT INTO marketing_asset_events VALUES(?,?,?,?,?,?,?,?)")
          .bind(
            `marketing-asset-event-${key}`,
            id,
            i.expectedVersion + 1,
            i.action,
            a.userId,
            i.reason,
            status,
            now,
          ),
      ],
    );
  }
  const row = await write
    .prepare(
      "SELECT * FROM marketing_lead_candidates WHERE id=? AND company_id=?",
    )
    .bind(id, a.companyId)
    .first<Row>();
  if (!row)
    throw new ApiError(
      404,
      "MARKETING_CANDIDATE_NOT_FOUND",
      "Lead candidate not found.",
    );
  const candidateOperation =
      i.action === "record-engagement"
        ? "marketing.candidate.engagement"
        : "marketing.candidate.handoff",
    candidatePrior = await replayCommand(
      write,
      a,
      key,
      candidateOperation,
      id,
      i,
    );
  if (candidatePrior) return candidatePrior;
  if (Number(row.version) !== i.expectedVersion)
    throw new ApiError(409, "CONFLICT", "Candidate changed. Reload and retry.");
  if (i.action === "record-engagement") {
    if (!i.eventType || !i.channel)
      throw new ApiError(
        422,
        "ENGAGEMENT_DETAIL_REQUIRED",
        "Event type and channel are required.",
      );
    const op = candidateOperation;
    const now = Date.now();
    return executeCommand(
      write,
      a,
      key,
      op,
      i,
      id,
      i.expectedVersion + 1,
      { id, status: row.status, version: i.expectedVersion + 1 },
      [
        assertion(
          write,
          key,
          "candidate",
          "EXISTS(SELECT 1 FROM marketing_lead_candidates WHERE id=? AND version=? AND status='pending')",
          [id, i.expectedVersion],
        ),
        write
          .prepare(
            "UPDATE marketing_lead_candidates SET version=version+1,updated_at=? WHERE id=?",
          )
          .bind(now, id),
        write
          .prepare(
            "INSERT INTO marketing_engagement_events VALUES(?,?,?,?,?,?,?,?)",
          )
          .bind(
            `marketing-engagement-${key}`,
            id,
            i.expectedVersion + 1,
            i.eventType,
            i.channel,
            i.reason,
            a.userId,
            now,
          ),
      ],
    );
  }
  if (i.action !== "handoff" || row.status !== "pending")
    throw new ApiError(
      409,
      "INVALID_MARKETING_TRANSITION",
      "Only a pending candidate can enter CRM.",
    );
  if (row.created_by === a.userId)
    throw new ApiError(
      409,
      "CONFLICT",
      "Independent review is required before CRM handoff.",
    );
  const crm = await createCRMLead(
      a,
      {
        branchId: String(row.branch_id),
        title: `Marketing lead · ${row.contact_name}`,
        contactName: String(row.contact_name),
        contactDetail: String(row.contact_detail),
        source: `Marketing form ${row.form_id}; attribution ${row.attribution_json}`,
        sourceEvidence: `Reviewed candidate ${row.id} from form ${row.form_id} version ${row.form_version}`,
        sourceKind: 'form',
        sourceId: String(row.id),
        campaignId: (()=>{try{return JSON.parse(String(row.attribution_json)).campaignId||undefined;}catch{return undefined;}})(),
        brief: String(row.brief),
        dueDate:
          i.dueDate ??
          new Date(Date.now() + 7 * 86400000).toISOString().slice(0, 10),
      },
      `${key}-crm`,
    ),
    op = candidateOperation;
  const now = Date.now();
  return executeCommand(
    write,
    a,
    key,
    op,
    i,
    id,
    i.expectedVersion + 1,
    {
      id,
      status: "handed_off",
      version: i.expectedVersion + 1,
      crmLeadId: crm.id,
    },
    [
      assertion(
        write,
        key,
        "candidate",
        "EXISTS(SELECT 1 FROM marketing_lead_candidates WHERE id=? AND version=? AND status='pending')",
        [id, i.expectedVersion],
      ),
      write
        .prepare(
          "UPDATE marketing_lead_candidates SET status='handed_off',crm_lead_id=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(crm.id, a.userId, now, id),
      write
        .prepare(
          "INSERT INTO marketing_engagement_events VALUES(?,? ,?,'crm_handoff','internal',?,?,?)",
        )
        .bind(
          `marketing-engagement-${key}`,
          id,
          i.expectedVersion + 1,
          i.reason,
          a.userId,
          now,
        ),
    ],
  );
}
export async function marketingDetail(a: WorkContext, id: string) {
  access(a);
  const { write } = await holdingResources(a.holdingId),
    candidate = await write
      .prepare(
        "SELECT c.*,a.name form_name FROM marketing_lead_candidates c JOIN marketing_assets a ON a.id=c.form_id WHERE c.id=? AND c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)",
      )
      .bind(
        id,
        a.companyId,
        a.branchId,
        a.branchId,
        a.departmentId,
        a.departmentId,
      )
      .first<Row>();
  if (!candidate)
    throw new ApiError(
      404,
      "MARKETING_CANDIDATE_NOT_FOUND",
      "Lead candidate not found.",
    );
  return {
    candidate,
    history: (
      await write
        .prepare(
          "SELECT * FROM marketing_engagement_events WHERE candidate_id=? ORDER BY version",
        )
        .bind(id)
        .all<Row>()
    ).results,
  };
}
export type MarketingExecutiveEvidenceMetric = "marketing-lead-candidates" | "marketing-engagement-events";
export async function marketingExecutiveEvidenceRow(
  db: SqlDatabase,
  a: WorkContext,
  metric: MarketingExecutiveEvidenceMetric,
  recordId: string,
  range?: { from: number; toExclusive: number },
) {
  access(a);
  if (metric === "marketing-lead-candidates") {
    return db.prepare(`SELECT c.id,c.contact_name,c.status,c.form_version,c.version,c.created_at,c.updated_at,c.branch_id,c.department_id,
        f.name form_name
      FROM marketing_lead_candidates c
      JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id
      WHERE c.id=? AND c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?) LIMIT 1`)
      .bind(recordId,a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId).first<Row>();
  }
  if (!range) return null;
  return db.prepare(`SELECT e.id,e.candidate_id,e.event_type,e.channel,e.version event_version,e.created_at,
      c.contact_name,c.status candidate_status,c.version candidate_version,c.branch_id,c.department_id,f.name form_name
    FROM marketing_engagement_events e
    JOIN marketing_lead_candidates c ON c.id=e.candidate_id
    JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id
    WHERE e.id=? AND c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)
      AND e.created_at>=? AND e.created_at<? LIMIT 1`)
    .bind(recordId,a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId,range.from,range.toExclusive).first<Row>();
}
export async function marketingWork(a: WorkContext): Promise<WorkItem[]> {
  if (!hasModuleAccess(a.modules, "marketing")) return [];
  try {
    requirePermission(a, "records.read");
  } catch {
    return [];
  }
  const { write } = await holdingResources(a.holdingId),
    rows = (
      await write
        .prepare(
          "SELECT * FROM marketing_lead_candidates WHERE company_id=? AND status='pending' AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY updated_at DESC",
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
    type: "marketing_candidate",
    title: `Marketing lead · ${x.contact_name}`,
    detail: String(x.brief),
    href: `/marketing/${x.id}`,
    status: String(x.status),
    updatedAt: Number(x.updated_at),
    queue: x.created_by === a.userId ? "waiting" : "todo",
    canAct: x.created_by !== a.userId,
    nextAction: "Review consent and hand off to CRM",
    relationships: [x.created_by === a.userId ? "requested" : "approval"],
  }));
}
