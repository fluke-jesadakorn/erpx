import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

/**
 * Seeds the owner rows the native Support SLA command persists. The native Rust
 * command suite owns policy approval and clock attachment; retained TypeScript
 * readers are tested against these approved/attached owner rows.
 */
export type SupportPolicySeedActor = { companyId: string; userId: string };

export async function seedSupportPolicy(
  db: SqlDatabase,
  actor: SupportPolicySeedActor,
  input: {
    id?: string;
    name: string;
    policy: unknown;
    status?: "draft" | "approved" | "retired";
    createdBy?: string;
    approvedBy?: string;
    version?: number;
  },
) {
  const now = Date.now();
  const id = input.id ?? `support-policy-${now}-${Math.random().toString(36).slice(2)}`;
  const status = input.status ?? "approved";
  const version = input.version ?? (status === "approved" ? 2 : 1);
  const createdBy = input.createdBy ?? (status === "approved" ? `seed-policy-author-${id}` : actor.userId);
  const approvedBy = status === "approved" ? input.approvedBy ?? actor.userId : null;
  if (status === "approved" && approvedBy === createdBy) throw new Error("SUPPORT_POLICY_APPROVER_MUST_DIFFER");
  await db.batch([
    db
      .prepare(
        "INSERT INTO support_sla_policies(id,company_id,name,policy_json,status,version,created_by,approved_by,approved_at,created_at,updated_at) VALUES(?,?,?,?::jsonb,?,?,?,?,?,?,?)",
      )
      .bind(id, actor.companyId, input.name, JSON.stringify(input.policy), status, version, createdBy, approvedBy, approvedBy ? now : null, now, now),
    db
      .prepare("INSERT INTO support_sla_policy_events(id,policy_id,version,action,actor_id,reason,created_at) VALUES(?,?,1,'create',?,?,?)")
      .bind(`${id}-event-1`, id, createdBy, "Seeded policy create", now),
    ...(status === "approved"
      ? [
          db
            .prepare("INSERT INTO support_sla_policy_events(id,policy_id,version,action,actor_id,reason,created_at) VALUES(?,?,2,'approve',?,?,?)")
            .bind(`${id}-event-2`, id, approvedBy, "Seeded independent approval", now),
        ]
      : []),
  ]);
  return { id, version, status };
}

export async function seedSupportClock(
  db: SqlDatabase,
  ticketId: string,
  input: { policyId: string; policyVersion: number; clock: unknown; actorId: string; reason?: string },
) {
  const now = Date.now();
  const current = await db.prepare("SELECT version,status,sla_policy_id FROM support_tickets WHERE id=?").bind(ticketId).first<{ version: number; status: string; sla_policy_id: string | null }>();
  if (!current) throw new Error("SUPPORT_SEED_TICKET_NOT_FOUND");
  if (current.sla_policy_id) throw new Error("SUPPORT_SEED_CLOCK_ALREADY_ATTACHED");
  const version = Number(current.version) + 1;
  await db.batch([
    db
      .prepare("UPDATE support_tickets SET sla_policy_id=?,sla_policy_version=?,sla_clock_json=?::jsonb,version=?,updated_at=? WHERE id=?")
      .bind(input.policyId, input.policyVersion, JSON.stringify(input.clock), version, now, ticketId),
    db
      .prepare("INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json) SELECT ?,id,?,'sla-attach',?,?,owner_id,due_date,status,?,? FROM support_tickets WHERE id=?")
      .bind(
        `support-sla-event-${ticketId}-${version}`,
        version,
        input.actorId,
        input.reason ?? "Seeded reviewed target attachment",
        now,
        JSON.stringify({ policyId: input.policyId, policyVersion: input.policyVersion, clock: input.clock, reason: input.reason ?? "Seeded reviewed target attachment" }),
        ticketId,
      ),
  ]);
  return { id: ticketId, version };
}
