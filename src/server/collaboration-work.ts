import "server-only";
import { createHash, randomUUID } from "node:crypto";
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
const sourceType = z.enum([
    "commerce_order",
    "support_ticket",
    "field_service_order",
    "staff_task",
  ]),
  reason = z.string().trim().min(3).max(3000),
  version = z.number().int().positive();
const relationSchema = z.object({
  sourceType,
  sourceId: z.string().min(1),
  targetType: sourceType,
  targetId: z.string().min(1),
  relation: z.enum(["related", "blocks", "duplicates", "depends_on"]),
  reason,
});
const cardSchema = z.object({
  sourceType,
  sourceId: z.string().min(1),
  title: z.string().trim().min(3).max(160),
  ownerId: z.string().min(1),
  dueAt: z.number().int().positive(),
  reason,
});
const actionSchema = z.discriminatedUnion("action", [
  z.object({
    action: z.literal("complete"),
    expectedVersion: version,
    evidence: z.string().trim().min(3).max(4000),
    reason,
  }),
  z.object({
    action: z.literal("reassign"),
    expectedVersion: version,
    ownerId: z.string().min(1),
    dueAt: z.number().int().positive(),
    reason,
  }),
  z.object({ action: z.literal("cancel"), expectedVersion: version, reason }),
]);
const csvCell = (value: unknown) => {
  const text = String(value ?? "");
  return `"${/^[=+@\-\t\r]/.test(text) ? "'" : ""}${text.replaceAll('"', '""')}"`;
};
type Source = {
  type: z.infer<typeof sourceType>;
  id: string;
  version: number;
  title: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  href: string;
};
type Card = Row & {
  id: string;
  version: number;
  status: string;
  owner_id: string;
  created_by: string;
  updated_at: number;
  due_at: number;
  owner_eligible: number;
};
function access(a: WorkContext, write = false) {
  requirePermission(a, write ? "records.write" : "records.read");
  if (!hasModuleAccess(a.modules, "collaboration"))
    throw new ApiError(
      403,
      "MODULE_FORBIDDEN",
      "Your current role cannot access Collaboration.",
    );
}
async function source(
  a: WorkContext,
  type: z.infer<typeof sourceType>,
  id: string,
): Promise<Source> {
  if (type === "commerce_order") {
    const r = await (await import("./commerce")).getOrder(a, id),
      x = r.order;
    return {
      type,
      id,
      version: Number(x.version),
      title: String(x.number),
      companyId: String(x.company_id),
      branchId: String(x.branch_id),
      departmentId: x.department_id ? String(x.department_id) : null,
      href: `/commerce/orders/${id}`,
    };
  }
  if (type === "support_ticket") {
    const x = await (await import("./support")).getSupportTicket(a, id);
    return {
      type,
      id,
      version: Number(x.version),
      title: String(x.title),
      companyId: String(x.company_id),
      branchId: String(x.branch_id),
      departmentId: x.department_id ? String(x.department_id) : null,
      href: `/support/${id}`,
    };
  }
  if (type === "field_service_order") {
    const x = (
      await (await import("./field-service")).fieldServiceDetail(a, id)
    ).order;
    return {
      type,
      id,
      version: Number(x.version),
      title: String(x.ticket_title),
      companyId: String(x.company_id),
      branchId: String(x.branch_id),
      departmentId: x.department_id ? String(x.department_id) : null,
      href: `/field-service/${id}`,
    };
  }
  const { write } = await holdingResources(a.holdingId),
    x = await (await import("./staff-tasks")).getTask(write, a, id);
  return {
    type,
    id,
    version: Number(x.version),
    title: String(x.title),
    companyId: String(x.company_id),
    branchId: x.branch_id ? String(x.branch_id) : null,
    departmentId: x.department_id ? String(x.department_id) : null,
    href: `/staff-tasks/${id}`,
  };
}
async function visibleSource(
  a: WorkContext,
  type: z.infer<typeof sourceType>,
  id: string,
) {
  try {
    return await source(a, type, id);
  } catch (e) {
    if (e instanceof ApiError && [403, 404].includes(e.status)) return null;
    throw e;
  }
}
const eligible = `EXISTS(SELECT 1 FROM assignments x JOIN hr_people p ON p.user_id=x.user_id AND p.company_id=x.company_id JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE x.company_id=c.company_id AND x.user_id=c.owner_id AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.write') AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=c.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=c.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active')`;
export async function collaborationWorkspace(a: WorkContext) {
  access(a);
  const { write } = await holdingResources(a.holdingId),
    relations = (
      await write
        .prepare(
          "SELECT * FROM collaboration_record_relations WHERE company_id=? AND status='active' ORDER BY updated_at DESC LIMIT 100",
        )
        .bind(a.companyId)
        .all<Row>()
    ).results,
    cards = (
      await write
        .prepare(
          `SELECT c.*,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible,(SELECT legal_name FROM hr_people WHERE company_id=c.company_id AND user_id=c.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name FROM collaboration_action_cards c WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?) ORDER BY c.updated_at DESC LIMIT 100`,
        )
        .bind(
          a.companyId,
          a.branchId,
          a.branchId,
          a.departmentId,
          a.departmentId,
        )
        .all<Card>()
    ).results,
    people = (
      await write
        .prepare(
          "SELECT DISTINCT p.user_id id,p.legal_name name FROM hr_people p JOIN assignments x ON x.user_id=p.user_id AND x.company_id=p.company_id WHERE p.company_id=? AND p.status='active' AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.write') ORDER BY p.legal_name",
        )
        .bind(a.companyId)
        .all<Row>()
    ).results;
  const visible = [];
  for (const r of relations) {
    const left = await visibleSource(
        a,
        sourceType.parse(r.source_type),
        String(r.source_id),
      ),
      right = await visibleSource(
        a,
        sourceType.parse(r.target_type),
        String(r.target_id),
      );
    if (left && right) {
      visible.push(r);
    }
  }
  return {
    relations: visible,
    // Action cards are independently scoped records. Their assigned owner may
    // need to complete the action even when the linked source requires a
    // different department permission.
    cards,
    people,
    sourceTypes: sourceType.options,
    canWrite: hasPermission(a, "records.write"),
    canReview: hasPermission(a, "records.review"),
  };
}
/** Export only records visible through the same source and workspace checks as the console. */
export async function collaborationExport(a: WorkContext) {
  const data = await collaborationWorkspace(a),
    rows: string[][] = [
      [
        "kind",
        "id",
        "status",
        "source_type",
        "source_id",
        "source_title",
        "target_type",
        "target_id",
        "target_title",
        "relation",
        "title",
        "owner_id",
        "owner_name",
        "due_at",
        "version",
        "reason",
      ],
    ];
  for (const relation of data.relations) {
    const left = await visibleSource(
        a,
        sourceType.parse(relation.source_type),
        String(relation.source_id),
      ),
      right = await visibleSource(
        a,
        sourceType.parse(relation.target_type),
        String(relation.target_id),
      );
    if (!left || !right) continue;
    rows.push([
      "relation",
      String(relation.id),
      String(relation.status),
      left.type,
      left.id,
      left.title,
      right.type,
      right.id,
      right.title,
      String(relation.relation),
      "",
      "",
      "",
      "",
      String(relation.version),
      String(relation.reason),
    ]);
  }
  for (const card of data.cards) {
    const linked = await visibleSource(
      a,
      sourceType.parse(card.source_type),
      String(card.source_id),
    );
    if (!linked) continue;
    rows.push([
      "action_card",
      String(card.id),
      String(card.status),
      linked.type,
      linked.id,
      linked.title,
      "",
      "",
      "",
      "",
      String(card.title),
      String(card.owner_id),
      String(card.owner_name ?? ""),
      String(card.due_at),
      String(card.version),
      "",
    ]);
  }
  const content = rows.map((row) => row.map(csvCell).join(",")).join("\r\n"),
    commandId = `collaboration-export-${randomUUID()}`,
    result = {
      commandId,
      format: "csv" as const,
      rows: rows.length - 1,
      content,
      sha256: createHash("sha256").update(content, "utf8").digest("hex"),
    };
  return executeCommand(
    (await holdingResources(a.holdingId)).write,
    a,
    commandId,
    "collaboration.export",
    { format: "csv" },
    commandId,
    1,
    result,
    [],
  );
}
export async function collaborationActionDetail(a: WorkContext, id: string) {
  const data = await collaborationWorkspace(a),
    card = data.cards.find((x) => x.id === id);
  if (!card)
    throw new ApiError(404, "ACTION_CARD_NOT_FOUND", "Action card not found.");
  const { write } = await holdingResources(a.holdingId);
  return {
    ...data,
    card,
    history: (
      await write
        .prepare(
          "SELECT * FROM collaboration_action_events WHERE card_id=? ORDER BY version",
        )
        .bind(id)
        .all<Row>()
    ).results,
  };
}
export async function createRecordRelation(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const input = relationSchema.parse(raw),
    left = await source(a, input.sourceType, input.sourceId),
    right = await source(a, input.targetType, input.targetId);
  if (left.companyId !== right.companyId || left.companyId !== a.companyId)
    throw new ApiError(
      422,
      "COLLABORATION_SCOPE_MISMATCH",
      "Both records must belong to the same Company.",
    );
  if (left.type === right.type && left.id === right.id)
    throw new ApiError(
      422,
      "COLLABORATION_SELF_RELATION",
      "Choose two different records.",
    );
  const symmetric = ["related", "duplicates"].includes(input.relation),
    swap = symmetric && `${left.type}:${left.id}` > `${right.type}:${right.id}`,
    from = swap ? right : left,
    to = swap ? left : right,
    { write } = await holdingResources(a.holdingId),
    id = `record-relation-${key}`,
    op = "collaboration.relation.create",
    prior = await replayCommand(write, a, key, op, id, input);
  if (prior) return prior;
  const now = Date.now();
  return executeCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    1,
    { id, version: 1, status: "active" },
    [
      write
        .prepare(
          "INSERT INTO collaboration_record_relations(id,company_id,branch_id,department_id,source_type,source_id,source_version,target_type,target_id,target_version,relation,reason,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          a.companyId,
          from.branchId,
          from.departmentId,
          from.type,
          from.id,
          from.version,
          to.type,
          to.id,
          to.version,
          input.relation,
          input.reason,
          a.userId,
          now,
          now,
        ),
      write
        .prepare(
          "INSERT INTO collaboration_relation_events VALUES(?,?,1,'create',? ,?,'active',?)",
        )
        .bind(`record-relation-event-${key}`, id, a.userId, input.reason, now),
    ],
  );
}
export async function removeRecordRelation(
  a: WorkContext,
  id: string,
  expectedVersion: number,
  why: string,
  key: string,
) {
  access(a, true);
  reason.parse(why);
  const { write } = await holdingResources(a.holdingId),
    op = "collaboration.relation.remove",
    input = { expectedVersion, reason: why },
    prior = await replayCommand(write, a, key, op, id, input);
  if (prior) return prior;
  const now = Date.now();
  return executeCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    expectedVersion + 1,
    { id, version: expectedVersion + 1, status: "removed" },
    [
      assertion(
        write,
        key,
        "relation",
        "EXISTS(SELECT 1 FROM collaboration_record_relations WHERE id=? AND company_id=? AND version=? AND status='active')",
        [id, a.companyId, expectedVersion],
      ),
      write
        .prepare(
          "UPDATE collaboration_record_relations SET status='removed',removed_by=?,removed_at=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(a.userId, now, now, id),
      write
        .prepare(
          "INSERT INTO collaboration_relation_events VALUES(?,?,?,'remove',?,?,'removed',?)",
        )
        .bind(
          `record-relation-event-${key}`,
          id,
          expectedVersion + 1,
          a.userId,
          why,
          now,
        ),
    ],
  );
}
export async function createActionCard(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const input = cardSchema.parse(raw),
    s = await source(a, input.sourceType, input.sourceId),
    { write } = await holdingResources(a.holdingId),
    id = `action-card-${key}`,
    op = "collaboration.action.create",
    prior = await replayCommand(write, a, key, op, id, input);
  if (prior) return prior;
  const now = Date.now();
  return executeCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    1,
    { id, version: 1, status: "open" },
    [
      write
        .prepare(
          "INSERT INTO collaboration_action_cards(id,company_id,branch_id,department_id,source_type,source_id,source_version,title,owner_id,due_at,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          a.companyId,
          s.branchId,
          s.departmentId,
          s.type,
          s.id,
          s.version,
          input.title,
          input.ownerId,
          input.dueAt,
          a.userId,
          now,
          now,
        ),
      assertion(
        write,
        key,
        "owner",
        `EXISTS(SELECT 1 FROM collaboration_action_cards c WHERE c.id=? AND ${eligible})`,
        [id],
      ),
      write
        .prepare(
          "INSERT INTO collaboration_action_events VALUES(?,?,1,'create',?,?, 'open',?,?,?)",
        )
        .bind(
          `action-card-event-${key}`,
          id,
          a.userId,
          input.reason,
          input.ownerId,
          input.dueAt,
          now,
        ),
      write
        .prepare(
          "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'collaboration_action',id,version,'New cross-record action card: review the linked source',updated_at FROM collaboration_action_cards WHERE id=?",
        )
        .bind(`action-card-notice-${key}`, id),
    ],
  );
}
export async function changeActionCard(
  a: WorkContext,
  id: string,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const input = actionSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    row = await write
      .prepare(
        `SELECT c.*,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible FROM collaboration_action_cards c WHERE c.id=? AND c.company_id=?`,
      )
      .bind(id, a.companyId)
      .first<Card>();
  if (!row)
    throw new ApiError(404, "ACTION_CARD_NOT_FOUND", "Action card not found.");
  const manage = hasPermission(a, "records.review"),
    own = row.owner_id === a.userId && Number(row.owner_eligible) > 0;
  if (input.action === "complete" && !own)
    throw new ApiError(
      403,
      "ACTION_CARD_OWNER_REQUIRED",
      "The assigned owner must complete this action.",
    );
  if (input.action !== "complete" && !manage)
    throw new ApiError(
      403,
      "ACTION_CARD_REVIEW_REQUIRED",
      "A reviewer must reassign or cancel this action.",
    );
  if (row.status !== "open")
    throw new ApiError(
      409,
      "ACTION_CARD_STATE_CHANGED",
      "Refresh the action card.",
    );
  const op = `collaboration.action.${input.action}`,
    prior = await replayCommand(write, a, key, op, id, input);
  if (prior) return prior;
  const now = Date.now(),
    status =
      input.action === "complete"
        ? "completed"
        : input.action === "cancel"
          ? "cancelled"
          : "open",
    owner = input.action === "reassign" ? input.ownerId : row.owner_id,
    due = input.action === "reassign" ? input.dueAt : row.due_at,
    evidence = input.action === "complete" ? input.evidence : null;
  return executeCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    input.expectedVersion + 1,
    { id, version: input.expectedVersion + 1, status },
    [
      assertion(
        write,
        key,
        "version",
        "EXISTS(SELECT 1 FROM collaboration_action_cards WHERE id=? AND version=? AND status='open')",
        [id, input.expectedVersion],
      ),
      write
        .prepare(
          "UPDATE collaboration_action_cards SET owner_id=?,due_at=?,status=?,completion_evidence=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(owner, due, status, evidence, now, id),
      ...(input.action === "reassign"
        ? [
            assertion(
              write,
              key,
              "owner",
              `EXISTS(SELECT 1 FROM collaboration_action_cards c WHERE c.id=? AND ${eligible})`,
              [id],
            ),
          ]
        : []),
      write
        .prepare(
          "INSERT INTO collaboration_action_events VALUES(?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          `action-card-event-${key}`,
          id,
          input.expectedVersion + 1,
          input.action,
          a.userId,
          input.reason,
          status,
          owner,
          due,
          now,
        ),
      write
        .prepare(
          "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'collaboration_action',id,version,'Cross-record action card updated: review the next action',updated_at FROM collaboration_action_cards WHERE id=?",
        )
        .bind(`action-card-notice-${key}`, id),
    ],
  );
}
export async function collaborationActionWork(
  a: WorkContext,
): Promise<WorkItem[]> {
  if (
    !hasPermission(a, "records.read") ||
    !hasModuleAccess(a.modules, "collaboration")
  )
    return [];
  const data = await collaborationWorkspace(a);
  return data.cards.map((c) => {
    const terminal = c.status !== "open",
      own = c.owner_id === a.userId && Number(c.owner_eligible) > 0,
      manage = hasPermission(a, "records.review");
    return {
      id: String(c.id),
      type: "collaboration_action" as const,
      title: String(c.title),
      detail: `${c.source_type} · ${c.source_id}`,
      href: "/collaboration",
      status: String(c.status),
      updatedAt: Number(c.updated_at),
      dueAt: Number(c.due_at),
      queue: terminal
        ? ("completed" as const)
        : own || manage
          ? ("todo" as const)
          : ("waiting" as const),
      canAct: !terminal && (own || manage),
      responsibleQueue: "Assigned action owner",
      nextAction: own
        ? "Complete with evidence"
        : "Review routing and source context",
      blocker: Number(c.owner_eligible)
        ? null
        : "Assigned owner is no longer eligible; reviewer reassignment is required",
      relationships: [
        own
          ? ("assigned" as const)
          : c.created_by === a.userId
            ? ("requested" as const)
            : ("team" as const),
      ],
    };
  });
}
