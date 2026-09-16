import {personIdentityIds,currentPersonPrincipals} from "./iam-person";
import { hasWorkflowPermission } from "@/lib/permissions";
import type { SqlDatabase } from "./runtime/types";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import {
  expensePoolCandidates,
  expenseAuthoritySnapshot,
  legacyExpenseRoute,
} from "./expense-routing";
import {
  activeStaff,
  authorityGuard,
  requireCompanyAuthority,
  eligibleRoleUsers,
  notification,
  requestRoute,
  staffGuard,
  type ApprovalRoute,
  type RecordScope,
  type RouteStep,
  type StaffActor,
} from "./staff-policy";

export type ApprovalKind = "expense" | "leave" | "task";
export type ApprovalPhase = "request" | "start" | "completion";
export type ApprovalInstance = RecordScope & {
  id: string;
  record_type: ApprovalKind;
  record_id: string;
  phase: ApprovalPhase;
  cycle: number;
  requester_user_id: string;
  assignee_user_id: string | null;
  state: string;
  current_step: number;
  blocked_reason: string | null;
  version: number;
  created_at: number;
};
export type ApprovalStep = {
  id: string;
  instance_id: string;
  ordinal: number;
  approver_user_id: string;
  authority: RouteStep["authority"];
  state: string;
  approver_name?: string;
  approver_pool_json?: string | null;
  required_role?: string | null;
  stage?: string | null;
  route_snapshot_json?: string | null;
};
export type ApprovalDecision = {
  id: string;
  instance_id: string;
  step_id: string | null;
  action: string;
  actor_user_id: string;
  actor_name: string;
  reason: string;
  reviewed_version: number;
  created_at: number;
  previous_approver_user_id: string | null;
  approver_user_id: string | null;
  authority_json?: string | null;
};
export type ApprovalView = {
  instance: ApprovalInstance | null;
  steps: ApprovalStep[];
  history: ApprovalDecision[];
  currentApprover: ApprovalStep | null;
  blockedReason: string | null;
  canDecide: boolean;
  eligibleApproverIds?: string[];
};

export function participantSql(
  kind: ApprovalKind,
  alias: string,
  userParameter = "?",
  activeAssignment?: string,
) {
  const roleCheck=activeAssignment ? ` AND (ast.state IN ('approved','rejected') OR EXISTS(SELECT 1 FROM assignments aa WHERE aa.id=${activeAssignment} AND iam_role_allows(aa.role,aa.company_id,'workflow.'||COALESCE(ast.required_role,CASE WHEN ast.authority IN ('named','department') THEN 'people_manager' ELSE ast.authority END)) AND aa.company_id=ai.company_id AND (aa.scope_kind='holding' OR aa.scope_kind='branch' AND aa.scope_node_id=ai.branch_id OR aa.scope_kind='department' AND aa.scope_node_id=ai.department_id)))` : "";
  return `EXISTS(SELECT 1 FROM approval_instances ai JOIN approval_steps ast ON ast.instance_id=ai.id WHERE ai.record_type='${kind}' AND ai.record_id=${alias}.id AND ai.company_id=${alias}.company_id AND EXISTS(SELECT 1 FROM (SELECT ast.approver_user_id user_id UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant WHERE iam_same_person(participant.user_id,${userParameter},ai.company_id))${roleCheck})`;
}
export async function currentApproval(
  db: SqlDatabase,
  kind: ApprovalKind,
  id: string,
  companyId: string,
) {
  return db
    .prepare(
      "SELECT * FROM approval_instances WHERE record_type=? AND record_id=? AND company_id=? ORDER BY insertion_order DESC LIMIT 1",
    )
    .bind(kind, id, companyId)
    .first<ApprovalInstance>();
}
async function stepsFor(db: SqlDatabase, instance: ApprovalInstance) {
  return (
    await db
      .prepare(
        "SELECT s.*,COALESCE(p.legal_name,s.approver_user_id) approver_name FROM approval_steps s LEFT JOIN hr_people p ON p.user_id=s.approver_user_id AND p.company_id=? AND p.merged_into_id IS NULL WHERE s.instance_id=? ORDER BY s.ordinal",
      )
      .bind(instance.company_id, instance.id)
      .all<ApprovalStep>()
  ).results;
}
export async function taskRoute(
  db: SqlDatabase,
  scope: RecordScope,
  creator: string,
  assignee: string,
  phase: ApprovalPhase,
  users: string[],
): Promise<ApprovalRoute> {
  try {
    await activeStaff(db, scope.company_id, assignee);
    if (users.length > 20 || new Set(users).size !== users.length)
      throw new ApiError(
        422,
        "APPROVAL_STEPS_INVALID",
        "Choose up to twenty different ordered approvers per phase.",
      );
    for (const user of users) {
      if (user === assignee || (phase === "start" && user === creator))
        throw new ApiError(
          422,
          "SELF_APPROVAL",
          "Assignees cannot approve their work; creators cannot approve their own start request.",
        );
      await activeStaff(db, scope.company_id, user);
    }
    return {
      scope,
      steps: users.map((userId) => ({ userId, authority: "named" })),
      blockedReason: null,
    };
  } catch (error) {
    if (!(error instanceof ApiError)) throw error;
    return { scope, steps: [], blockedReason: error.message };
  }
}
export async function openApproval(
  db: SqlDatabase,
  key: string,
  kind: ApprovalKind,
  id: string,
  phase: ApprovalPhase,
  requester: string,
  assignee: string | null,
  route: ApprovalRoute,
  recordVersion: number,
) {
  if (route.blockedReason)
    throw new ApiError(409, "APPROVAL_ROUTE_BLOCKED", route.blockedReason);
  const previous = await db
    .prepare(
      "SELECT COALESCE(MAX(cycle),0) cycle FROM approval_instances WHERE record_type=? AND record_id=? AND phase=?",
    )
    .bind(kind, id, phase)
    .first<{ cycle: number }>();
  const instanceId = `approval-${key}`,
    cycle = (previous?.cycle ?? 0) + 1,
    now = Date.now();
  const statements = [
    ...(route.checks ?? []).map((check, i) =>
      assertion(db, key, `route-snapshot-${i}`, check.query, check.args),
    ),
    db
      .prepare(
        "UPDATE approval_instances SET state='superseded',version=version+1,updated_at=? WHERE record_type=? AND record_id=? AND state IN ('pending','blocked')",
      )
      .bind(now, kind, id),
    db
      .prepare(
        "INSERT INTO approval_instances(id,company_id,branch_id,department_id,record_type,record_id,phase,cycle,requester_user_id,assignee_user_id,state,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?, ?,?)",
      )
      .bind(
        instanceId,
        route.scope.company_id,
        route.scope.branch_id,
        route.scope.department_id,
        kind,
        id,
        phase,
        cycle,
        requester,
        assignee,
        route.steps.length ? "pending" : "approved",
        now,
        now,
      ),
    ...route.steps.flatMap((s, ordinal) => [
      staffGuard(
        db,
        key,
        route.scope.company_id,
        s.userId,
        `route-staff-${ordinal}`,
      ),
      ...(s.authority === "hr_admin" || s.authority === "holding_owner"
        ? [
            roleGuard(
              db,
              key,
              route.scope,
              { approver_user_id: s.userId, authority: s.authority },
              `route-role-${ordinal}`,
            ),
          ]
        : []),
      db
        .prepare(
          "INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority,approver_pool_json,required_role,stage,route_snapshot_json) VALUES(?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          `${instanceId}-${ordinal}`,
          instanceId,
          ordinal,
          s.userId,
          s.authority,
          s.candidateUserIds ? JSON.stringify(s.candidateUserIds) : null,
          s.requiredRole ?? null,
          s.stage ?? null,
          s.snapshot ? JSON.stringify(s.snapshot) : null,
        ),
    ]),
  ];
  if (route.steps[0])
    for (const userId of route.steps[0].candidateUserIds ?? [
      route.steps[0].userId,
    ])
      statements.push(
        notification(
          db,
          route.scope,
          userId,
          kind,
          id,
          recordVersion,
          `${phase === "start" ? "Start" : phase === "completion" ? "Completion" : "Request"} approval needs your decision`,
        ),
      );
  return { statements, instanceId };
}

/** Pending legacy requests receive routes only; existing historical decisions are never synthesized. */
export async function initializeLegacyApproval(
  db: SqlDatabase,
  instance: ApprovalInstance,
) {
  if (instance.record_type === "task" || instance.state !== "blocked")
    return instance;
  if ((await stepsFor(db, instance)).length) return instance;
  const expense =
    instance.record_type === "expense"
      ? await db
          .prepare(
            "SELECT created_by,prepared_by FROM finance_expenses WHERE id=? AND company_id=?",
          )
          .bind(instance.record_id, instance.company_id)
          .first<{ created_by: string; prepared_by: string | null }>()
      : null;
  const route = await requestRoute(
    db,
    instance.company_id,
    instance.requester_user_id,
    instance.record_type,
    expense
      ? [
          expense.created_by,
          ...(expense.prepared_by ? [expense.prepared_by] : []),
        ]
      : [],
  );
  if (route.blockedReason) {
    console.warn(
      JSON.stringify({
        event: "approval_route_blocked",
        recordType: instance.record_type,
        recordId: instance.record_id,
        companyId: instance.company_id,
        reason: route.blockedReason,
      }),
    );
    return { ...instance, blocked_reason: route.blockedReason };
  }
  const table =
      instance.record_type === "leave" ? "leave_requests" : "finance_expenses",
    stateColumn = instance.record_type === "leave" ? "state" : "status",
    pendingState =
      instance.record_type === "leave" ? "pending_approval" : "in_review";
  const record = await db
    .prepare(
      `SELECT version FROM ${table} WHERE id=? AND company_id=? AND ${stateColumn}=?`,
    )
    .bind(instance.record_id, instance.company_id, pendingState)
    .first<{ version: number }>();
  if (!record) return instance;
  const key = `initialize-${instance.id}`,
    now = Date.now();
  try {
    await db.batch([
      assertion(
        db,
        key,
        "legacy-route",
        "EXISTS(SELECT 1 FROM approval_instances WHERE id=? AND version=? AND state='blocked') AND NOT EXISTS(SELECT 1 FROM approval_steps WHERE instance_id=?)",
        [instance.id, instance.version, instance.id],
      ),
      assertion(
        db,
        key,
        "legacy-record",
        `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND ${stateColumn}=? AND version=?)`,
        [instance.record_id, instance.company_id, pendingState, record.version],
      ),
      staffGuard(
        db,
        key,
        instance.company_id,
        instance.requester_user_id,
        "legacy-requester",
      ),
      ...(route.checks ?? []).map((check, i) =>
        assertion(db, key, `legacy-snapshot-${i}`, check.query, check.args),
      ),
      ...route.steps.flatMap((s, ordinal) => [
        staffGuard(
          db,
          key,
          instance.company_id,
          s.userId,
          `eligible-${ordinal}`,
        ),
        ...(s.authority === "hr_admin" || s.authority === "holding_owner"
          ? [
              roleGuard(
                db,
                key,
                route.scope,
                { approver_user_id: s.userId, authority: s.authority },
                `legacy-role-${ordinal}`,
              ),
            ]
          : []),
        db
          .prepare(
            "INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority) VALUES(?,?,?,?,?)",
          )
          .bind(
            `${instance.id}-${ordinal}`,
            instance.id,
            ordinal,
            s.userId,
            s.authority,
          ),
      ]),
      db
        .prepare(
          "UPDATE approval_instances SET state='pending',branch_id=?,department_id=?,blocked_reason=NULL,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(
          route.scope.branch_id,
          route.scope.department_id,
          now,
          instance.id,
        ),
      db
        .prepare(
          `UPDATE ${table} SET branch_id=?,department_id=?,version=version+1,updated_at=? WHERE id=?`,
        )
        .bind(
          instance.record_type === "leave"
            ? (route.scope.branch_id ?? "")
            : route.scope.branch_id,
          route.scope.department_id,
          now,
          instance.record_id,
        ),
      db
        .prepare(
          "INSERT INTO approval_decisions(id,instance_id,action,actor_user_id,reason,reviewed_version,command_id,created_at) VALUES(?,?,'initialize','system','Initialize pending request under department routing',?,?,?)",
        )
        .bind(key, instance.id, record.version, key, now),
      db
        .prepare("INSERT INTO audit_events VALUES(?,?,?,?,?,?,?,?,?,?)")
        .bind(
          key,
          key,
          "system",
          "system",
          "approval.initialize",
          instance.record_type,
          instance.record_id,
          record.version + 1,
          "Initialize route; no historical approval inferred",
          now,
        ),
      db
        .prepare(
          "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?)",
        )
        .bind(
          key,
          key,
          `${instance.record_type}.approval.initialize`,
          instance.record_type,
          instance.record_id,
          record.version + 1,
          JSON.stringify({
            id: instance.record_id,
            state: pendingState,
            companyId: instance.company_id,
            branchId: route.scope.branch_id,
            departmentId: route.scope.department_id,
            version: record.version + 1,
          }),
          now,
        ),
      notification(
        db,
        route.scope,
        route.steps[0].userId,
        instance.record_type,
        instance.record_id,
        instance.version,
        "Request approval needs your decision",
      ),
      db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key),
    ]);
  } catch (error) {
    if (!/constraint|UNIQUE/i.test(String(error))) throw error;
  }
  return (
    (await currentApproval(
      db,
      instance.record_type,
      instance.record_id,
      instance.company_id,
    )) ?? instance
  );
}
export async function stepBlock(
  db: SqlDatabase,
  instance: ApprovalInstance,
  step: ApprovalStep,
  steps: ApprovalStep[],
) {
  if (step.approver_pool_json)
    return (await expensePoolCandidates(db, instance, step, steps)).length
      ? null
      : "No eligible independent reviewer remains. Resolve this approval in Administration.";
  try {
    const principalMap=await currentPersonPrincipals(db,instance.company_id,[step.approver_user_id,...steps.map(s=>s.approver_user_id),instance.requester_user_id,...(instance.assignee_user_id?[instance.assignee_user_id]:[])]);
    const principal=(id:string)=>principalMap.get(id)??id;
    await activeStaff(db, instance.company_id, step.approver_user_id);
    if (instance.assignee_user_id)
      await activeStaff(db, instance.company_id, instance.assignee_user_id);
    if (
      instance.assignee_user_id && principal(instance.assignee_user_id) === principal(step.approver_user_id) ||
      (instance.phase !== "completion" &&
        principal(instance.requester_user_id) === principal(step.approver_user_id))
    )
      return "This approval would be a self-approval.";
    if (step.authority === "hr_admin" || step.authority === "holding_owner") {
      if (
        !(await eligibleRoleUsers(db, instance, step.authority)).includes(
          principal(step.approver_user_id),
        )
      )
        return "The assigned approver no longer has the required company or HR authority.";
      if (
        step.authority === "hr_admin" &&
        steps.some(
          (s) =>
            s.ordinal < step.ordinal &&
            principal(s.approver_user_id) === principal(step.approver_user_id),
        )
      )
        return "The HR decision must be independent of the department decision.";
    }
    return null;
  } catch (error) {
    if (!(error instanceof ApiError)) throw error;
    return "The assigned approver is unavailable. An administrator must reroute with a reason.";
  }
}
function roleGuard(
  db: SqlDatabase,
  key: string,
  instance: RecordScope,
  step: Pick<ApprovalStep, "approver_user_id" | "authority">,
  label = "approval-authority",
) {
  const now = Date.now();
  return assertion(
    db,
    key,
    label,
    `EXISTS(SELECT 1 FROM assignments WHERE user_id=? AND company_id=? AND iam_role_allows(role,company_id,'workflow.'||?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND
    (scope_kind='holding' OR (?='hr_admin' AND ((scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?)))))`,
    [
      step.approver_user_id,
      instance.company_id,
      step.authority,
      now,
      now,
      step.authority,
      instance.branch_id,
      instance.department_id,
    ],
  );
}
export async function approvalView(
  db: SqlDatabase,
  kind: ApprovalKind,
  id: string,
  actor: Pick<StaffActor, "companyId" | "userId" | "role" | "assignmentId"> & {inspection?:true},
): Promise<ApprovalView> {
  let instance = await currentApproval(db, kind, id, actor.companyId);
  if (instance && !actor.inspection) instance = await initializeLegacyApproval(db, instance);
  const steps = instance ? await stepsFor(db, instance) : [];
  const currentApprover =
    instance && instance.state === "pending"
      ? (steps.find((s) => s.ordinal === instance.current_step) ?? null)
      : null;
  const poolCandidates =
    currentApprover?.approver_pool_json && instance
      ? await expensePoolCandidates(db, instance, currentApprover, steps)
      : null;
  const blockedReason =
    instance?.blocked_reason ??
    (currentApprover
      ? poolCandidates
        ? poolCandidates.length
          ? null
          : "No eligible independent reviewer remains. Resolve this approval in Administration."
        : await stepBlock(db, instance!, currentApprover, steps)
      : null);
  const activeScope = instance ? !!await db.prepare("SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR scope_kind='branch' AND scope_node_id=? OR scope_kind='department' AND scope_node_id=?)").bind(actor.assignmentId,actor.userId,actor.companyId,actor.role,Date.now(),Date.now(),instance.branch_id,instance.department_id).first() : false;
  const history = (
    await db
      .prepare(
        "SELECT d.*,COALESCE(p.legal_name,d.actor_user_id) actor_name FROM approval_decisions d JOIN approval_instances i ON i.id=d.instance_id LEFT JOIN hr_people p ON p.user_id=d.actor_user_id AND p.company_id=i.company_id AND p.merged_into_id IS NULL WHERE i.record_type=? AND i.record_id=? AND i.company_id=? ORDER BY d.created_at,d.insertion_order",
      )
      .bind(kind, id, actor.companyId)
      .all<ApprovalDecision>()
  ).results;
  const eligibleApproverIds = await personIdentityIds(db,actor.companyId,poolCandidates??(currentApprover?[currentApprover.approver_user_id]:[]));
  return {
    instance,
    steps,
    history,
    currentApprover,
    blockedReason,
    eligibleApproverIds,
    canDecide:
      !!currentApprover && activeScope &&
      !blockedReason &&
      eligibleApproverIds.includes(actor.userId) && (kind === "task" && currentApprover.authority === "named" && !currentApprover.required_role || hasWorkflowPermission(actor,currentApprover.required_role ?? (["named", "department"].includes(currentApprover.authority) ? "people_manager" : currentApprover.authority))),
  };
}
async function expenseDecisionEvidence(
  db: SqlDatabase,
  actor: StaffActor,
  instance: ApprovalInstance,
  step: ApprovalStep,
  eligible: string[],
) {
  const now = Date.now();
  const [assignments, receipts, person] = await Promise.all([
    db
      .prepare(
        "SELECT id,role,scope_kind,scope_node_id,valid_from,valid_until FROM assignments WHERE company_id=? AND id=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? ORDER BY id",
      )
      .bind(instance.company_id, actor.assignmentId, now, now)
      .all(),
    db
      .prepare(
        "SELECT id,revision,checksum_sha256,original_name FROM attachments WHERE owner_type='expense_claim' AND owner_id=? AND revoked_at IS NULL ORDER BY id",
      )
      .bind(instance.record_id)
      .all(),
    activeStaff(db, instance.company_id, actor.userId).catch((error) => {
      if (error instanceof ApiError) return null;
      throw error;
    }),
  ]);
  return JSON.stringify({
    actorId: actor.userId,
    companyId: instance.company_id,
    branchId: instance.branch_id,
    departmentId: instance.department_id,
    stage: step.stage,
    authority: step.authority,
    requiredRole: step.required_role,
    route: JSON.parse(step.route_snapshot_json ?? "{}"),
    eligiblePersonIds: eligible,
    activeAuthority: {role:actor.role,assignmentId:actor.assignmentId,...actor.authority},
    assignments: assignments.results,
    person,
    receipts: receipts.results,
  });
}
export async function prepareDecision(
  db: SqlDatabase,
  actor: StaffActor,
  key: string,
  kind: ApprovalKind,
  id: string,
  approve: boolean,
  reason: string,
  version: number,
) {
  if (!approve && !reason.trim())
    throw new ApiError(
      422,
      "REASON_REQUIRED",
      "Explain why this work is being rejected.",
    );
  const authoritySnapshot =
    kind === "expense"
      ? await expenseAuthoritySnapshot(db, actor.companyId)
      : null;
  const view = await approvalView(db, kind, id, actor),
    instance = view.instance,
    step = view.currentApprover;
  if (!instance || !step || !view.canDecide)
    throw new ApiError(
      403,
      "CURRENT_APPROVER_REQUIRED",
      view.blockedReason ??
        "Only the named approver of the current step can decide.",
    );
  const next = view.steps.find((s) => s.ordinal === step.ordinal + 1);
  const decisionEvidence = step.approver_pool_json
    ? await expenseDecisionEvidence(
        db,
        actor,
        instance,
        step,
        view.eligibleApproverIds ?? [],
      )
    : JSON.stringify({activeAuthority:{role:actor.role,assignmentId:actor.assignmentId,...actor.authority,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId}});
  const final = !approve || !next,
    now = Date.now();
  const statements = [
    ...(step.approver_pool_json && authoritySnapshot
      ? [authoritySnapshot.guard(key)]
      : []),
    assertion(
      db,
      key,
      "approval-version",
      "EXISTS(SELECT 1 FROM approval_instances WHERE id=? AND version=? AND state='pending' AND current_step=?) AND EXISTS(SELECT 1 FROM approval_steps WHERE id=? AND ? IN (SELECT approver_user_id UNION SELECT value FROM erpx_json_each(COALESCE(approver_pool_json,'[]'))) AND state='pending')",
      [instance.id, instance.version, step.ordinal, step.id, actor.userId],
    ),
    assertion(db, key, "active-decision-role", "EXISTS(SELECT 1 FROM assignments WHERE id=? AND iam_role_allows(role,company_id,'workflow.'||?))", [actor.assignmentId, step.required_role ?? (kind === "task" && step.authority === "named" ? actor.role : ["named", "department"].includes(step.authority) ? "people_manager" : step.authority)]),
    staffGuard(db, key, actor.companyId, actor.userId, "decision-staff"),
    ...(instance.assignee_user_id
      ? [
          staffGuard(
            db,
            key,
            actor.companyId,
            instance.assignee_user_id,
            "decision-assignee",
          ),
        ]
      : []),
    ...(!step.approver_pool_json &&
    (step.authority === "hr_admin" || step.authority === "holding_owner")
      ? [
          roleGuard(db, key, instance, {
            ...step,
            approver_user_id: actor.userId,
          }),
        ]
      : []),
    ...(step.approver_pool_json
      ? [
          assertion(
            db,
            key,
            "expense-separation",
            "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND NOT iam_same_person(claimant_user_id,?,company_id) AND NOT iam_same_person(prepared_by,?,company_id)) AND NOT EXISTS(SELECT 1 FROM approval_steps WHERE instance_id=? AND ordinal<? AND state='approved' AND iam_same_person(approver_user_id,?,(SELECT company_id FROM approval_instances WHERE id=instance_id)))",
            [
              id,
              actor.userId,
              actor.userId,
              instance.id,
              step.ordinal,
              actor.userId,
            ],
          ),
          ...(step.required_role
            ? [
                assertion(
                  db,
                  key,
                  "expense-role",
                  "EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND id=? AND iam_role_allows(role,company_id,'workflow.'||?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?)))",
                  [
                    instance.company_id,
                    actor.assignmentId,
                    step.required_role,
                    now,
                    now,
                    instance.branch_id,
                    instance.department_id,
                  ],
                ),
              ]
            : []),
        ]
      : []),
    db
      .prepare(
        "UPDATE approval_steps SET state=?,approver_user_id=? WHERE id=?",
      )
      .bind(approve ? "approved" : "rejected", actor.userId, step.id),
    db
      .prepare(
        "UPDATE approval_instances SET state=?,current_step=?,version=version+1,updated_at=? WHERE id=?",
      )
      .bind(
        !approve ? "rejected" : final ? "approved" : "pending",
        next && approve ? next.ordinal : step.ordinal,
        now,
        instance.id,
      ),
    db
      .prepare(
        "INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,reason,reviewed_version,command_id,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?,?,?)",
      )
      .bind(
        crypto.randomUUID(),
        instance.id,
        step.id,
        approve ? "approve" : "reject",
        actor.userId,
        reason,
        version,
        key,
        now,
        decisionEvidence,
      ),
    notification(
      db,
      instance,
      instance.requester_user_id,
      kind,
      id,
      version + 1,
      approve
        ? final
          ? "Approval complete"
          : "Step approved; awaiting the next decision"
        : "Rejected: correction required",
    ),
  ];
  if (approve && next)
    for (const userId of next.approver_pool_json
      ? await expensePoolCandidates(
          db,
          instance,
          next,
          view.steps.map((s) =>
            s.id === step.id
              ? { ...s, state: "approved", approver_user_id: actor.userId }
              : s,
          ),
        )
      : [next.approver_user_id])
      statements.push(
        notification(
          db,
          instance,
          userId,
          kind,
          id,
          version + 1,
          "Approval needs your decision",
        ),
      );
  return { statements, final, instance };
}
export function closeApprovals(
  db: SqlDatabase,
  key: string,
  kind: ApprovalKind,
  id: string,
  actorId: string,
  action: "cancel" | "revise",
  reason: string,
  version: number,
) {
  const now = Date.now();
  return [
    db
      .prepare(
        "INSERT INTO approval_decisions(id,instance_id,action,actor_user_id,reason,reviewed_version,command_id,created_at) SELECT ?||'-'||id,id,?,?,?,?,?,? FROM approval_instances WHERE record_type=? AND record_id=? AND state IN ('pending','blocked','approved')",
      )
      .bind(key, action, actorId, reason, version, key, now, kind, id),
    db
      .prepare(
        "UPDATE approval_instances SET state=?,version=version+1,updated_at=? WHERE record_type=? AND record_id=? AND state IN ('pending','blocked','approved')",
      )
      .bind(action === "cancel" ? "cancelled" : "superseded", now, kind, id),
  ];
}

export async function rerouteApproval(
  db: SqlDatabase,
  actor: StaffActor,
  id: string,
  input: {
    expectedVersion: number;
    instanceVersion: number;
    approverUserId: string;
    reason: string;
  },
  key: string,
) {
  await requireCompanyAuthority(db, actor);
  const authoritySnapshot = await expenseAuthoritySnapshot(db, actor.companyId);
  const instance = await db
    .prepare("SELECT * FROM approval_instances WHERE id=? AND company_id=?")
    .bind(id, actor.companyId)
    .first<ApprovalInstance>();
  if (!instance)
    throw new ApiError(
      409,
      "ROUTE_CONFIGURATION_REQUIRED",
      "Configure the request route before rerouting an active step.",
    );
  const operation = `${instance.record_type}.approval.reroute`,
    prior = await replayCommand(
      db,
      actor,
      key,
      operation,
      instance.record_id,
      input,
    );
  if (prior) return prior;
  if (instance.state !== "pending")
    throw new ApiError(
      409,
      "APPROVAL_NOT_PENDING",
      "Only a pending approval can be rerouted.",
    );
  const steps = await stepsFor(db, instance),
    current = steps.find((s) => s.ordinal === instance.current_step);
  if (!current)
    throw new ApiError(
      409,
      "APPROVAL_STEP_MISSING",
      "The current approval step is missing.",
    );
  if (
    instance.record_type === "leave" &&
    steps.some(
      (s) => s.id !== current.id && s.approver_user_id === input.approverUserId,
    )
  )
    throw new ApiError(
      422,
      "INDEPENDENT_HR_REQUIRED",
      "Department and HR decisions must remain independent.",
    );
  if (
    instance.record_type === "expense" &&
    (await db
      .prepare(
        "SELECT 1 FROM finance_expenses WHERE id=? AND (created_by=? OR prepared_by=?)",
      )
      .bind(instance.record_id, input.approverUserId, input.approverUserId)
      .first())
  )
    throw new ApiError(
      422,
      "SELF_APPROVAL",
      "An expense preparer cannot review their own preparation.",
    );
  const replacement = {
    ...current,
    approver_user_id: input.approverUserId,
    ...(current.approver_pool_json
      ? { approver_pool_json: JSON.stringify([input.approverUserId]) }
      : {}),
  };
  if (current.approver_pool_json && current.stage === "department_approval" && !JSON.parse(current.route_snapshot_json ?? "{}").spendingPolicy) {
    const claim = await db
      .prepare(
        "SELECT claimant_user_id,prepared_by FROM finance_expenses WHERE id=? AND company_id=?",
      )
      .bind(instance.record_id, instance.company_id)
      .first<{ claimant_user_id: string; prepared_by: string }>();
    if (claim) {
      const route = await legacyExpenseRoute(
        db,
        instance.company_id,
        claim.claimant_user_id,
        [claim.prepared_by],
      );
      if (route.steps[0]) {
        replacement.authority = route.steps[0].authority;
        replacement.route_snapshot_json = JSON.stringify(
          route.steps[0].snapshot ?? {},
        );
      }
    }
  }
  const blocked = await stepBlock(db, instance, replacement, steps);
  if (blocked) throw new ApiError(422, "APPROVER_INELIGIBLE", blocked);
  const table =
      instance.record_type === "leave"
        ? "leave_requests"
        : instance.record_type === "expense"
          ? "finance_expenses"
          : "staff_tasks",
    column = instance.record_type === "expense" ? "status" : "state";
  const state =
      instance.record_type === "leave"
        ? "pending_approval"
        : instance.record_type === "expense"
          ? "in_review"
          : instance.phase === "start"
            ? "start_approval"
            : "completion_approval",
    now = Date.now();
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    instance.record_id,
    input.expectedVersion + 1,
    {
      id: instance.record_id,
      instanceId: id,
      version: input.expectedVersion + 1,
    },
    [
      authorityGuard(db, key, actor),
      db.prepare("UPDATE flow_routes SET steps_json=jsonb_set(steps_json::jsonb,ARRAY[?::text,'primaryUserId'],to_jsonb(?::text),true)::text,version=version+1 WHERE company_id=? AND kind=? AND record_id=?")
        .bind(current.stage ?? (instance.record_type === "leave" ? `approval_${current.ordinal}` : `${instance.phase}_${current.ordinal}`),input.approverUserId,actor.companyId,instance.record_type === "task" ? "staff_task" : instance.record_type,instance.record_id),
      ...(current.approver_pool_json ? [authoritySnapshot.guard(key)] : []),
      staffGuard(db, key, actor.companyId, input.approverUserId),
      ...(!replacement.approver_pool_json &&
      (replacement.authority === "hr_admin" ||
        replacement.authority === "holding_owner")
        ? [roleGuard(db, key, instance, replacement)]
        : []),
      assertion(
        db,
        key,
        "route-version",
        `EXISTS(SELECT 1 FROM approval_instances WHERE id=? AND version=? AND state='pending' AND current_step=?) AND EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=? AND ${column}=?)`,
        [
          id,
          input.instanceVersion,
          instance.current_step,
          instance.record_id,
          actor.companyId,
          input.expectedVersion,
          state,
        ],
      ),
      db
        .prepare(
          "UPDATE approval_steps SET approver_user_id=?,approver_pool_json=?,authority=?,route_snapshot_json=? WHERE id=? AND state='pending'",
        )
        .bind(
          input.approverUserId,
          replacement.approver_pool_json ?? null,
          replacement.authority,
          replacement.route_snapshot_json ?? null,
          current.id,
        ),
      db
        .prepare(
          "UPDATE approval_instances SET blocked_reason=NULL,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, id),
      db
        .prepare(
          "INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,previous_approver_user_id,approver_user_id,reason,reviewed_version,command_id,created_at) VALUES(?,?,?,'reroute',?,?,?,?,?,?,?)",
        )
        .bind(
          crypto.randomUUID(),
          id,
          current.id,
          actor.userId,
          current.approver_user_id,
          input.approverUserId,
          input.reason,
          input.expectedVersion,
          key,
          now,
        ),
      db
        .prepare(
          `UPDATE ${table} SET version=version+1,updated_at=? WHERE id=?`,
        )
        .bind(now, instance.record_id),
      notification(
        db,
        instance,
        input.approverUserId,
        instance.record_type,
        instance.record_id,
        input.expectedVersion + 1,
        "Approval rerouted to you",
      ),
    ],
    {
      companyId: instance.company_id,
      branchId: instance.branch_id,
      departmentId: instance.department_id,
    },
  );
}
