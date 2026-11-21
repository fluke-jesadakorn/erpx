import { randomUUID } from "node:crypto";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

/**
 * Seeds the owner rows the native Support evidence bind/revoke command persists.
 * Native Rust evidence PG suites own command validation; retained readers and
 * portal tests consume the resulting bound/revoked attachment and event rows.
 */
export async function bindSupportEvidenceSeed(
  db: SqlDatabase,
  actor: WorkContext,
  input: { ticketId: string; setId: string; expectedVersion?: number; reason?: string; commandId?: string },
) {
  const now = Date.now(), commandId = input.commandId ?? `seed-evidence-${randomUUID()}`;
  const row = await db
    .prepare(
      `SELECT s.purpose_key,l.id link_id,l.attachment_id source_id,a.checksum_sha256,a.media_type,a.bytes,s.state set_state,l.state link_state
       FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id
       JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
       WHERE s.id=? AND s.company_id=? AND s.state IN ('draft','bound')`,
    )
    .bind(input.setId, actor.companyId)
    .first<{ purpose_key: string; link_id: string; source_id: string; checksum_sha256: string; media_type: string; bytes: number; set_state: string; link_state: string }>();
  if (!row) throw new Error("SUPPORT_EVIDENCE_SEED_SET_NOT_FOUND");
  const ticket = await db
    .prepare("SELECT version FROM support_tickets WHERE id=? AND company_id=?")
    .bind(input.ticketId, actor.companyId)
    .first<{ version: number }>();
  if (!ticket) throw new Error("SUPPORT_EVIDENCE_SEED_TICKET_NOT_FOUND");
  const version = Number(ticket.version) + 1;
  const manifest = [{ linkId: row.link_id, sourceId: row.source_id, purposeKey: row.purpose_key, checksumSha256: row.checksum_sha256, mediaType: row.media_type, bytes: Number(row.bytes) }];
  if (row.set_state === "bound" && row.link_state === "bound") return { version: Number(ticket.version), manifest };
  if (input.expectedVersion !== undefined && Number(ticket.version) !== input.expectedVersion)
    throw new Error("SUPPORT_EVIDENCE_SEED_VERSION_CHANGED");
  await db.batch([
    db.prepare("UPDATE attachment_sets SET state='bound',bound_at=?,bound_command_id=?,updated_at=? WHERE id=? AND company_id=? AND state='draft'").bind(now, commandId, now, input.setId, actor.companyId),
    db.prepare("UPDATE attachment_links SET state='bound',bound_at=?,binding_type='support_action',binding_id=? WHERE id=? AND state='draft'").bind(now, `support-evidence:${commandId}`, row.link_id),
    db.prepare("UPDATE support_tickets SET version=?,updated_at=? WHERE id=? AND company_id=?").bind(version, now, input.ticketId, actor.companyId),
    db.prepare("INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json) SELECT ?,id,?,'attachment-bind',?,?,owner_id,due_date,status,?,?::jsonb FROM support_tickets WHERE id=?").bind(
      `support-evidence-${commandId}`,
      version,
      actor.userId,
      input.reason ?? "Seeded reviewed support evidence",
      now,
      JSON.stringify({ manifest }),
      input.ticketId,
    ),
  ]);
  return { version, manifest };
}

export async function revokeSupportEvidenceSeed(
  db: SqlDatabase,
  actor: WorkContext,
  input: { ticketId: string; attachmentId: string; expectedVersion?: number; reason?: string },
) {
  const now = Date.now(), commandId = `seed-evidence-revoke-${randomUUID()}`;
  const ticket = await db
    .prepare("SELECT version FROM support_tickets WHERE id=? AND company_id=?")
    .bind(input.ticketId, actor.companyId)
    .first<{ version: number }>();
  if (!ticket) throw new Error("SUPPORT_EVIDENCE_SEED_TICKET_NOT_FOUND");
  if (input.expectedVersion !== undefined && Number(ticket.version) !== input.expectedVersion)
    throw new Error("SUPPORT_EVIDENCE_SEED_VERSION_CHANGED");
  const version = Number(ticket.version) + 1;
  await db.batch([
    db.prepare("UPDATE attachments SET revoked_at=? WHERE id=? AND owner_type='support_ticket' AND owner_id=?").bind(now, input.attachmentId, input.ticketId),
    db.prepare("UPDATE support_tickets SET version=?,updated_at=? WHERE id=? AND company_id=?").bind(version, now, input.ticketId, actor.companyId),
    db.prepare("INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json) SELECT ?,id,?,'attachment-revoke',?,?,owner_id,due_date,status,?,?::jsonb FROM support_tickets WHERE id=?").bind(
      `support-evidence-${commandId}`,
      version,
      actor.userId,
      input.reason ?? "Seeded reviewed support evidence withdrawal",
      now,
      JSON.stringify({ attachmentId: input.attachmentId }),
      input.ticketId,
    ),
  ]);
  return { version };
}
