/**
 * Native-seeding oracle for retained CRM activity integration reads.
 *
 * The TypeScript `createCRMActivity` / `changeCRMActivity` writers were retired
 * with the native CRM activity command (`packages/server-crm/src/adapters/
 * activities_native.rs` + composition `CrmActivityCommandService`). This helper
 * is test-only. It writes the same `crm_lead_activities` and immutable
 * `crm_lead_activity_events` rows as the native owner adapter so the retained
 * workspace/Work/history assertions remain meaningful.
 *
 * It intentionally does not emulate Workflow receipts, purchase enforcement,
 * Account session resolution or Sales-recipient eligibility; those native
 * command paths are covered by the Rust owner tests. Do not import this module
 * outside tests.
 */
import type {SqlDatabase,TransactionalSqlDatabase} from "@erpx/server-runtime/public/runtime/types";

type ActivityRow = Readonly<{
  id: string;
  version: number;
  status: string;
  owner_id: string;
  due_date: string;
  outcome: string | null;
}>;

const failure = (code: string, status: number): never => {
  throw Object.assign(new Error(code), {code, status});
};

const current = async (
  tx: SqlDatabase,
  id: string,
  companyId: string,
  branchId: string | null,
  departmentId: string | null,
): Promise<ActivityRow | null> =>
  tx
    .prepare(
      "SELECT x.id,x.version,x.status,x.owner_id,x.due_date,x.outcome FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id AND l.company_id=x.company_id WHERE x.id=? AND x.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?)",
    )
    .bind(id, companyId, branchId, branchId, departmentId, departmentId)
    .first<ActivityRow>();

export type NativeActivitySeed = Readonly<{
  key: string;
  leadId: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  actorId: string;
  ownerId: string;
  activityType: string;
  subject: string;
  details: string;
  dueDate: string;
  expectedVersion: number;
  now?: number;
}>;

/** Insert one native activity + create event, replaying the deterministic id. */
export async function seedNativeActivity(
  db: TransactionalSqlDatabase,
  input: NativeActivitySeed,
): Promise<ActivityRow> {
  const id = `crm-activity-${input.key}`;
  return db.transaction(async (tx) => {
    const prior = await current(
      tx,
      id,
      input.companyId,
      input.branchId,
      input.departmentId,
    );
    if (prior) return prior;
    const lead = await tx
      .prepare(
        "SELECT l.version,l.status,v.stage FROM crm_leads l LEFT JOIN crm_opportunities v ON v.lead_id=l.id WHERE l.id=? AND l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?)",
      )
      .bind(
        input.leadId,
        input.companyId,
        input.branchId,
        input.branchId,
        input.departmentId,
        input.departmentId,
      )
      .first<{version: number; status: string; stage: string | null}>();
    if (!lead) return failure("CRM_LEAD_NOT_FOUND", 404);
    if (!["open", "qualified"].includes(lead.status) || ["won", "lost"].includes(lead.stage ?? ""))
      return failure("CRM_ACTIVITY_SOURCE_CLOSED", 409);
    if (Number(lead.version) !== input.expectedVersion) return failure("CONFLICT", 409);
    const now = input.now ?? Date.now();
    const subject = input.subject.trim();
    const details = input.details.trim();
    await tx
      .prepare(
        "INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
      )
      .bind(
        id,
        input.companyId,
        input.leadId,
        input.activityType,
        subject,
        details,
        input.ownerId,
        input.dueDate,
        input.actorId,
        now,
        now,
      )
      .run();
    await tx
      .prepare(
        "INSERT INTO crm_lead_activity_events(id,activity_id,version,action,actor_id,owner_id,due_date,note,snapshot_json,created_at) VALUES(?,?,1,'create',?,?,?,?,?,?)",
      )
      .bind(
        `crm-activity-event-${input.key}`,
        id,
        input.actorId,
        input.ownerId,
        input.dueDate,
        details,
        JSON.stringify({
          leadId: input.leadId,
          expectedVersion: input.expectedVersion,
          activityType: input.activityType,
          subject,
          details,
          ownerId: input.ownerId,
          dueDate: input.dueDate,
        }),
        now,
      )
      .run();
    const row = await current(tx, id, input.companyId, input.branchId, input.departmentId);
    if (!row) return failure("CRM_ACTIVITY_NOT_FOUND", 404);
    return row;
  });
}

export type NativeActivityChange = Readonly<{
  key: string;
  id: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  actorId: string;
  action: "complete" | "cancel" | "reassign";
  expectedVersion: number;
  reason: string;
  manager?: boolean;
  ownerId?: string;
  dueDate?: string;
  now?: number;
}>;

/** Apply one native activity event (complete/cancel/reassign) with replay. */
export async function changeNativeActivity(
  db: TransactionalSqlDatabase,
  input: NativeActivityChange,
): Promise<ActivityRow> {
  return db.transaction(async (tx) => {
    const record = await current(
      tx,
      input.id,
      input.companyId,
      input.branchId,
      input.departmentId,
    );
    if (!record) return failure("CRM_ACTIVITY_NOT_FOUND", 404);
    const eventId = `crm-activity-event-${input.key}`;
    const prior = await tx
      .prepare("SELECT note FROM crm_lead_activity_events WHERE id=?")
      .bind(eventId)
      .first<{note: string}>();
    if (prior) {
      if (prior.note !== input.reason) return failure("IDEMPOTENCY_CONFLICT", 409);
      return record;
    }
    if (Number(record.version) !== input.expectedVersion) return failure("CONFLICT", 409);
    if (record.status !== "open") return failure("CRM_ACTIVITY_STATE_CHANGED", 409);
    if (input.action === "complete" && record.owner_id !== input.actorId)
      return failure("CRM_ACTIVITY_OWNER_REQUIRED", 403);
    if (input.action === "cancel" && !input.manager && record.owner_id !== input.actorId)
      return failure("CRM_ACTIVITY_OWNER_REQUIRED", 403);
    if (input.action === "reassign" && !input.manager)
      return failure("CRM_ACTIVITY_OWNER_REQUIRED", 403);
    const now = input.now ?? Date.now();
    const version = input.expectedVersion + 1;
    const owner = input.action === "reassign" ? input.ownerId : record.owner_id;
    const due = input.action === "reassign" ? input.dueDate : record.due_date;
    if (!owner || !due) return failure("VALIDATION_FAILED", 422);
    const status =
      input.action === "complete"
        ? "completed"
        : input.action === "cancel"
          ? "cancelled"
          : "open";
    const outcome = input.action === "reassign" ? null : input.reason.trim();
    await tx
      .prepare(
        "UPDATE crm_lead_activities SET owner_id=?,due_date=?,status=?,outcome=?,version=version+1,updated_at=? WHERE id=? AND version=? AND status='open'",
      )
      .bind(owner, due, status, outcome, now, input.id, input.expectedVersion)
      .run();
    await tx
      .prepare(
        "INSERT INTO crm_lead_activity_events(id,activity_id,version,action,actor_id,owner_id,due_date,note,snapshot_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)",
      )
      .bind(
        eventId,
        input.id,
        version,
        input.action,
        input.actorId,
        owner,
        due,
        input.action === "reassign" ? input.reason.trim() : outcome,
        JSON.stringify(
          input.action === "reassign"
            ? {
                action: input.action,
                expectedVersion: input.expectedVersion,
                ownerId: owner,
                dueDate: due,
                reason: input.reason.trim(),
              }
            : {action: input.action, expectedVersion: input.expectedVersion, reason: outcome},
        ),
        now,
      )
      .run();
    const row = await current(
      tx,
      input.id,
      input.companyId,
      input.branchId,
      input.departmentId,
    );
    if (!row) return failure("CRM_ACTIVITY_NOT_FOUND", 404);
    return row;
  });
}
