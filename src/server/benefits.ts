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
import type { SqlDatabase, SqlStatement } from "./runtime/types";

const reason = z.string().trim().min(3).max(3000),
  version = z.number().int().positive(),
  date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/),
  currency = z
    .string()
    .length(3)
    .transform((x) => x.toUpperCase());
const planSchema = z.object({
  name: z.string().trim().min(2).max(160),
  provider: z.string().trim().min(2).max(160),
  coverageType: z.enum(["employee", "family"]),
  currency,
  employeeDeduction: z.string().regex(/^\d+(\.\d{1,2})?$/),
  employerCost: z.string().regex(/^\d+(\.\d{1,2})?$/),
  effectiveFrom: date,
  effectiveTo: date.nullable().optional(),
});
const windowSchema = z.object({
  name: z.string().trim().min(2).max(160),
  opensOn: date,
  closesOn: date,
  effectiveOn: date,
});
const dependentSchema = z.object({
  name: z.string().trim().min(2).max(160),
  relationship: z.string().trim().min(2).max(80),
  birthDate: date,
});
const electionSchema = z.object({
  planId: z.string().min(1),
  planVersion: version,
  windowId: z.string().min(1),
  windowVersion: version,
  dependentIds: z.array(z.string().min(1)).max(20).default([]),
});
const transitionSchema = z.object({
  action: z.enum([
    "publish",
    "reject",
    "retire",
    "open",
    "close",
    "cancel",
    "verify",
    "remove",
    "submit",
    "acknowledge",
    "release",
  ]),
  expectedVersion: version,
  reason,
  providerReference: z.string().trim().min(2).max(160).optional(),
});
function access(a: WorkContext, write = false) {
  requirePermission(a, write ? "records.write" : "records.read");
  if (!hasModuleAccess(a.modules, "benefits"))
    throw new ApiError(
      403,
      "MODULE_FORBIDDEN",
      "Your current role cannot access Benefits.",
    );
}
async function personForActor(a: WorkContext, write: SqlDatabase) {
  return write
    .prepare(
      "SELECT p.id,e.id engagement_id,e.branch_id,e.department_id FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' LIMIT 1",
    )
    .bind(a.companyId, a.userId)
    .first<Row>();
}
export async function benefitsWorkspace(a: WorkContext) {
  access(a);
  const { write } = await holdingResources(a.holdingId),
    p = await personForActor(a, write);
  const [plans, windows, dependents, elections] = await Promise.all([
    write
      .prepare(
        "SELECT * FROM benefit_plans WHERE company_id=? ORDER BY updated_at DESC",
      )
      .bind(a.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT * FROM benefit_enrollment_windows WHERE company_id=? ORDER BY updated_at DESC",
      )
      .bind(a.companyId)
      .all<Row>(),
    p
      ? write
          .prepare(
            "SELECT * FROM benefit_dependents WHERE company_id=? AND person_id=? ORDER BY updated_at DESC",
          )
          .bind(a.companyId, p.id)
          .all<Row>()
      : Promise.resolve({ results: [] }),
    write
      .prepare(
        "SELECT e.*,p.name plan_name,(SELECT legal_name FROM hr_people WHERE id=e.person_id) employee_name FROM benefit_elections e JOIN benefit_plans p ON p.id=e.plan_id WHERE e.company_id=? AND (? OR e.person_id=?) ORDER BY e.updated_at DESC",
      )
      .bind(a.companyId, hasPermission(a, "records.review"), p?.id ?? "")
      .all<Row>(),
  ]);
  return {
    plans: plans.results,
    windows: windows.results,
    dependents: dependents.results,
    elections: elections.results,
    actorId: a.userId,
    canWrite: hasPermission(a, "records.write"),
    canReview: hasPermission(a, "records.review"),
  };
}
export async function createBenefitPlan(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  requirePermission(a, "records.review");
  const i = planSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    id = `benefit-plan-${key}`,
    op = "benefits.plan.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  if (i.effectiveTo && i.effectiveTo < i.effectiveFrom)
    throw new ApiError(
      422,
      "INVALID_EFFECTIVE_RANGE",
      "Effective end must follow start.",
    );
  const now = Date.now(),
    result = { id, status: "draft", version: 1 };
  return executeCommand(write, a, key, op, i, id, 1, result, [
    write
      .prepare(
        "INSERT INTO benefit_plans VALUES(?,?,?,?,?,?,?,?,?,?, 'draft',?,NULL,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        i.name,
        i.provider,
        i.coverageType,
        i.currency,
        decimalToMinor(i.employeeDeduction),
        decimalToMinor(i.employerCost),
        i.effectiveFrom,
        i.effectiveTo ?? null,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO benefit_plan_events VALUES(?,?,1,'create',?,'Plan drafted for independent review','draft',?)",
      )
      .bind(`benefit-plan-event-${key}`, id, a.userId, now),
  ]);
}
export async function createBenefitWindow(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  requirePermission(a, "records.review");
  const i = windowSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    id = `benefit-window-${key}`,
    op = "benefits.window.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  if (i.closesOn < i.opensOn)
    throw new ApiError(
      422,
      "INVALID_WINDOW",
      "Close date must follow open date.",
    );
  const now = Date.now(),
    result = { id, status: "draft", version: 1 };
  return executeCommand(write, a, key, op, i, id, 1, result, [
    write
      .prepare(
        "INSERT INTO benefit_enrollment_windows VALUES(?,?,?,?,?,?,'draft',?,NULL,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        i.name,
        i.opensOn,
        i.closesOn,
        i.effectiveOn,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO benefit_window_events VALUES(?,?,1,'create',?,'Window drafted for independent review','draft',?)",
      )
      .bind(`benefit-window-event-${key}`, id, a.userId, now),
  ]);
}
export async function createBenefitDependent(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = dependentSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    p = await personForActor(a, write);
  if (!p)
    throw new ApiError(
      422,
      "BENEFIT_WORKER_REQUIRED",
      "An active worker record is required.",
    );
  const id = `benefit-dependent-${key}`,
    op = "benefits.dependent.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const now = Date.now(),
    result = { id, status: "pending", version: 1 };
  return executeCommand(write, a, key, op, i, id, 1, result, [
    write
      .prepare(
        "INSERT INTO benefit_dependents VALUES(?,?,?,?,?,?,'pending',?,NULL,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        p.id,
        i.name,
        i.relationship,
        i.birthDate,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO benefit_dependent_events VALUES(?,?,1,'create',?,'Dependent submitted for verification','pending',?)",
      )
      .bind(`benefit-dependent-event-${key}`, id, a.userId, now),
  ]);
}
export async function createBenefitElection(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = electionSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    p = await personForActor(a, write);
  if (!p)
    throw new ApiError(
      422,
      "BENEFIT_WORKER_REQUIRED",
      "An active worker record is required.",
    );
  const plan = await write
      .prepare(
        "SELECT * FROM benefit_plans WHERE id=? AND company_id=? AND version=? AND status='published'",
      )
      .bind(i.planId, a.companyId, i.planVersion)
      .first<Row>(),
    win = await write
      .prepare(
        "SELECT * FROM benefit_enrollment_windows WHERE id=? AND company_id=? AND version=? AND status='open'",
      )
      .bind(i.windowId, a.companyId, i.windowVersion)
      .first<Row>();
  if (!plan || !win)
    throw new ApiError(
      409,
      "BENEFIT_SOURCE_CHANGED",
      "Choose current published plan and open window versions.",
    );
  const today = new Date().toISOString().slice(0, 10);
  if (today < String(win.opens_on) || today > String(win.closes_on))
    throw new ApiError(
      409,
      "ENROLLMENT_WINDOW_CLOSED",
      "The enrollment window is not active today.",
    );
  if (String(plan.coverage_type) === "family" && !i.dependentIds.length)
    throw new ApiError(
      422,
      "VERIFIED_DEPENDENT_REQUIRED",
      "Family coverage requires a verified dependent.",
    );
  if (i.dependentIds.length) {
    const found = (
      await write
        .prepare(
          "SELECT id FROM benefit_dependents WHERE company_id=? AND person_id=? AND status='verified'",
        )
        .bind(a.companyId, p.id)
        .all<{ id: string }>()
    ).results;
    const verified = new Set(found.map((row) => row.id));
    if (
      new Set(i.dependentIds).size !== i.dependentIds.length ||
      i.dependentIds.some((id) => !verified.has(id))
    )
      throw new ApiError(
        422,
        "VERIFIED_DEPENDENT_REQUIRED",
        "Every selected dependent must be verified.",
      );
  }
  const id = `benefit-election-${key}`,
    op = "benefits.election.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const now = Date.now(),
    result = { id, status: "elected", version: 1 };
  return executeCommand(write, a, key, op, i, id, 1, result, [
    write
      .prepare(
        "INSERT INTO benefit_elections(id,company_id,person_id,engagement_id,plan_id,plan_version,window_id,window_version,coverage_type,dependent_ids_json,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,status,created_by,version,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'elected',?,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        p.id,
        p.engagement_id,
        i.planId,
        i.planVersion,
        i.windowId,
        i.windowVersion,
        plan.coverage_type,
        JSON.stringify(i.dependentIds),
        plan.currency,
        plan.employee_deduction_minor,
        plan.employer_cost_minor,
        win.effective_on,
        plan.effective_to,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO benefit_election_events VALUES(?,?,1,'elect',?,'Employee elected reviewed coverage','elected',?)",
      )
      .bind(`benefit-election-event-${key}`, id, a.userId, now),
  ]);
}
export async function changeBenefitRecord(
  a: WorkContext,
  kind: "plan" | "window" | "dependent" | "election",
  id: string,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = transitionSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    table =
      kind === "plan"
        ? "benefit_plans"
        : kind === "window"
          ? "benefit_enrollment_windows"
          : kind === "dependent"
            ? "benefit_dependents"
            : "benefit_elections",
    eventTable =
      kind === "plan"
        ? "benefit_plan_events"
        : kind === "window"
          ? "benefit_window_events"
          : kind === "dependent"
            ? "benefit_dependent_events"
            : "benefit_election_events",
    row = await write
      .prepare(`SELECT * FROM ${table} WHERE id=? AND company_id=?`)
      .bind(id, a.companyId)
      .first<Row>();
  if (!row)
    throw new ApiError(
      404,
      "BENEFIT_RECORD_NOT_FOUND",
      "Benefits record not found.",
    );
  const rules: Record<string, [string, string]> =
    kind === "plan"
      ? {
          publish: ["draft", "published"],
          reject: ["draft", "rejected"],
          retire: ["published", "retired"],
        }
      : kind === "window"
        ? {
            open: ["draft", "open"],
            close: ["open", "closed"],
            cancel: ["draft", "cancelled"],
          }
        : kind === "dependent"
          ? {
              verify: ["pending", "verified"],
              reject: ["pending", "rejected"],
              remove: ["verified", "removed"],
            }
          : {
              submit: ["elected", "submitted"],
              acknowledge: ["submitted", "acknowledged"],
              release: ["acknowledged", "released"],
              cancel: ["elected", "cancelled"],
            };
  const rule = rules[i.action];
  if (!rule || row.status !== rule[0])
    throw new ApiError(
      409,
      "INVALID_BENEFIT_TRANSITION",
      "This transition is not valid for the current status.",
    );
  if (Number(row.version) !== i.expectedVersion)
    throw new ApiError(
      409,
      "CONFLICT",
      "The record changed. Reload and retry.",
    );
  if (kind !== "election" || i.action !== "submit")
    requirePermission(a, "records.review");
  if (
    ["publish", "reject", "open", "verify"].includes(i.action) &&
    row.created_by === a.userId
  )
    throw new ApiError(409, "CONFLICT", "Independent review is required.");
  if (i.action === "acknowledge" && !i.providerReference)
    throw new ApiError(
      422,
      "PROVIDER_REFERENCE_REQUIRED",
      "Record the provider acknowledgement reference.",
    );
  if (i.action === "release" && row.acknowledged_by === a.userId)
    throw new ApiError(
      409,
      "CONFLICT",
      "Payroll release must be independent from provider acknowledgement.",
    );
  const op = `benefits.${kind}.${i.action}`,
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const now = Date.now(),
    next = i.expectedVersion + 1,
    sets = ["status=?", "version=version+1", "updated_at=?"],
    bind: (string | number | null)[] = [rule[1], now];
  if (i.action === "submit") {
    sets.push("submitted_by=?");
    bind.push(a.userId);
  }
  if (i.action === "acknowledge") {
    sets.push("provider_reference=?", "acknowledged_by=?");
    bind.push(i.providerReference ?? null, a.userId);
  }
  if (i.action === "release") {
    sets.push("released_by=?");
    bind.push(a.userId);
  }
  if (
    [
      "publish",
      "reject",
      "retire",
      "open",
      "close",
      "verify",
      "remove",
    ].includes(i.action)
  ) {
    sets.push("reviewed_by=?");
    bind.push(a.userId);
  }
  bind.push(id);
  const statements: SqlStatement[] = [
    assertion(
      write,
      key,
      "record",
      `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=? AND status=?)`,
      [id, a.companyId, i.expectedVersion, rule[0]],
    ),
    write
      .prepare(`UPDATE ${table} SET ${sets.join(",")} WHERE id=?`)
      .bind(...bind),
    write
      .prepare(`INSERT INTO ${eventTable} VALUES(?,?,?,?,?,?,?,?)`)
      .bind(
        `benefit-${kind}-event-${key}`,
        id,
        next,
        i.action,
        a.userId,
        i.reason,
        rule[1],
        now,
      ),
  ];
  if (kind === "election" && i.action === "release")
    statements.push(
      write
        .prepare(
          "INSERT INTO benefit_payroll_inputs(id,election_id,company_id,person_id,engagement_id,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,source_election_version,status,released_by,created_at) SELECT ?,id,company_id,person_id,engagement_id,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,?,'released',?,? FROM benefit_elections WHERE id=?",
        )
        .bind(`benefit-payroll-${key}`, next, a.userId, now, id),
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
export async function benefitDetail(a: WorkContext, id: string) {
  access(a);
  const { write } = await holdingResources(a.holdingId),
    p = await personForActor(a, write),
    e = await write
      .prepare(
        "SELECT e.*,p.name plan_name,(SELECT legal_name FROM hr_people WHERE id=e.person_id) employee_name FROM benefit_elections e JOIN benefit_plans p ON p.id=e.plan_id WHERE e.id=? AND e.company_id=? AND (? OR e.person_id=?)",
      )
      .bind(id, a.companyId, hasPermission(a, "records.review"), p?.id ?? "")
      .first<Row>();
  if (!e)
    throw new ApiError(
      404,
      "BENEFIT_ELECTION_NOT_FOUND",
      "Election not found.",
    );
  const history = (
      await write
        .prepare(
          "SELECT * FROM benefit_election_events WHERE election_id=? ORDER BY version",
        )
        .bind(id)
        .all<Row>()
    ).results,
    payroll = await write
      .prepare("SELECT * FROM benefit_payroll_inputs WHERE election_id=?")
      .bind(id)
      .first<Row>();
  return {
    election: e,
    history,
    payroll,
    canReview: hasPermission(a, "records.review"),
    actorId: a.userId,
  };
}
export async function benefitWork(a: WorkContext): Promise<WorkItem[]> {
  if (
    !hasModuleAccess(a.modules, "benefits") ||
    !hasPermission(a, "records.read")
  )
    return [];
  const { write } = await holdingResources(a.holdingId),
    p = await personForActor(a, write),
    rows = (
      await write
        .prepare(
          "SELECT e.*,p.name plan_name,(SELECT legal_name FROM hr_people WHERE id=e.person_id) employee_name FROM benefit_elections e JOIN benefit_plans p ON p.id=e.plan_id WHERE e.company_id=? AND e.status NOT IN ('released','cancelled') AND (? OR e.person_id=?) ORDER BY e.updated_at DESC",
        )
        .bind(a.companyId, hasPermission(a, "records.review"), p?.id ?? "")
        .all<Row>()
    ).results;
  return rows.map((x) => {
    const review = ["submitted", "acknowledged"].includes(String(x.status)),
      canAct = review
        ? hasPermission(a, "records.review") && x.created_by !== a.userId
        : x.person_id === p?.id;
    return {
      id: String(x.id),
      type: "benefit_enrollment",
      title: `Benefits · ${x.plan_name}`,
      detail: `${x.employee_name} · ${x.status}`,
      href: `/benefits/${x.id}`,
      status: String(x.status),
      updatedAt: Number(x.updated_at),
      queue: canAct ? "todo" : "waiting",
      canAct,
      nextAction:
        String(x.status) === "elected"
          ? "Submit election"
          : String(x.status) === "submitted"
            ? "Record provider acknowledgement"
            : "Release payroll input",
      relationships: [
        x.person_id === p?.id ? "requested" : "team",
        ...(review ? ["approval" as const] : []),
      ],
    };
  });
}
