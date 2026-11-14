import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

/**
 * Seeds the Support session/capability owner rows the native session command
 * persists. Native Rust PG suites own the command transitions; retained TS
 * readers are tested against these exact owner rows.
 */
export type SupportSessionSeedActor = {
  companyId: string;
  userId: string;
};

export type SeededCapability = {
  id?: string;
  code: "customer_order_context" | "ticket_evidence_metadata";
  status?: "active" | "revoked";
  version?: number;
  reason?: string;
  issuedBy?: string;
  revokedBy?: string | null;
  revokedAt?: number | null;
  createdAt?: number;
  updatedAt?: number;
};

const capabilityEvents: Record<string, string[]> = {
  active: ["issue"],
  "active-reissued": ["issue", "revoke", "reissue"],
  revoked: ["issue", "revoke", "reissue", "session-revoke"],
};

export async function seedSupportSession(
  db: SqlDatabase,
  actor: SupportSessionSeedActor,
  input: {
    id?: string;
    ticketId: string;
    ticketVersion?: number;
    requestedFor?: string;
    status?: "pending" | "approved" | "rejected" | "revoked";
    version?: number;
    expiresAt?: number;
    reason?: string;
    createdBy?: string;
    reviewedBy?: string | null;
    reviewedAt?: number | null;
    revokedBy?: string | null;
    revokedAt?: number | null;
    createdAt?: number;
    updatedAt?: number;
    capabilities?: SeededCapability[];
  },
) {
  const now = Date.now();
  const id = input.id ?? `support-session-${now}-${Math.random().toString(36).slice(2)}`;
  const ticket = await db
    .prepare("SELECT version,company_id,branch_id,department_id FROM support_tickets WHERE id=?")
    .bind(input.ticketId)
    .first<{ version: number; company_id: string; branch_id: string | null; department_id: string | null }>();
  if (!ticket) throw new Error("SUPPORT_SESSION_SEED_TICKET_NOT_FOUND");
  const status = input.status ?? "pending";
  const version = input.version ?? (status === "pending" ? 1 : status === "revoked" ? 3 : 2);
  const createdBy = input.createdBy ?? actor.userId;
  const reviewedBy = status === "pending" ? null : input.reviewedBy ?? actor.userId;
  if (reviewedBy === createdBy) throw new Error("SUPPORT_SESSION_SEED_REVIEWER_MUST_DIFFER");
  const reviewedAt = status === "pending" ? null : input.reviewedAt ?? now;
  const revokedBy = status === "revoked" ? input.revokedBy ?? actor.userId : null;
  const revokedAt = status === "revoked" ? input.revokedAt ?? now : null;
  const expiresAt = input.expiresAt ?? now + 60 * 60 * 1000;
  const createdAt = input.createdAt ?? now - 60_000;
  const updatedAt = input.updatedAt ?? now;
  if (expiresAt <= createdAt) throw new Error("SUPPORT_SESSION_SEED_EXPIRY_INVALID");
  await db
    .prepare(
      "INSERT INTO support_sessions(id,company_id,branch_id,department_id,ticket_id,ticket_version,requested_for,reason,expires_at,status,created_by,reviewed_by,reviewed_at,revoked_by,revoked_at,version,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(
      id,
      ticket.company_id,
      ticket.branch_id,
      ticket.department_id,
      input.ticketId,
      input.ticketVersion ?? ticket.version,
      input.requestedFor ?? actor.userId,
      input.reason ?? "Need the minimum customer context to resolve this case.",
      expiresAt,
      status,
      createdBy,
      reviewedBy,
      reviewedAt,
      revokedBy,
      revokedAt,
      version,
      createdAt,
      updatedAt,
    )
    .run();

  const actions: string[] =
    status === "pending"
      ? ["request"]
      : status === "approved"
        ? ["request", "approve"]
        : status === "rejected"
          ? ["request", "reject"]
          : ["request", "approve", "revoke"];
  while (actions.length < version) actions.push("revoke");
  await db.batch(
    actions.slice(0, version).map((action, index) =>
      db
        .prepare("INSERT INTO support_session_events(id,session_id,version,action,actor_id,reason,status,expires_at,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
        .bind(
          `${id}-event-${index + 1}`,
          id,
          index + 1,
          action,
          action === "request" ? createdBy : action === "approve" ? reviewedBy! : action === "reject" ? reviewedBy! : input.revokedBy ?? actor.userId,
          action === "request" ? input.reason ?? "Requested temporary access" : action === "approve" ? "Independently approved" : action === "reject" ? "Rejected" : "Revoked",
          index + 1 === version ? status : index === 0 ? "pending" : "approved",
          expiresAt,
          createdAt + index,
        ),
    ),
  );

  for (const capability of input.capabilities ?? []) {
    const capabilityId = capability.id ?? `${id}-capability-${capability.code}`;
    const capabilityStatus = capability.status ?? "active";
    const capabilityVersion = capability.version ?? (capabilityStatus === "active" ? 1 : 4);
    const issuedBy = capability.issuedBy ?? actor.userId;
    const revokedAt = capabilityStatus === "revoked" ? capability.revokedAt ?? now : null;
    await db
      .prepare(
        "INSERT INTO support_capabilities(id,session_id,code,status,reason,issued_by,revoked_by,revoked_at,version,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
      )
      .bind(
        capabilityId,
        id,
        capability.code,
        capabilityStatus,
        capability.reason ?? "Required for the current support case.",
        issuedBy,
        capabilityStatus === "revoked" ? capability.revokedBy ?? actor.userId : null,
        revokedAt,
        capabilityVersion,
        capability.createdAt ?? createdAt,
        capability.updatedAt ?? now,
      )
      .run();
    const eventActions = capabilityStatus === "active" && capabilityVersion >= 3
      ? capabilityEvents["active-reissued"]
      : capabilityStatus === "revoked"
        ? capabilityEvents.revoked
        : capabilityEvents.active;
    const events = eventActions.slice(0, capabilityVersion);
    await db.batch(
      events.map((action, index) =>
        db
          .prepare("INSERT INTO support_capability_events(id,capability_id,version,action,actor_id,reason,status,created_at) VALUES(?,?,?,?,?,?,?,?)")
          .bind(
            `${capabilityId}-event-${index + 1}`,
            capabilityId,
            index + 1,
            action,
            action === "session-revoke" ? input.revokedBy ?? actor.userId : issuedBy,
            action === "session-revoke" ? "Session revoked" : `Capability ${action}`,
            action === "revoke" || action === "session-revoke" ? "revoked" : "active",
            createdAt + index,
          ),
      ),
    );
  }

  return { id, version, status };
}
