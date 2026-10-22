import { beforeAll, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import { getStaffWork } from "@erpx/server-work_projection/public/domain-work";
import { listTasks, taskDetail, type TaskDetail, type TaskRow } from "@erpx/server-operations/public/staff-tasks";
import { measureReads } from "@erpx/server-runtime/adapters/runtime/read-metrics";
import { withPolicyReadSnapshot } from "@erpx/server-identity_access/public/iam-read-snapshot";
import { mkdir, writeFile } from "node:fs/promises";

vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
}));
const db = env.H001_DB;
const actor = {
  holdingId: "h001", companyId: "work-read-model", userId: "work-reader",
  assignmentId: "work-reader-grant", role: "employee", branchId: null, departmentId: null,
  scope: { kind: "holding" },
} as WorkContext;

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  for (let index = 0; index < 12; index++) {
    const id = `work-read-${index}`;
    await db.prepare(`INSERT INTO staff_tasks(id,company_id,title,creator_user_id,assignee_user_id,
      state,created_at,updated_at) VALUES(?,?,?,?,'work-assignee','completed',1,?)`)
      .bind(id, actor.companyId, `Completed task ${index}`, actor.userId, index + 1).run();
    await db.prepare(`INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,
      media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at)
      VALUES(?,'staff_task',?,'evidence','evidence.txt','text/plain',4,?,'ready',?,2,1,1)`)
      .bind(`upload-${id}`, id, `staging-${id}`, actor.userId).run();
    await db.prepare(`INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,
      revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at)
      VALUES(?,?,'staff_task',?,'evidence',?,1,'evidence.txt','text/plain',4,'fixture',?,1)`)
      .bind(`attachment-${id}`, `upload-${id}`, id, `file-${id}`, actor.userId).run();
    await db.prepare(`INSERT INTO staff_task_delegations(id,company_id,task_id,delegator_user_id,
      delegate_user_id,starts_at,expires_at,reason,created_at,updated_at)
      VALUES(?,?,?,?, 'former-delegate',1,2,'Completed handoff',1,1)`)
      .bind(`delegation-${id}`, actor.companyId, id, actor.userId).run();
  }
});

/** Original Work projection, using the complete authorized task detail reader. */
function originalWorkRow(task: TaskRow, detail: TaskDetail): WorkItem {
  const actionable = detail.permittedActions.some(action =>
    ["edit", "submit", "start", "save-progress", "complete", "approve", "reject"].includes(action));
  return {
    id: task.id, type: "staff_task", dueAt: task.due_at, blocker: detail.blockedReason,
    responsibleQueue: detail.approval.currentApprover?.approver_name ?? task.assignee_name,
    relationships: [
      ...(task.creator_user_id === actor.userId ? ["requested" as const] : []),
      ...(task.assignee_user_id === actor.userId ? ["assigned" as const] : []),
      ...(detail.permittedActions.includes("approve") ? ["approval" as const] : []),
    ],
    title: task.title,
    detail: detail.blockedReason ? `Blocked: ${detail.blockedReason}`
      : detail.approval.currentApprover ? `Awaiting ${detail.approval.currentApprover.approver_name}`
        : `${task.assignee_name} · ${task.project_title ?? "Operational task"}`,
    href: `/staff-tasks/${task.id}`, status: task.state, updatedAt: task.updated_at,
    canAct: actionable, category: "operational", actionKind: detail.permittedActions.includes("approve") ? "approval" : "work",
    queue: actionable ? "todo" : ["completed", "cancelled"].includes(task.state) ? "completed" : "waiting",
  };
}

it("keeps complete Work task rows with at least half the original detail reads", async () => {
  const reference = await measureReads(() => withPolicyReadSnapshot(async () => {
    const rows: WorkItem[] = [];
    for (const task of await listTasks(db, actor))
      rows.push(originalWorkRow(task, await taskDetail(db, actor, task.id)));
    return rows.sort((a, b) => b.updatedAt - a.updatedAt || a.id.localeCompare(b.id));
  }));
  const rounds = process.env.ERPX_WORK_READ_BENCHMARK === "1" ? 20 : 1;
  const observations = [];
  for (let round = 0; round < rounds; round++) {
    const measured = await measureReads(() => withPolicyReadSnapshot(() => getStaffWork(actor)));
    expect(measured.result).toEqual(reference.result);
    expect(measured.metrics.queries).toBeLessThanOrEqual(reference.metrics.queries / 2);
    observations.push(measured.metrics);
  }
  if (process.env.ERPX_WORK_READ_BENCHMARK === "1") {
    const evidence = { event: "work_task_read_benchmark", fixtureItems: 12,
      reference: reference.metrics, observations };
    console.info(JSON.stringify(evidence));
    await mkdir(".scratch/dev", { recursive: true });
    await writeFile(`.scratch/dev/work-task-read-${process.env.ERPX_WORK_READ_VARIANT ?? "current"}.json`, JSON.stringify(evidence, null, 2));
  }
});
