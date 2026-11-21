import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

/**
 * Seeds the Support incident rows that the native Rust command owns. The retired
 * TypeScript incident writer must not be used as a test oracle; the native Rust
 * PostgreSQL suite owns create/change/link/close behavior, while retained
 * TypeScript readers can be tested against these owner rows.
 */
export type SupportIncidentSeedActor = {
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  userId: string;
};

export type SupportIncidentSeedState =
  | "open"
  | "investigating"
  | "mitigated"
  | "closed"
  | "cancelled";

export type SupportIncidentSeedTicket = {
  ticketId: string;
  linkedVersion?: number;
  linkedBy?: string;
  linkedAt?: number;
  active?: boolean;
};

export type SupportIncidentSeedEvent = {
  action: string;
  reason?: string;
  actorId?: string;
  version?: number;
  status?: SupportIncidentSeedState;
  createdAt?: number;
  evidence?: unknown;
};

export type SupportIncidentSeedInput = {
  id?: string;
  title?: string;
  impact?: string;
  severity?: "critical" | "high" | "normal";
  ownerId?: string;
  dueDate?: string;
  status?: SupportIncidentSeedState;
  rootCause?: string | null;
  mitigation?: string | null;
  mitigatedBy?: string | null;
  version?: number;
  branchId?: string;
  departmentId?: string | null;
  createdBy?: string;
  createdAt?: number;
  updatedAt?: number;
  tickets?: SupportIncidentSeedTicket[];
  events?: SupportIncidentSeedEvent[];
};

const defaultActions: Record<SupportIncidentSeedState, string[]> = {
  open: ["create"],
  investigating: ["create", "start"],
  mitigated: ["create", "start", "mitigate"],
  closed: ["create", "start", "mitigate", "close"],
  cancelled: ["create", "cancel"],
};

let counter = 0;
export function supportIncidentSeedId(prefix = "seed-incident") {
  counter += 1;
  return `${prefix}-${Date.now()}-${counter}`;
}

async function ticketSource(
  db: SqlDatabase,
  companyId: string,
  link: SupportIncidentSeedTicket,
) {
  const row = await db
    .prepare(
      "SELECT branch_id,department_id,version FROM support_tickets WHERE id=? AND company_id=?",
    )
    .bind(link.ticketId, companyId)
    .first<{ branch_id: string; department_id: string | null; version: number }>();
  if (!row) throw new Error("SUPPORT_INCIDENT_SEED_TICKET_NOT_FOUND");
  return row;
}

export async function seedSupportIncident(
  db: SqlDatabase,
  actor: SupportIncidentSeedActor,
  input: SupportIncidentSeedInput = {},
) {
  const now = Date.now();
  const id = input.id ?? supportIncidentSeedId();
  const status = input.status ?? "open";
  const links = input.tickets ?? [];
  const sources = new Map<
    string,
    { branch_id: string; department_id: string | null; version: number }
  >();
  for (const link of links)
    sources.set(link.ticketId, await ticketSource(db, actor.companyId, link));
  const first = links.length ? sources.get(links[0].ticketId)! : null;
  const branchId = input.branchId ?? first?.branch_id ?? actor.branchId;
  if (!branchId) throw new Error("SUPPORT_INCIDENT_SEED_BRANCH_REQUIRED");
  const departmentId =
    input.departmentId !== undefined
      ? input.departmentId
      : (first?.department_id ?? actor.departmentId);
  const actions = input.events?.map(event => event.action) ?? defaultActions[status];
  const version = input.version ?? actions.length;
  const events: SupportIncidentSeedEvent[] =
    input.events ??
    Array.from({ length: version }, (_, index) => ({
      action:
        index === 0
          ? "create"
          : index === version - 1
            ? actions[actions.length - 1]
            : "advance",
      reason:
        index === 0
          ? "Seeded shared incident"
          : index === version - 1
            ? `Seeded ${actions[actions.length - 1]} state`
            : "Seeded owner state",
      status,
    }));
  if (events.length !== version)
    throw new Error("SUPPORT_INCIDENT_SEED_EVENT_VERSION_MISMATCH");
  const review = status === "mitigated" || status === "closed";
  const ownerId = input.ownerId ?? actor.userId;
  const rootCause =
    input.rootCause ??
    (review ? "Seeded shared root cause" : null);
  const mitigation =
    input.mitigation ??
    (review ? "Seeded verified mitigation" : null);
  const mitigatedBy =
    input.mitigatedBy ??
    (review ? ownerId : null);
  const createdAt = input.createdAt ?? now;
  const updatedAt = input.updatedAt ?? createdAt + Math.max(0, version - 1);
  const statements = [
    db
      .prepare(
        "INSERT INTO support_incidents(id,company_id,branch_id,department_id,title,impact,severity,owner_id,due_date,status,root_cause,mitigation,mitigated_by,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
      )
      .bind(
        id,
        actor.companyId,
        branchId,
        departmentId ?? null,
        input.title ?? "Seeded shared incident",
        input.impact ?? "Seeded synthetic impact",
        input.severity ?? "normal",
        ownerId,
        input.dueDate ?? "2026-10-20",
        status,
        rootCause,
        mitigation,
        mitigatedBy,
        version,
        input.createdBy ?? actor.userId,
        createdAt,
        updatedAt,
      ),
    ...events.map((event, index) =>
      db
        .prepare(
          "INSERT INTO support_incident_events(id,incident_id,version,action,actor_id,reason,created_at,evidence_json) VALUES(?,?,?,?,?,?,?,?::jsonb)",
        )
        .bind(
          `${id}-event-${index + 1}`,
          id,
          event.version ?? index + 1,
          event.action,
          event.actorId ?? actor.userId,
          event.reason ?? "Seeded incident event",
          event.createdAt ?? createdAt + index,
          JSON.stringify(event.evidence ?? { seeded: true }),
        ),
    ),
    ...links.map(link =>
      db
        .prepare(
          "INSERT INTO support_incident_tickets(incident_id,ticket_id,linked_version,linked_by,linked_at,active) VALUES(?,?,?,?,?,?)",
        )
        .bind(
          id,
          link.ticketId,
          link.linkedVersion ?? sources.get(link.ticketId)!.version,
          link.linkedBy ?? actor.userId,
          link.linkedAt ?? now,
          link.active ?? true,
        ),
    ),
  ];
  await db.batch(statements);
  const row = await db
    .prepare("SELECT * FROM support_incidents WHERE id=?")
    .bind(id)
    .first<Record<string, unknown>>();
  return { id, version, status, row: row! };
}

/**
 * Moves a seeded owner incident to the state a retained reader needs. This is
 * owner-row seeding for the native command, not a TypeScript command
 * implementation: the native Rust PostgreSQL suite owns transition validation.
 */
export async function seedSupportIncidentState(
  db: SqlDatabase,
  id: string,
  input: {
    action: string;
    status: SupportIncidentSeedState;
    ownerId?: string;
    dueDate?: string;
    rootCause?: string | null;
    mitigation?: string | null;
    mitigatedBy?: string | null;
    reason?: string;
    actorId?: string;
    now?: number;
    evidence?: unknown;
  },
) {
  const current = await db
    .prepare("SELECT * FROM support_incidents WHERE id=?")
    .bind(id)
    .first<Record<string, unknown>>();
  if (!current) throw new Error("SUPPORT_INCIDENT_SEED_NOT_FOUND");
  const now = input.now ?? Date.now();
  const version = Number(current.version) + 1;
  const review = input.status === "mitigated" || input.status === "closed";
  const rootCause =
    input.rootCause ??
    (review
      ? ((current.root_cause as string | null) ?? "Seeded shared root cause")
      : (current.root_cause as string | null));
  const mitigation =
    input.mitigation ??
    (review
      ? ((current.mitigation as string | null) ?? "Seeded verified mitigation")
      : (current.mitigation as string | null));
  const mitigatedBy =
    input.mitigatedBy ??
    (review
      ? ((current.mitigated_by as string | null) ?? (input.actorId ?? "seed-actor"))
      : (current.mitigated_by as string | null));
  if (review && (!rootCause || !mitigation || !mitigatedBy))
    throw new Error("SUPPORT_INCIDENT_SEED_REVIEW_REQUIRED");
  const ownerId = input.ownerId ?? (current.owner_id as string);
  const dueDate = input.dueDate ?? (current.due_date as string);
  await db.batch([
    db
      .prepare(
        "UPDATE support_incidents SET status=?,owner_id=?,due_date=?,root_cause=?,mitigation=?,mitigated_by=?,version=?,updated_at=? WHERE id=?",
      )
      .bind(
        input.status,
        ownerId,
        dueDate,
        rootCause,
        mitigation,
        mitigatedBy,
        version,
        now,
        id,
      ),
    db
      .prepare(
        "INSERT INTO support_incident_events(id,incident_id,version,action,actor_id,reason,created_at,evidence_json) VALUES(?,?,?,?,?,?,?,?::jsonb)",
      )
      .bind(
        `${id}-event-${version}`,
        id,
        version,
        input.action,
        input.actorId ?? "seed-actor",
        input.reason ?? `Seeded ${input.action}`,
        now,
        JSON.stringify(input.evidence ?? { seeded: true }),
      ),
  ]);
  return { id, version, status: input.status };
}
