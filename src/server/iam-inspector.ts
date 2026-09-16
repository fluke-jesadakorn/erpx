import "server-only";
import { z } from "zod";
import { flowKinds } from "@/lib/flow";
import type { AccessDecision } from "@/lib/iam";
import type { WorkContext } from "./authorization";
import { iamOverview } from "./iam-service";
import type { IamActor } from "./iam-context";
import type { PostgresDatabase } from "./runtime/postgres";
import { iamDb } from "./iam-db";
import { ApiError } from "./http";
import { requireFlowAccess } from "./flow-access";
import { resolveFlow } from "./flow-routing";
import { flowRecord } from "./flow-routing";
import { samePerson } from "@/lib/iam";
import type { FlowKind } from "@/lib/flow";

const recordPermissions: Record<FlowKind, readonly string[]> = {
  expense: [
    "self.requests",
    "expense.read",
    "expense.prepare",
    "expense.approve",
    "expense.pay",
    "spending.approve",
  ],
  leave: ["self.requests", "people.read", "people.manage"],
  staff_task: ["self.tasks", "people.read", "people.manage"],
  request: ["self.requests", "spending.approve"],
  stock_count: ["inventory.read", "inventory.prepare", "inventory.approve"],
  transfer: ["inventory.read", "inventory.transfer"],
  order: [
    "commerce.read",
    "commerce.sales",
    "commerce.purchase",
    "commerce.approve",
  ],
  invoice: ["finance.read", "finance.write", "finance.approve", "finance.pay"],
  journal: ["finance.read", "finance.write", "finance.approve"],
  payroll: ["payroll.read", "payroll.prepare", "payroll.approve"],
  record: ["records.read", "records.write"],
};

export const inspectorInput = z.object({
  userId: z.string(),
  assignmentId: z.string(),
  permission: z.string(),
  workspaceId: z.string().optional(),
  recordType: z.enum(flowKinds).optional(),
  recordId: z.string().optional(),
});
/** Inspects domain policies without a session, context selection, command, or record payload. */
export async function inspectIamAccess(
  raw: PostgresDatabase,
  admin: IamActor,
  input: z.infer<typeof inspectorInput>,
) {
  const db = iamDb(raw),
    data = await iamOverview(db, admin);
  const assignment = data.assignments.find(
    (a) => a.id === input.assignmentId && a.user_id === input.userId,
  );
  if (!assignment)
    throw new ApiError(
      404,
      "ASSIGNMENT_NOT_FOUND",
      "Choose an assignment within your administration scope.",
    );
  const source = data.decisions
    .find((d) => d.assignmentId === input.assignmentId)
    ?.decisions.find((d) => d.permission === input.permission);
  if (!source)
    throw new ApiError(
      422,
      "PERMISSION_UNAVAILABLE",
      "Choose a catalog action.",
    );
  const decision: AccessDecision = {
    ...source,
    reasons: [...source.reasons],
    conditions: [...source.conditions],
  };
  const result = () => ({
    ...decision,
    executionAuthorized: false,
    recordId: input.recordId ?? null,
    recordType: input.recordType ?? null,
    workspaceId:
      input.workspaceId ?? assignment.scope_node_id ?? admin.companyId,
  });
  if (["denied", "unavailable"].includes(decision.result)) return result();
  let scopeKind = String(assignment.scope_kind),
    branchId = assignment.branch_id as string | null,
    departmentId =
      scopeKind === "department" ? String(assignment.scope_node_id) : null;
  if (input.workspaceId && input.workspaceId !== admin.companyId) {
    const node = data.nodes.find((n) => n.id === input.workspaceId);
    if (
      !node ||
      (assignment.scope_kind === "department" &&
        assignment.scope_node_id !== node.id) ||
      (assignment.scope_kind === "branch" &&
        assignment.scope_node_id !== node.id &&
        assignment.scope_node_id !== node.branch_id)
    ) {
      decision.result = "denied";
      decision.reasons.push(
        "This assignment does not cover the selected workspace.",
      );
      return result();
    }
    scopeKind = String(node.kind);
    departmentId = scopeKind === "department" ? String(node.id) : null;
    branchId =
      scopeKind === "branch"
        ? String(node.id)
        : (node.branch_id as string | null);
  } else if (
    input.workspaceId === admin.companyId &&
    assignment.scope_kind !== "holding" &&
    !admin.platform
  ) {
    decision.result = "denied";
    decision.reasons.push(
      "A branch or department assignment cannot act at company scope.",
    );
    return result();
  }
  if (
    (departmentId && input.permission.startsWith("inventory.")) ||
    (branchId && input.permission === "inventory.approve")
  ) {
    decision.result = "denied";
    decision.reasons.push(
      "Inventory policy requires branch or company scope; adjustment approval requires company scope.",
    );
    return result();
  }
  if (!input.recordId) {
    if (input.recordType)
      decision.conditions.push(
        "Choose a record to evaluate participation and workflow state.",
      );
    return result();
  }
  if (!input.recordType || admin.platform) {
    decision.result = "conditional";
    decision.conditions.push(
      "Select a supported customer record type to inspect its policy.",
    );
    return result();
  }
  if (!recordPermissions[input.recordType].includes(input.permission)) {
    decision.result = "unavailable";
    decision.reasons.push(
      "This action does not apply to the selected record type.",
    );
    return result();
  }
  const role = data.roles.find((r) => r.id === assignment.role)!;
  const person = (
    await db
      .prepare(
        "SELECT person_id FROM iam_members WHERE company_id=? AND user_id=?",
      )
      .bind(admin.companyId, input.userId)
      .first<{ person_id: string | null }>()
  )?.person_id;
  const aliases = person
    ? (
        await db
          .prepare(
            "SELECT user_id FROM iam_members WHERE company_id=? AND person_id=?",
          )
          .bind(admin.companyId, person)
          .all<{ user_id: string }>()
      ).results.map((r) => r.user_id)
    : [];
  // Deliberately omit authority/session: this object can only reach read policy helpers.
  const target = {
    inspection: true,
    userId: input.userId,
    name: "",
    email: "",
    role: role.id,
    assignmentId: input.assignmentId,
    companyId: admin.companyId,
    holdingId: admin.holdingId!,
    holdingSlug: "",
    holdingName: "",
    branchId,
    departmentId,
    assignmentScope: assignment.scope_kind,
    scope: {
      kind: scopeKind,
      companyId: admin.companyId,
      branchId,
      departmentId,
      holdingId: admin.holdingId,
      holdingSlug: "",
      holdingName: "",
    },
    inherited: scopeKind !== assignment.scope_kind,
    modules: role.permissions
      .filter((p) => p.startsWith("module."))
      .map((p) => p.split(".")[1]),
    assignments: [],
    workspaces: [],
    iam: {
      permissions: role.permissions,
      grantablePermissions: [],
      roleRevision: role.revision,
      identityVersion: Number(assignment.identity_version ?? 0),
      personId: person ?? null,
      personUserIds: [input.userId, ...aliases],
      principalId: input.userId,
      freshUntil: null,
    },
  } as WorkContext;
  try {
    await requireFlowAccess(target, input.recordType, input.recordId);
    if (input.permission.startsWith("self.")) {
      const row = await flowRecord(
        raw,
        admin.companyId,
        input.recordType,
        input.recordId,
      );
      const participant = [
        row.requester_user_id,
        row.claimant_user_id,
        row.creator_user_id,
        row.created_by,
        ...(input.recordType === "staff_task" ? [row.assignee_user_id] : []),
      ].some((id) => typeof id === "string" && samePerson(target, id));
      if (!participant) {
        decision.result = "denied";
        decision.reasons.push(
          "This person is not an owner or assigned participant of this record.",
        );
        return result();
      }
    }
    decision.reasons.push(
      "The current record read policy permits this person in the selected workspace.",
    );
    if (
      input.permission.startsWith("self.") ||
      /\.read$/.test(input.permission)
    ) {
      decision.result = "allowed";
      decision.conditions = [];
    } else {
      const flow = await resolveFlow(
        raw,
        target,
        input.recordType,
        input.recordId,
      );
      decision.reasons.push(
        flow.canAct
          ? "The person is eligible for the current workflow step."
          : "The person cannot execute the current workflow step.",
      );
      if (flow.blocker) decision.reasons.push(flow.blocker);
      // A permission can cover multiple commands. Readiness, amounts and command inputs
      // remain mandatory and cannot be inferred from a record identifier alone.
      decision.result = "conditional";
      decision.conditions.push(
        "The concrete command must pass its transaction, input, amount and current-record checks.",
      );
    }
  } catch (error) {
    if (!(error instanceof ApiError)) throw error;
    decision.result = "denied";
    decision.reasons.push(
      "The record is unavailable under the selected person, assignment and workspace policy.",
    );
  }
  return result();
}
