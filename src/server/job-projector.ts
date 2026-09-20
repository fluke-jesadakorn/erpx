import { drainSocialAgent } from "./social-agent";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { drainRequestAi } from "./request-ai";
import {
  registeredResources,
  registeredDatabases,
  resourceSlots,
  type RegistryEnv,
} from "../lib/holding-registry";
import { drainExpenseAi } from "./expense-ai";
import type { BetaEnvironment } from "./beta-config";
export type EventMessage = {
  holdingId: string;
  id: string;
  eventType: string;
  aggregateId: string;
  aggregateVersion: number;
  payload: {
    id: string;
    state?: string;
    version: number;
    companyId: string;
    branchId: string | null;
    departmentId: string | null;
  };
};
export type JobEnv = RegistryEnv &
  BetaEnvironment & {
    [binding: string]: unknown;
    CONTROL_DB?: SqlDatabase;
  };

const databases = registeredDatabases;

export async function project(env: JobEnv, message: EventMessage) {
  if (!resourceSlots(env).some((s) => s.id === message.holdingId))
    throw new Error("INVALID_HOLDING");
  const { read, write } = databases(env, message.holdingId);
  if (message.eventType === "legal.ai.queue") await (await import("./legal/jobs")).drainLegalAi(write,env);
  if (message.eventType === "executive.generation.queue") await (await import("./executive/generation")).drainExecutiveGeneration(write,env);
  if (["social.post.publish", "social.post.comment", "social.agent.retry"].includes(message.eventType)) await drainSocialAgent(write, env, message.aggregateId);
  const isLeave = message.eventType.startsWith("leave.") && !message.eventType.startsWith("leave.policy.");
  const isOvertime = message.eventType.startsWith("overtime.") && message.eventType !== "overtime.eligibility";
  const isPayroll = message.eventType.startsWith("people.payroll.");
  const isTask =
    message.eventType.startsWith("task.") ||
    message.eventType === "routine.run";
  if (message.eventType === "expense.ai.prepare")
    await drainExpenseAi(
      write,
      registeredResources(env, message.holdingId).files,
      env,
      message.aggregateId,
    );
  if (message.eventType === "request.ai.prepare") await drainRequestAi(write, env, message.aggregateId);
  let payload = message.payload,
    version = message.aggregateVersion;
  if (isTask || ((isLeave || isPayroll || isOvertime) && !payload.state)) {
    // Backward compatibility for committed events written before state was included.
    const owner = await write
      .prepare(
        isTask
          ? "SELECT state,version,company_id,branch_id,department_id FROM staff_tasks WHERE id=?"
          : isOvertime
            ? "SELECT state,version,company_id,branch_id,department_id FROM overtime_requests WHERE id=?"
          : isLeave
            ? "SELECT state,version,company_id,branch_id,department_id FROM leave_requests WHERE id=?"
            : "SELECT status state,version,company_id,NULL branch_id,NULL department_id FROM hr_payroll_runs WHERE id=?",
      )
      .bind(message.aggregateId)
      .first<{
        state: string;
        version: number;
        company_id: string;
        branch_id: string | null;
        department_id: string | null;
      }>();
    if (!owner) throw new Error("PROJECTION_SOURCE_UNAVAILABLE");
    payload = {
      ...payload,
      state: owner.state,
      version: owner.version,
      companyId: owner.company_id,
      branchId: owner.branch_id,
      departmentId: owner.department_id,
    };
    version = owner.version;
  }
  const statements: SqlStatement[] = [
    read
      .prepare(
        "INSERT INTO projection_receipts(event_id,aggregate_id,aggregate_version,applied_at) VALUES(?,?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(
        message.id,
        message.aggregateId,
        message.aggregateVersion,
        Date.now(),
      ),
  ];
  if (isLeave || isPayroll || isTask || isOvertime) {
    const completed = [
      "approved",
      "posted",
      "rejected",
      "cancelled",
      "completed",
      "withdrawn",
    ].includes(payload.state ?? "");
    const queue = completed
      ? "completed"
      : payload.state === "pending_approval" || payload.state === "preview"
        ? "todo"
        : "waiting";
    const sourceType = isOvertime ? "overtime_request" : isTask
      ? "staff_task"
      : isPayroll
        ? "hr_payroll_run"
        : "leave_request";
    const title = isOvertime ? "Overtime request" : isTask
      ? "Staff task"
      : isPayroll
        ? "Payroll preview"
        : "Leave request";
    statements.push(
      read
        .prepare(
          `INSERT INTO work_items(id,source_type,source_id,source_version,title,subtitle,queue,company_id,branch_id,department_id,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)
      ON CONFLICT(source_type,source_id) DO UPDATE SET source_version=excluded.source_version,subtitle=excluded.subtitle,
        queue=excluded.queue,company_id=excluded.company_id,branch_id=excluded.branch_id,department_id=excluded.department_id,updated_at=excluded.updated_at
      WHERE work_items.source_version<excluded.source_version`,
        )
        .bind(
          `work_${message.aggregateId}`,
          sourceType,
          message.aggregateId,
          version,
          title,
          payload.state,
          queue,
          payload.companyId,
          payload.branchId,
          payload.departmentId,
          Date.now(),
        ),
    );
  }
  statements.push(
    read
      .prepare(
        `INSERT INTO projection_checkpoints(projector,last_event_id,last_event_at,updated_at) VALUES('work-items',?,?,?)
      ON CONFLICT(projector) DO UPDATE SET last_event_id=excluded.last_event_id,last_event_at=excluded.last_event_at,updated_at=excluded.updated_at`,
      )
      .bind(message.id, Date.now(), Date.now()),
  );
  await read.batch(statements);
}

