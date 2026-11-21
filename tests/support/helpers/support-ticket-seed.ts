import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

/**
 * Seeds the Support ticket rows that the native Rust command owns. The retired
 * TypeScript ticket writer must not be used as a test oracle; native Rust PostgreSQL
 * suites own create/change behavior, while retained TypeScript readers can be tested
 * against these owner rows.
 */
export type SupportTicketSeedActor = {
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  userId: string;
};

export type SupportTicketSeedEvent = {
  action: string;
  reason?: string;
  actorId?: string;
  ownerId?: string | null;
  dueDate?: string | null;
  status?: string;
  createdAt?: number;
  evidence?: unknown;
};

export type SupportTicketSeedInput = {
  id?: string;
  title?: string;
  issue?: string;
  ownerId?: string | null;
  dueDate?: string | null;
  status?: "triage" | "open" | "in_progress" | "waiting_customer" | "resolved" | "closed" | "cancelled";
  version?: number;
  sourceKind?: string;
  sourceRecord?: unknown;
  orderId?: string | null;
  customerId?: string | null;
  createdBy?: string;
  createdAt?: number;
  updatedAt?: number;
  blockedReason?: string | null;
  resolution?: string | null;
  resolvedBy?: string | null;
  resolvedAt?: number | null;
  confirmationNoteId?: string | null;
  confirmationNoteVersion?: number | null;
  confirmedBy?: string | null;
  confirmedAt?: number | null;
  confirmationReason?: string | null;
  slaPolicyId?: string | null;
  slaPolicyVersion?: number | null;
  slaClock?: unknown;
  events?: SupportTicketSeedEvent[];
};

let counter = 0;
export function supportSeedId(prefix = "seed-ticket") {
  counter += 1;
  return `${prefix}-${Date.now()}-${counter}`;
}

export async function seedSupportTicket(
  db: SqlDatabase,
  actor: SupportTicketSeedActor,
  input: SupportTicketSeedInput = {},
) {
  const now = Date.now();
  const id = input.id ?? supportSeedId();
  const version = input.version ?? 1;
  const sourceKind = input.sourceKind ?? (input.orderId ? "sales_order" : "general");
  const order = input.orderId
    ? await db
        .prepare("SELECT party_id,branch_id,department_id FROM commerce_orders WHERE id=?")
        .bind(input.orderId)
        .first<{ party_id: string; branch_id: string | null; department_id: string | null }>()
    : null;
  if (input.orderId && !order) throw new Error("SUPPORT_SEED_ORDER_NOT_FOUND");
  const status = input.status ?? (input.orderId ? "open" : "triage");
  const ownerId = "ownerId" in input ? input.ownerId : status === "triage" || status === "cancelled" ? null : actor.userId;
  const dueDate = "dueDate" in input ? input.dueDate : ownerId ? "2026-10-20" : null;
  const createdAt = input.createdAt ?? now;
  const updatedAt = input.updatedAt ?? now;
  const branchId = order?.branch_id ?? actor.branchId;
  const departmentId = order?.department_id ?? actor.departmentId;
  const customerId = input.customerId ?? order?.party_id ?? null;
  const orderId = sourceKind === "sales_order" ? input.orderId ?? null : null;
  const sourceRecord = sourceKind === "general" || sourceKind === "sales_order" ? null : input.sourceRecord;
  if (sourceKind === "general" && (orderId || customerId || sourceRecord)) throw new Error("SUPPORT_SEED_GENERAL_SOURCE_INVALID");
  if (sourceKind === "sales_order" && (!orderId || !customerId || sourceRecord)) throw new Error("SUPPORT_SEED_ORDER_SOURCE_INVALID");
  if (!["general", "sales_order"].includes(sourceKind) && !sourceRecord) throw new Error("SUPPORT_SEED_TYPED_SOURCE_REQUIRED");
  await db
    .prepare(
      `INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,status,source_kind,created_by,created_at,updated_at,source_record_json,blocked_reason,resolution,resolved_by,resolved_at,confirmation_note_id,confirmation_note_version,confirmed_by,confirmed_at,confirmation_reason,sla_policy_id,sla_policy_version,sla_clock_json,version)
       VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`,
    )
    .bind(
      id,
      actor.companyId,
      branchId,
      departmentId,
      orderId,
      customerId,
      input.title ?? "Seeded support ticket",
      input.issue ?? "Seeded support issue",
      ownerId,
      dueDate,
      status,
      sourceKind,
      input.createdBy ?? actor.userId,
      createdAt,
      updatedAt,
      sourceRecord == null ? null : JSON.stringify(sourceRecord),
      input.blockedReason ?? null,
      input.resolution ?? (status === "resolved" || status === "closed" ? "Seeded resolution" : null),
      input.resolvedBy ?? (status === "resolved" || status === "closed" ? actor.userId : null),
      input.resolvedAt ?? (status === "resolved" || status === "closed" ? now : null),
      input.confirmationNoteId ?? null,
      input.confirmationNoteVersion ?? null,
      input.confirmedBy ?? (status === "closed" && sourceKind !== "sales_order" ? actor.userId : null),
      input.confirmedAt ?? (status === "closed" && sourceKind !== "sales_order" ? now : null),
      input.confirmationReason ?? (status === "closed" && sourceKind !== "sales_order" ? "Seeded independent confirmation" : null),
      input.slaPolicyId ?? null,
      input.slaPolicyVersion ?? null,
      input.slaClock === undefined ? null : JSON.stringify(input.slaClock),
      version,
    )
    .run();

  const events: SupportTicketSeedEvent[] = input.events ?? Array.from({ length: version }, (_, index) => ({
    action: index === 0 ? "report" : index === version - 1 ? "seed" : "advance",
    reason: index === 0 ? "Seeded original report" : "Seeded owner state",
    status: index === version - 1 ? status : "open",
  }));
  if (events.length !== version) throw new Error("SUPPORT_SEED_EVENT_VERSION_MISMATCH");
  const statements = events.map((event, index) =>
    db
      .prepare(
        "INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
      )
      .bind(
        `${id}-event-${index + 1}`,
        id,
        index + 1,
        event.action,
        event.actorId ?? actor.userId,
        event.reason ?? "Seeded event",
        event.ownerId === undefined ? ownerId : event.ownerId,
        event.dueDate === undefined ? dueDate : event.dueDate,
        event.status ?? status,
        event.createdAt ?? createdAt + index,
        JSON.stringify(event.evidence ?? {}),
      ),
  );
  if (statements.length) await db.batch(statements);
  const row = await db.prepare("SELECT * FROM support_tickets WHERE id=?").bind(id).first<Record<string, unknown>>();
  return { id, version, status, row: row! };
}

/**
 * Moves a seeded owner ticket to the state a retained test needs. This is owner-row
 * seeding for the native command, not a TypeScript command implementation: the
 * native Rust PostgreSQL suite owns transition validation.
 */
export async function seedSupportTicketState(
  db: SqlDatabase,
  id: string,
  input: {
    action: string;
    status: "triage" | "open" | "in_progress" | "waiting_customer" | "resolved" | "closed" | "cancelled";
    ownerId?: string | null;
    dueDate?: string | null;
    reason?: string;
    actorId?: string;
    resolution?: string | null;
    resolvedBy?: string | null;
    resolvedAt?: number | null;
    confirmationNoteId?: string | null;
    confirmationNoteVersion?: number | null;
    confirmedBy?: string | null;
    confirmedAt?: number | null;
    confirmationReason?: string | null;
    blockedReason?: string | null;
    slaClock?: unknown;
    now?: number;
  },
) {
  const current = await db.prepare("SELECT * FROM support_tickets WHERE id=?").bind(id).first<Record<string, unknown>>();
  if (!current) throw new Error("SUPPORT_SEED_TICKET_NOT_FOUND");
  const now = input.now ?? Date.now();
  const version = Number(current.version) + 1;
  const ownerId = input.ownerId === undefined ? (current.owner_id as string | null) : input.ownerId;
  const dueDate = input.dueDate === undefined ? (current.due_date as string | null) : input.dueDate;
  const resolved = input.status === "resolved" || input.status === "closed";
  await db
    .prepare(
      "UPDATE support_tickets SET status=?,owner_id=?,due_date=?,blocked_reason=?,resolution=?,resolved_by=?,resolved_at=?,confirmation_note_id=?,confirmation_note_version=?,confirmed_by=?,confirmed_at=?,confirmation_reason=?,sla_clock_json=coalesce(?::jsonb,sla_clock_json),version=?,updated_at=? WHERE id=?",
    )
    .bind(
      input.status,
      ownerId,
      dueDate,
      input.blockedReason ?? null,
      input.resolution ?? (resolved ? (current.resolution as string | null) ?? "Seeded resolution" : null),
      input.resolvedBy ?? (resolved ? (current.resolved_by as string | null) ?? input.actorId ?? "seed-actor" : null),
      input.resolvedAt ?? (resolved ? now : null),
      input.confirmationNoteId ?? (current.confirmation_note_id as string | null),
      input.confirmationNoteVersion ?? (current.confirmation_note_version as number | null),
      input.confirmedBy ?? (input.status === "closed" ? input.actorId ?? "seed-actor" : null),
      input.confirmedAt ?? (input.status === "closed" ? now : null),
      input.confirmationReason ?? (input.status === "closed" ? input.reason ?? "Seeded confirmation" : null),
      input.slaClock === undefined ? null : JSON.stringify(input.slaClock),
      version,
      now,
      id,
    )
    .run();
  await db
    .prepare(
      "INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(
      `${id}-event-${version}`,
      id,
      version,
      input.action,
      input.actorId ?? "seed-actor",
      input.reason ?? `Seeded ${input.action}`,
      ownerId,
      dueDate,
      input.status,
      now,
      JSON.stringify({ seeded: true }),
    )
    .run();
  return { id, version, status: input.status };
}
