import { requirePermission } from "@/server/iam-permissions";
import { samePerson } from "@/lib/iam";
import { hasWorkflowPermission } from "@/lib/permissions";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
// Database-only domain code shared by the Next.js server and jobs Worker.
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { RecordScope, ApprovalRoute } from "./staff-policy";
import { activeStaff, notification } from "./staff-policy";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { resourceRequestRoute, expenseRoute, spendingStepCandidates, expenseAuthoritySnapshot } from "./expense-routing";
import {
  serviceCategories,
  categoryLabel,
  type RequestKind,
  type RequestItem,
} from "@/lib/employee-services";
import {
  companyMember,
  inServiceWorkspace,
  serviceAuthoritySnapshot,
  serviceMember,
  serviceOwner,
  validateServiceScope,
} from "./service-access";
import { resolveServiceTeam, teamHandlers } from "./service-teams";
import type { WorkItem } from "@/lib/work-item";

export const employeeRequestSchema = z.object({
  kind: z.enum(["service", "equipment", "purchase"]),
  category: z.enum(serviceCategories.map(([id]) => id)).default("it"),
  title: z.string().trim().max(200).default(""),
  description: z.string().trim().max(10000).default(""),
  location: z.string().trim().max(300).default(""),
  impact: z.enum(["normal", "urgent", "critical"]).default("normal"),
  branchId: z.string().min(1).nullable().default(null),
  departmentId: z.string().min(1).nullable().default(null),
  neededDate: z.union([z.iso.date(), z.literal("")]).default(""),
  currency: z
    .union([z.string().regex(/^[A-Z]{3}$/), z.literal("")])
    .default(""),
  items: z
    .array(
      z.object({
        description: z.string().trim().max(500).default(""),
        quantity: z
          .string()
          .regex(/^(?:\d{1,9}(?:\.\d{1,3})?)?$/)
          .default(""),
        estimatedCost: z
          .string()
          .regex(/^(?:\d{1,12}(?:\.\d{1,2})?)?$/)
          .default(""),
      }),
    )
    .max(20)
    .default([]),
  parentRequestId: z.string().max(180).nullable().default(null),
  expectedVersion: z.number().int().positive().default(1),
});
export const requestActionSchema = z.object({
  action: z.enum([
    "submit",
    "approve",
    "return",
    "reject",
    "assign",
    "start",
    "ask",
    "reply",
    "resolve",
    "confirm",
    "reopen",
    "cancel",
    "repair",
    "link-order",
    "comment",
  ]),
  expectedVersion: z.number().int().positive(),
  note: z.string().trim().max(10000).default(""),
  assigneeUserId: z.string().max(160).nullable().default(null),
  teamId: z.string().max(180).nullable().default(null),
  approverUserId: z.string().max(160).nullable().default(null),
  purchaseOrderId: z.string().max(180).nullable().default(null),
  assetReference: z.string().trim().max(180).default(""),
});
export type EmployeeRequest = RecordScope & {
  engagement_id?:string|null;
  id: string;
  kind: RequestKind;
  category: string;
  title: string;
  description: string;
  location: string;
  impact: string;
  needed_date: string | null;
  items_json: string;
  currency: string;
  requester_user_id: string;
  created_by: string;
  parent_request_id: string | null;
  state: string;
  team_id: string | null;
  assignee_user_id: string | null;
  routing_json: string;
  approval_json: string;
  blocked_reason: string | null;
  resolution: string;
  purchase_order_id: string | null;
  handover_json: string;
  version: number;
  evidence_version: number;
  created_at: number;
  updated_at: number;
};
export type SavedApproval = {
  currentStep?: number;
  decisions?: {stage:string;userId:string;note:string;at:number;version:number}[];
  pool?: string[];
  decision?: string;
  decidedBy?: string;
  version?: number;
  evidenceVersion?: number;
  route?: ApprovalRoute;
  repairedBy?: string;
};
const terminal = ["closed", "cancelled", "rejected"];
const editable = ["draft", "returned"];
export function requestVersionGuard(
  db: SqlDatabase,
  key: string,
  row: EmployeeRequest,
) {
  return assertion(
    db,
    key,
    "request-version",
    "EXISTS(SELECT 1 FROM employee_requests WHERE id=? AND company_id=? AND version=? AND state=? AND evidence_version=?)",
    [row.id, row.company_id, row.version, row.state, row.evidence_version],
  );
}
export function requestOwn(actor: WorkContext, row: EmployeeRequest) {
  return [row.requester_user_id, row.created_by].some(id=>samePerson(actor,id));
}
async function requestAccess(
  db: SqlDatabase,
  actor: WorkContext,
  row: EmployeeRequest,
) {
  if (
    !inServiceWorkspace(actor, row) ||
    !(await serviceMember(db, row, actor.userId))
  )
    throw new ApiError(
      404,
      "REQUEST_NOT_FOUND",
      "This request is outside your current workspace.",
    );
  const owner = await serviceOwner(db, actor),
    handlers = await teamHandlers(db, row, row.team_id),
    member = handlers.find((h) => h.user_id === actor.userId);
  const saved = JSON.parse(row.approval_json) as SavedApproval;
  let route: ApprovalRoute | null = null,
    pool: string[] = [];
  if (row.state === "awaiting_approval") {
    if(saved.route?.steps.some(s=>s.snapshot?.spendingPolicy)) {
      route=saved.route;
      const step=route.steps[saved.currentStep??0];
      pool=step?await spendingStepCandidates(db,row,row.requester_user_id,[row.created_by],{...step,candidateUserIds:saved.pool},(saved.decisions??[]).map(d=>d.userId)):[];
    } else {
      route = await resourceRequestRoute(db,row.company_id,row.requester_user_id,row.created_by);
      pool = (route.steps[0]?.candidateUserIds ?? []).filter(id=>saved.pool?.includes(id));
    }
  }
  const own = requestOwn(actor, row),
    approver = pool.some(id=>samePerson(actor,id)) && hasWorkflowPermission(actor,route?.steps[saved.currentStep ?? 0]?.requiredRole ?? "people_manager");
  if (
    !own &&
    !owner &&
    !member &&
    !approver &&
    !saved.decisions?.some(d=>samePerson(actor,d.userId)) && !samePerson(actor,saved.decidedBy)
  )
    throw new ApiError(
      404,
      "REQUEST_NOT_FOUND",
      "This request is not available to you.",
    );
  return { owner, own, member, handlers, pool, route, approver, saved };
}
async function readEmployeeRequest(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
) {
  const row = await db
    .prepare("SELECT * FROM employee_requests WHERE id=? AND company_id=?")
    .bind(id, actor.companyId)
    .first<EmployeeRequest>();
  if (!row) throw new ApiError(404, "REQUEST_NOT_FOUND", "Request not found.");
  return row;
}
export async function getEmployeeRequest(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
) {
  const row = await readEmployeeRequest(db, actor, id);
  await requestAccess(db, actor, row);
  return row;
}
export async function requestAttachments(db: SqlDatabase, id: string) {
  return (
    await db
      .prepare(
        "SELECT id,original_name,media_type,bytes,checksum_sha256,purpose,revision FROM attachments WHERE owner_type='employee_request' AND owner_id=? AND revoked_at IS NULL ORDER BY created_at,id",
      )
      .bind(id)
      .all<{
        id: string;
        original_name: string;
        media_type: string;
        bytes: number;
        checksum_sha256: string;
        purpose: string;
        revision: number;
      }>()
  ).results;
}
export async function employeeRequestDetail(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  includeEvidence = true,
) {
  const row = await readEmployeeRequest(db, actor, id),
    access = await requestAccess(db, actor, row);
  const blocked =
    row.state === "awaiting_approval" && !access.pool.length
      ? (access.route?.blockedReason ??
        "The independent approver is unavailable. Resolve the approval issue.")
      : row.team_id &&
          !terminal.includes(row.state) &&
          !editable.includes(row.state) &&
          row.state !== "awaiting_approval" &&
          (!access.handlers.some((h) => h.responsibility === "lead") ||
            (row.assignee_user_id &&
              !access.handlers.some((h) => h.user_id === row.assignee_user_id)))
        ? "The assigned team or handler is unavailable. Reassign this request in Administration."
        : row.blocked_reason;
  const actions: string[] = [];
  if (access.own && editable.includes(row.state))
    actions.push("edit", "submit", "cancel");
  if (access.own && row.state === "blocked") actions.push("cancel");
  if (access.approver) actions.push("approve", "return", "reject");
  if (
    access.member?.responsibility === "lead" &&
    ["queued", "in_progress", "waiting_requester"].includes(row.state)
  )
    actions.push("assign");
  if (!terminal.includes(row.state) && !editable.includes(row.state))
    actions.push("comment");
  const handler = !!access.member && samePerson(actor,row.assignee_user_id);
  if (handler && row.state === "queued") actions.push("start");
  if (handler && row.state === "in_progress")
    actions.push(
      "ask",
      "resolve",
      ...(row.kind === "purchase" ? ["link-order"] : []),
    );
  if (access.own && row.state === "waiting_requester") actions.push("reply");
  if (
    actor.userId === row.requester_user_id &&
    ["resolved", "fulfilled"].includes(row.state)
  )
    actions.push("confirm", "reopen");
  if (
    access.owner &&
    !terminal.includes(row.state) &&
    !editable.includes(row.state)
  )
    actions.push("repair");
  if (access.member && row.kind === "service" && !terminal.includes(row.state))
    actions.push("resource-request");
  const events = includeEvidence
    ? (
        await db
          .prepare(
            "SELECT e.id,e.action,e.actor_id,COALESCE((SELECT legal_name FROM hr_people p WHERE p.company_id=e.company_id AND p.user_id=e.actor_id AND p.merged_into_id IS NULL LIMIT 1),'Company member') actor_name,e.note,e.evidence_json,e.request_version,e.created_at FROM employee_request_events e WHERE request_id=? AND company_id=? ORDER BY created_at,id LIMIT 250",
          )
          .bind(id, actor.companyId)
          .all<{
            id: string;
            action: string;
            actor_id: string;
            actor_name: string;
            note: string;
            evidence_json: string;
            request_version: number;
            created_at: number;
          }>()
      ).results
    : [];
  const team = row.team_id
    ? await db
        .prepare("SELECT name FROM service_teams WHERE id=? AND company_id=?")
        .bind(row.team_id, actor.companyId)
        .first<{ name: string }>()
    : null;
  const children = includeEvidence
    ? (
        await db
          .prepare(
            "SELECT id,title,kind,state FROM employee_requests WHERE parent_request_id=? AND company_id=? ORDER BY created_at LIMIT 50",
          )
          .bind(id, actor.companyId)
          .all<{ id: string; title: string; kind: string; state: string }>()
      ).results
    : [];
  const visibleChildren = [];
  for (const child of children) {
    try {
      await getEmployeeRequest(db, actor, child.id);
      visibleChildren.push(child);
    } catch (e) {
      if (!(e instanceof ApiError) || e.status !== 404) throw e;
    }
  }
  const approvalStage=access.saved.route?.steps[access.saved.currentStep??0]?.stage;
  const approvalLabel=approvalStage==="accounts_review"?"Accounting preparation":approvalStage==="accounts_approval"?"Accounting approval (Level 1–4)":"Supervisor approval";
  const responsible =
    row.state === "awaiting_approval"
      ? approvalLabel
      : row.state === "waiting_requester" ||
          ["resolved", "fulfilled"].includes(row.state)
        ? "Requester"
        : editable.includes(row.state)
          ? "Requester"
          : (team?.name ?? "Service setup");
  const next = blocked
    ? "Resolve routing issue"
    : row.state === "awaiting_approval"
      ? approvalStage==="accounts_review"?"Prepare planned spending details":approvalStage==="accounts_approval"?"Approve planned spending before ordering":"Review purpose and requested resources"
      : row.state === "queued"
        ? row.assignee_user_id
          ? "Start work"
          : "Assign a handler"
        : row.state === "in_progress"
          ? row.kind === "purchase"
            ? "Prepare the purchase order and record fulfillment"
            : row.kind === "equipment"
              ? "Record the equipment handover"
              : "Resolve the issue"
          : row.state === "waiting_requester"
            ? "Reply to the team"
            : row.state === "resolved" || row.state === "fulfilled"
              ? "Confirm completion or reopen"
              : editable.includes(row.state)
                ? "Complete and submit your request"
                : terminal.includes(row.state)
                  ? "No further action"
                  : "Resolve routing issue";
  return {
    request: row,
    items: JSON.parse(row.items_json) as RequestItem[],
    actions,
    blockedReason: blocked,
    responsibleQueue: responsible,
    assigneeName: row.assignee_user_id
      ? (access.handlers.find((h) => h.user_id === row.assignee_user_id)
          ?.name ?? "Assigned handler unavailable")
      : null,
    nextAction: next,
    handlers: access.member || access.owner ? access.handlers : [],
    approvers: access.owner ? access.pool : [],
    events,
    attachments: includeEvidence ? await requestAttachments(db, id) : [],
    children: visibleChildren,
    relationship: {
      own: access.own,
      assigned: handler,
      approval: access.approver,
      team: !!access.member,
    },
  };
}
function event(
  db: SqlDatabase,
  row: EmployeeRequest,
  actor: WorkContext,
  action: string,
  note: string,
  evidence: unknown = {},
) {
  return db
    .prepare(
      "INSERT INTO employee_request_events(id,company_id,request_id,request_version,action,actor_id,note,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)",
    )
    .bind(
      crypto.randomUUID(),
      row.company_id,
      row.id,
      row.version,
      action,
      actor.userId,
      note,
      JSON.stringify(evidence),
      Date.now(),
    );
}
async function requestNotice(
  db: SqlDatabase,
  row: EmployeeRequest,
  actor: WorkContext,
  message: string,
  extra: string[] = [],
) {
  const recipients = new Set([row.requester_user_id, row.created_by, ...extra]);
  recipients.delete(actor.userId);
  const statements = [];
  for (const user of recipients)
    if (await serviceMember(db, row, user))
      statements.push(
        notification(db, row, user, "request", row.id, row.version, message),
      );
  return statements;
}
export async function saveEmployeeRequest(
  db: SqlDatabase,
  actor: WorkContext,
  id: string | null,
  input: z.infer<typeof employeeRequestSchema>,
  key: string,
) {
  const snapshot = await serviceAuthoritySnapshot(db, actor.companyId),
    recordId = id ?? `request-${key}`,
    operation = id ? "employee.request.update" : "employee.request.create";
  if (!(await companyMember(db, actor.companyId, actor.userId)))
    throw new ApiError(
      403,
      "MEMBERSHIP_REQUIRED",
      "Current company membership is required.",
    );
  const prior = await replayCommand<{
    id: string;
    version: number;
    state: string;
  }>(db, actor, key, operation, recordId, input);
  if (prior) return prior;
  const current = id ? await getEmployeeRequest(db, actor, id) : null;
  if (
    current &&
    (!requestOwn(actor, current) || !editable.includes(current.state))
  )
    throw new ApiError(
      403,
      "REQUEST_LOCKED",
      "Only the requester or preparer can edit an unsubmitted request.",
    );
  if (current && current.version !== input.expectedVersion)
    throw new ApiError(
      409,
      "VERSION_CONFLICT",
      "This request changed. Review the current version before saving.",
    );
  if (
    current &&
    (input.kind !== current.kind ||
      input.parentRequestId !== current.parent_request_id)
  )
    throw new ApiError(
      422,
      "REQUEST_IDENTITY_LOCKED",
      "Request type and original service link cannot change.",
    );
  let requester = current?.requester_user_id ?? actor.userId;
  if (!current && input.parentRequestId) {
    const parent = await getEmployeeRequest(db, actor, input.parentRequestId),
      a = await requestAccess(db, actor, parent);
    if (
      parent.kind !== "service" ||
      input.kind === "service" ||
      (!a.member && !a.own) ||
      terminal.includes(parent.state)
    )
      throw new ApiError(
        403,
        "RESOURCE_LINK_FORBIDDEN",
        "Create a resource request from an active service job you handle or requested.",
      );
    requester = parent.requester_user_id;
  }
  const engagement=input.kind === "purchase" ? await activeStaff(db,actor.companyId,requester) : null;
  if(current?.engagement_id&&engagement&&current.engagement_id!==engagement.engagement_id)throw new ApiError(409,"ENGAGEMENT_CHANGED","Start a new request for the current engagement.");
  const scope = {
    company_id: actor.companyId,
    branch_id: input.branchId ?? actor.branchId,
    department_id: input.departmentId ?? actor.departmentId,
  };
  await validateServiceScope(db, actor, scope);
  if (!(await serviceMember(db, scope, requester)))
    throw new ApiError(
      422,
      "REQUESTER_SCOPE_REQUIRED",
      "The requester needs access covering this location.",
    );
  const category =
    input.kind === "equipment"
      ? "operations"
      : input.kind === "purchase"
        ? "purchasing"
        : input.category;
  const version = current ? current.version + 1 : 1,
    now = Date.now();
  const values = [
    scope.branch_id,
    scope.department_id,
    category,
    input.title,
    input.description,
    input.location,
    input.impact,
    input.neededDate || null,
    JSON.stringify(input.items),
    input.currency,
  ];
  const row = { id: recordId, ...scope, version } as EmployeeRequest;
  const statements = [
    ...snapshot(key),
    ...(current
      ? [
          requestVersionGuard(db, key, current),
          db
            .prepare(
              "UPDATE employee_requests SET branch_id=?,department_id=?,category=?,title=?,description=?,location=?,impact=?,needed_date=?,items_json=?,currency=?,state='draft',blocked_reason=NULL,approval_json='{}',version=?,updated_at=? WHERE id=? AND company_id=?",
            )
            .bind(...values, version, now, recordId, actor.companyId),
        ]
      : [
          db
            .prepare(
              "INSERT INTO employee_requests(id,company_id,engagement_id,branch_id,department_id,category,title,description,location,impact,needed_date,items_json,currency,kind,requester_user_id,created_by,parent_request_id,version,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            )
            .bind(
              recordId,
              actor.companyId,
              engagement?.engagement_id??null,
              ...values,
              input.kind,
              requester,
              actor.userId,
              input.parentRequestId,
              version,
              now,
              now,
            ),
        ]),
    event(db, row, actor, current ? "edit" : "create", "", {
      fields: input,
      requester,
    }),
  ];
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    recordId,
    version,
    { id: recordId, version, state: "draft" },
    statements,
    {
      companyId: scope.company_id,
      branchId: scope.branch_id,
      departmentId: scope.department_id,
    },
  );
}
async function submittedRoute(db: SqlDatabase, row: EmployeeRequest) {
  if (
    row.title.length < 2 ||
    row.description.length < 2 ||
    !row.location.trim()
  )
    throw new ApiError(
      422,
      "REQUEST_DETAILS_REQUIRED",
      "Add a title, description or purpose, and location before submitting.",
    );
  if (row.kind === "service") return null;
  const items = JSON.parse(row.items_json) as RequestItem[];
  if (
    !items.length ||
    items.some(
      (i) =>
        !i.description.trim() ||
        !Number.isFinite(Number(i.quantity)) ||
        Number(i.quantity) <= 0,
    ) ||
    (row.kind === "equipment" && !row.needed_date)
  )
    throw new ApiError(
      422,
      "RESOURCE_DETAILS_REQUIRED",
      row.kind === "equipment"
        ? "Add the requested items, positive quantities, and needed date."
        : "Add the requested items and positive quantities.",
    );
  if (items.some((i) => i.estimatedCost) && !row.currency)
    throw new ApiError(
      422,
      "CURRENCY_REQUIRED",
      "Choose a currency for estimated costs.",
    );
  const staff = await activeStaff(db, row.company_id, row.requester_user_id);
  if (
    staff.branch_id !== row.branch_id ||
    staff.department_id !== row.department_id
  )
    throw new ApiError(
      409,
      "REQUESTER_LOCATION_REQUIRED",
      "Use the requester’s active branch and department. Review their People profile.",
    );
  return row.kind === "purchase" ? expenseRoute(db,row.company_id,row.requester_user_id,[row.created_by],false,"purchase") : resourceRequestRoute(db,row.company_id,row.requester_user_id,row.created_by);
}
export async function actOnEmployeeRequest(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  input: z.infer<typeof requestActionSchema>,
  key: string,
) {
  const snapshot = await serviceAuthoritySnapshot(db, actor.companyId);
  const spendingSnapshot = await expenseAuthoritySnapshot(db, actor.companyId);
  const current = await getEmployeeRequest(db, actor, id),
    detail = await employeeRequestDetail(db, actor, id),
    operation = `employee.request.${input.action}`;
  const prior = await replayCommand<{
    id: string;
    version: number;
    state: string;
  }>(db, actor, key, operation, id, input);
  if (prior) return prior;
  if (current.version !== input.expectedVersion)
    throw new ApiError(
      409,
      "VERSION_CONFLICT",
      "The request changed. Refresh and review before continuing.",
    );
  if (!detail.actions.includes(input.action))
    throw new ApiError(
      403,
      "REQUEST_ACTION_FORBIDDEN",
      "This action is not available to you at the current step.",
    );
  await validateServiceScope(db, actor, current);
  const row = {
      ...current,
      version: current.version + 1,
      updated_at: Date.now(),
    },
    evidence: Record<string, unknown> = {},
    extra: string[] = [];
  const statements: SqlStatement[] = [
    ...snapshot(key),
    requestVersionGuard(db, key, current),
    spendingSnapshot.guard(key,"purchase-approval-authority"),
  ];
  const requiredNote = [
    "approve",
    "return",
    "reject",
    "ask",
    "reply",
    "resolve",
    "reopen",
    "repair",
    "link-order",
    "comment",
  ];
  if (requiredNote.includes(input.action) && input.note.length < 2)
    throw new ApiError(
      422,
      "REASON_REQUIRED",
      "Record a short reason or evidence for this action.",
    );
  async function routeTeam(explicit: string | null = null) {
    const resolved = await resolveServiceTeam(db, row, row.category);
    if (explicit && resolved?.team.id !== explicit)
      throw new ApiError(
        422,
        "TEAM_COVERAGE_REQUIRED",
        "Choose the currently eligible team covering this category and location.",
      );
    row.assignee_user_id = null;
    if (!resolved) {
      row.state = "blocked";
      row.team_id = null;
      row.blocked_reason = `Configure ${categoryLabel(row.category)} coverage for this location in Administration.`;
      row.routing_json = "{}";
      return;
    }
    row.team_id = resolved.team.id;
    row.state = "queued";
    row.blocked_reason = null;
    row.routing_json = JSON.stringify({
      teamId: resolved.team.id,
      teamVersion: resolved.team.version,
      scopeKind: resolved.team.scope_kind,
      scopeNodeId: resolved.team.scope_node_id,
      category: row.category,
      at: Date.now(),
    });
    extra.push(
      ...resolved.handlers
        .filter((h) => h.responsibility === "lead")
        .map((h) => h.user_id),
    );
  }
  if (input.action === "submit") {
    if(row.kind==='purchase') {
      const staff=await activeStaff(db,row.company_id,row.requester_user_id);
      if(row.engagement_id&&row.engagement_id!==staff.engagement_id)throw new ApiError(409,'ENGAGEMENT_CHANGED','Start a new purchase request for the current engagement.');
      statements.push(db.prepare('UPDATE employee_requests SET engagement_id=? WHERE id=? AND company_id=?').bind(staff.engagement_id,row.id,row.company_id));
    }
    const route = await submittedRoute(db, row);
    if (route) {
      row.state = route.blockedReason ? "blocked" : "awaiting_approval";
      row.blocked_reason = route.blockedReason;
      row.approval_json = JSON.stringify({
        pool: route.steps[0]?.candidateUserIds ?? [],
        route: {...route,checks:undefined},
        currentStep:0, decisions:[],
        version: row.version,
        evidenceVersion: row.evidence_version,
      });
      extra.push(...(route.steps[0]?.candidateUserIds ?? []));
      for (const [i, c] of (route.checks ?? []).entries())
        statements.push(
          assertion(db, key, `request-route-${i}`, c.query, c.args),
        );
    } else await routeTeam();
  } else if (input.action === "approve") {
    const access = await requestAccess(db, actor, current);
    if (!access.pool.some(id=>samePerson(actor,id)))
      throw new ApiError(
        403,
        "APPROVER_UNAVAILABLE",
        "Current independent department authority is required.",
      );
    const strict=access.saved.route?.steps.some(s=>s.snapshot?.spendingPolicy);
    const currentStep=access.saved.currentStep??0;
    const decisions=[...(access.saved.decisions??[]),{stage:access.saved.route?.steps[currentStep]?.stage??"department_approval",userId:actor.userId,note:input.note,at:Date.now(),version:current.version}];
    const next=strict?access.saved.route?.steps[currentStep+1]:null;
    row.approval_json=JSON.stringify({...access.saved,decisions,currentStep:currentStep+1,pool:next?.candidateUserIds??[],decision:next?"pending":"approved",decidedBy:actor.userId,reviewedVersion:current.version,evidenceVersion:current.evidence_version,decisionAt:Date.now()});
    evidence.approval=JSON.parse(row.approval_json);
    evidence.attachments=await requestAttachments(db,id);
    if(next){row.state="awaiting_approval";extra.push(...(next.candidateUserIds??[]));}else await routeTeam();
  } else if (input.action === "return" || input.action === "reject") {
    row.state = input.action === "return" ? "returned" : "rejected";
    row.team_id = null;
    row.assignee_user_id = null;
    row.blocked_reason = null;
    row.approval_json = JSON.stringify({
      ...JSON.parse(row.approval_json),
      decision: input.action,
      decidedBy: actor.userId,
      reviewedVersion: current.version,
    });
  } else if (input.action === "assign") {
    if (
      !input.assigneeUserId ||
      !detail.handlers.some((h) => h.user_id === input.assigneeUserId)
    )
      throw new ApiError(
        422,
        "HANDLER_INVALID",
        "Choose a current team member with access covering this request.",
      );
    row.assignee_user_id = input.assigneeUserId;
    extra.push(input.assigneeUserId);
    evidence.previousAssignee = current.assignee_user_id;
    evidence.assigneeUserId = input.assigneeUserId;
    row.blocked_reason = null;
  } else if (input.action === "start") {
    row.state = "in_progress";
  } else if (input.action === "ask") {
    row.state = "waiting_requester";
  } else if (input.action === "reply") {
    row.state = "in_progress";
    if (row.assignee_user_id) extra.push(row.assignee_user_id);
  } else if (input.action === "resolve") {
    if (row.kind === "purchase") {
      if (!row.purchase_order_id)
        throw new ApiError(
          422,
          "PURCHASE_HANDOFF_REQUIRED",
          "Link the approved purchase request to its purchase order before confirming fulfillment.",
        );
      const order = await db
        .prepare(
          "SELECT state,version FROM commerce_orders WHERE id=? AND company_id=? AND kind='purchase' AND branch_id IS NOT DISTINCT FROM ? AND department_id IS NOT DISTINCT FROM ?",
        )
        .bind(
          row.purchase_order_id,
          row.company_id,
          row.branch_id,
          row.department_id,
        )
        .first<{ state: string; version: number }>();
      if (!order || !["fulfilled", "invoiced"].includes(order.state))
        throw new ApiError(
          409,
          "PURCHASE_NOT_RECEIVED",
          "Record completed receipt through the purchase order before confirming fulfillment.",
        );
      statements.push(
        assertion(
          db,
          key,
          "received-order-version",
          "EXISTS(SELECT 1 FROM commerce_orders WHERE id=? AND company_id=? AND version=? AND state=? AND kind='purchase' AND branch_id IS NOT DISTINCT FROM ? AND department_id IS NOT DISTINCT FROM ?)",
          [
            row.purchase_order_id,
            row.company_id,
            order.version,
            order.state,
            row.branch_id,
            row.department_id,
          ],
        ),
      );
    }
    row.state = row.kind === "service" ? "resolved" : "fulfilled";
    row.resolution = input.note;
    if (row.kind === "equipment") {
      row.handover_json = JSON.stringify({
        handedBy: actor.userId,
        receivedBy: row.requester_user_id,
        assetReference: input.assetReference,
        items: JSON.parse(row.items_json),
        note: input.note,
        at: Date.now(),
      });
      evidence.handover = JSON.parse(row.handover_json);
    }
  } else if (input.action === "confirm") {
    row.state = "closed";
    evidence.confirmedBy = actor.userId;
  } else if (input.action === "reopen") {
    row.state = "in_progress";
    row.resolution = "";
    if (row.assignee_user_id) extra.push(row.assignee_user_id);
  } else if (input.action === "cancel") {
    row.state = "cancelled";
    row.blocked_reason = null;
  } else if (input.action === "repair") {
    evidence.previousRouting = JSON.parse(current.routing_json);
    evidence.previousApproval = JSON.parse(current.approval_json);
    const approval = JSON.parse(row.approval_json) as SavedApproval;
    if (row.kind !== "service" && approval.decision !== "approved") {
      const route = await submittedRoute(db, row);
      const pool = route?.steps[0]?.candidateUserIds ?? [];
      if (!pool.length)
        throw new ApiError(
          409,
          "ROUTING_SETUP_REQUIRED",
          route?.blockedReason ??
            "Configure an eligible independent department approver.",
        );
      if (input.approverUserId && !pool.includes(input.approverUserId))
        throw new ApiError(
          422,
          "REPLACEMENT_INELIGIBLE",
          "The replacement must have current independent department authority.",
        );
      row.approval_json = JSON.stringify({
        pool: input.approverUserId ? [input.approverUserId] : pool,
        route: {...route,checks:undefined},
        currentStep:0,decisions:[],
        repairedBy: actor.userId,
        version: row.version,
        evidenceVersion: row.evidence_version,
      });
      for(const [i,c] of (route?.checks??[]).entries()) statements.push(assertion(db,key,`request-repair-${i}`,c.query,c.args));
      row.state = "awaiting_approval";
      row.blocked_reason = null;
      extra.push(...(input.approverUserId ? [input.approverUserId] : pool));
    } else await routeTeam(input.teamId);
  } else if (input.action === "link-order") {
    if (actor.beta)
      throw new ApiError(
        403,
        "BETA_CAPABILITY_UNAVAILABLE",
        "Purchasing execution is unavailable in this beta workspace.",
      );
    actor = requirePermission(actor,"commerce.purchase");
    const quotation = await db.prepare("SELECT id,order_id FROM procurement_rfqs WHERE request_id=? AND status<>'cancelled'").bind(row.id).first<{id:string;order_id:string|null}>();
    if(quotation && quotation.order_id!==input.purchaseOrderId)
      throw new ApiError(409,"REQUEST_RFQ_REQUIRED","The linked purchase order must come from the active approved supplier award.");
    if (!input.purchaseOrderId)
      throw new ApiError(422, "ORDER_REQUIRED", "Choose a purchase order.");
    const order=await db.prepare("SELECT kind,company_id,branch_id,department_id,state,version FROM commerce_orders WHERE id=? AND company_id=?").bind(input.purchaseOrderId,row.company_id).first<{kind:string;company_id:string;branch_id:string;department_id:string|null;state:string;version:number}>();
    if (!order) throw new ApiError(404,"ORDER_NOT_FOUND","Purchase order not found.");
    if (
      order.kind !== "purchase" ||
      order.company_id !== row.company_id ||
      order.branch_id !== row.branch_id ||
      order.department_id !== row.department_id ||
      ["cancelled"].includes(String(order.state))
    )
      throw new ApiError(
        422,
        "ORDER_SCOPE_INVALID",
        "Choose a purchase order for the same company, branch, and department.",
      );
    if (
      row.purchase_order_id &&
      row.purchase_order_id !== input.purchaseOrderId
    )
      throw new ApiError(
        409,
        "ORDER_ALREADY_LINKED",
        "This request already has a purchase-order handoff.",
      );
    statements.push(
      assertion(
        db,
        key,
        "linked-order-version",
        "EXISTS(SELECT 1 FROM commerce_orders WHERE id=? AND company_id=? AND version=? AND state=? AND kind='purchase' AND branch_id IS NOT DISTINCT FROM ? AND department_id IS NOT DISTINCT FROM ?)",
        [
          input.purchaseOrderId,
          row.company_id,
          Number(order.version),
          String(order.state),
          row.branch_id,
          row.department_id,
        ],
      ),
    );
    row.purchase_order_id = input.purchaseOrderId;
    evidence.purchaseOrderId = input.purchaseOrderId;
  }
  if (input.action === "comment" && row.assignee_user_id)
    extra.push(row.assignee_user_id);
  if (row.state === "blocked")
    extra.push(
      ...(
        await db
          .prepare(
            "SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND iam_role_allows(role,company_id,'company.owner') AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint)",
          )
          .bind(row.company_id)
          .all<{ user_id: string }>()
      ).results.map((o) => o.user_id),
    );
  statements.push(
    db
      .prepare(
        "UPDATE employee_requests SET state=?,team_id=?,assignee_user_id=?,routing_json=?,approval_json=?,blocked_reason=?,resolution=?,purchase_order_id=?,handover_json=?,version=?,updated_at=? WHERE id=? AND company_id=?",
      )
      .bind(
        row.state,
        row.team_id,
        row.assignee_user_id,
        row.routing_json,
        row.approval_json,
        row.blocked_reason,
        row.resolution,
        row.purchase_order_id,
        row.handover_json,
        row.version,
        row.updated_at,
        id,
        row.company_id,
      ),
    event(db, row, actor, input.action, input.note, {
      ...evidence,
      routing: JSON.parse(row.routing_json),
      from: current.state,
      to: row.state,
    }),
    ...(await requestNotice(
      db,
      row,
      actor,
      `${row.title || "Request"} · ${input.action === "ask" ? "Your reply is needed" : input.action === "assign" ? "Assigned to you" : row.state.replaceAll("_", " ")}`,
      extra,
    )),
  );
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    id,
    row.version,
    { id, version: row.version, state: row.state },
    statements,
    {
      companyId: row.company_id,
      branchId: row.branch_id,
      departmentId: row.department_id,
    },
  );
}
export async function listEmployeeRequests(db: SqlDatabase, actor: WorkContext) {
  const owner = await serviceOwner(db, actor);
  const rows = (
    await db
      .prepare(
        `SELECT r.* FROM employee_requests r WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) AND
    (?=1 OR iam_same_person(requester_user_id,?,r.company_id) OR iam_same_person(created_by,?,r.company_id) OR EXISTS(SELECT 1 FROM service_team_members m WHERE m.team_id=r.team_id AND m.company_id=r.company_id AND m.user_id=?) OR EXISTS(SELECT 1 FROM erpx_json_each(r.approval_json,'$.pool') WHERE value=?) OR erpx_json_extract(r.approval_json,'$.decidedBy')=? OR EXISTS(SELECT 1 FROM jsonb_array_elements(COALESCE(r.approval_json::jsonb -> 'decisions','[]')) decision WHERE decision ->> 'userId'=?)) ORDER BY updated_at DESC,id LIMIT 150`,
      )
      .bind(
        actor.companyId,
        actor.branchId,
        actor.branchId,
        actor.departmentId,
        actor.departmentId,
        owner ? 1 : 0,
        actor.userId,
        actor.userId,
        actor.userId,
        actor.userId,
        actor.userId,
        actor.userId,
      )
      .all<EmployeeRequest>()
  ).results;
  const visible = [];
  for (const r of rows) {
    try {
      await requestAccess(db, actor, r);
      visible.push(r);
    } catch (e) {
      if (!(e instanceof ApiError) || e.status !== 404) throw e;
    }
  }
  return visible;
}
export async function employeeRequestWork(
  db: SqlDatabase,
  actor: WorkContext,
): Promise<WorkItem[]> {
  const items: WorkItem[] = [];
  for (const row of await listEmployeeRequests(db, actor)) {
    const detail = await employeeRequestDetail(db, actor, row.id, false),
      r = detail.relationship;
    // Owners see routing exceptions, not every colleague's routine work.
    if (
      !r.own &&
      !r.assigned &&
      !r.approval &&
      !r.team &&
      !detail.blockedReason
    )
      continue;
    const actionable =
      detail.actions.some(
        (a) =>
          !["repair", "resource-request", "link-order", "comment"].includes(a),
      ) ||
      (!!detail.blockedReason && detail.actions.includes("repair"));
    items.push({
      id: row.id,
      type:
        row.kind === "service"
          ? "service_request"
          : row.kind === "equipment"
            ? "equipment_request"
            : "purchase_request",
      title: row.title || "Untitled request",
      detail: detail.nextAction,
      href: `/requests/${row.id}`,
      status: row.state,
      updatedAt: row.updated_at,
      queue: terminal.includes(row.state)
        ? "completed"
        : actionable
          ? "todo"
          : "waiting",
      responsibleQueue: detail.responsibleQueue,
      nextAction: detail.nextAction,
      blocker: detail.blockedReason,
      dueAt: row.needed_date
        ? Date.parse(row.needed_date + "T23:59:59Z")
        : null,
      relationships: [
        ...(r.own ? ["requested" as const] : []),
        ...(r.assigned ? ["assigned" as const] : []),
        ...(r.approval ? ["approval" as const] : []),
        ...(r.team ? ["team" as const] : []),
      ],
    });
  }
  return items;
}
export async function employeeRequestUpload(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  purpose?: string,
) {
  const row = await getEmployeeRequest(db, actor, id),
    detail = await employeeRequestDetail(db, actor, id);
  const canEdit = detail.actions.includes("edit"),
    canWork =
      detail.relationship.assigned &&
      ["queued", "in_progress", "waiting_requester"].includes(row.state),
    canReply = detail.relationship.own && row.state === "waiting_requester";
  if (!canEdit && !canWork && !canReply)
    throw new ApiError(
      403,
      "REQUEST_EVIDENCE_LOCKED",
      "Files can be added while preparing the request or carrying out the assigned work.",
    );
  const allowedPurpose = canEdit ? "request_evidence" : "fulfillment";
  if (purpose && purpose !== allowedPurpose)
    throw new ApiError(
      422,
      "EVIDENCE_PURPOSE_INVALID",
      `Use ${allowedPurpose.replaceAll("_", " ")} for this stage.`,
    );
  return row;
}
export async function prepareRequestOrderHandoff(
  db: SqlDatabase,
  actor: WorkContext,
  requestId: string,
  orderId: string,
  key: string,
  expectedVersion?: number,
  editing = false,
  rfqId?: string,
) {
  if (actor.beta)
    throw new ApiError(
      403,
      "BETA_CAPABILITY_UNAVAILABLE",
      "Purchasing execution is unavailable in beta.",
    );
  actor = requirePermission(actor,"commerce.purchase");
  const snap = await serviceAuthoritySnapshot(db, actor.companyId),
    detail = await employeeRequestDetail(db, actor, requestId),
    row = detail.request;
  const quotation = await db.prepare("SELECT id FROM procurement_rfqs WHERE request_id=? AND status<>'cancelled'").bind(requestId).first<{id:string}>();
  if (!editing && quotation && quotation.id !== rfqId)
    throw new ApiError(409,"REQUEST_RFQ_REQUIRED","Continue the active supplier quotation review before creating this request's purchase order.");
  if (
    row.kind !== "purchase" ||
    JSON.parse(row.approval_json).decision !== "approved" ||
    (!editing &&
      (!detail.actions.includes("link-order") || row.purchase_order_id))
  )
    throw new ApiError(
      403,
      "PURCHASE_REQUEST_REQUIRED",
      "An approved purchase request assigned to you and in progress is required for this handoff.",
    );
  if (!editing && row.version !== expectedVersion)
    throw new ApiError(
      409,
      "VERSION_CONFLICT",
      "The purchase request changed. Reopen it before preparing the order.",
    );
  if (editing && row.purchase_order_id !== orderId)
    throw new ApiError(
      409,
      "ORDER_LINK_CONFLICT",
      "This order is linked to another request.",
    );
  await validateServiceScope(db, actor, row);
  return {
    row,
    actor: {
      ...actor,
      branchId: row.branch_id,
      departmentId: row.department_id,
    },
    steps: [
      ...snap(key),
      requestVersionGuard(db, key, row),
      ...(!editing
        ? [
            db
              .prepare(
                "UPDATE employee_requests SET purchase_order_id=?,version=version+1,updated_at=? WHERE id=? AND company_id=?",
              )
              .bind(orderId, Date.now(), row.id, row.company_id),
            event(
              db,
              { ...row, version: row.version + 1 },
              actor,
              "purchase-order-created",
              "Purchase order prepared from the approved request.",
              { orderId, reviewedVersion: row.version },
            ),
            ...(await requestNotice(
              db,
              { ...row, version: row.version + 1 },
              actor,
              `${row.title} · Purchase order prepared`,
            )),
          ]
        : []),
    ],
  };
}

/** Validate the existing request owner's approval and handler before sourcing quotations. */
export async function requestQuotationSource(db:SqlDatabase,actor:WorkContext,id:string) {
  requirePermission(actor,"commerce.purchase");
  const detail=await employeeRequestDetail(db,actor,id),row=detail.request;
  if(actor.beta||row.kind!=="purchase"||JSON.parse(row.approval_json).decision!=="approved"||!detail.actions.includes("link-order")||row.purchase_order_id)
    throw new ApiError(403,"PURCHASE_REQUEST_REQUIRED","An approved purchase request assigned to you and in progress is required for quotations.");
  await validateServiceScope(db,actor,row);
  return {request:row,items:detail.items};
}
