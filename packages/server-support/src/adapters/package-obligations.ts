import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

import { packageSettlementSource, withPackageSettlementSources, type PackageSettlementSourceLock } from "@erpx/server-commercial/public/package-settlement-sources";

type Input = { companyId: string; packageId: string; packageVersion: number; moduleId: string; now: number };
type Fact = { kind: string; id: string; version: number; state: string };
const tables = ["support_tickets", "support_incidents", "support_sessions", "support_capabilities", "support_sla_alerts"] as const;
const sources = new WeakMap<SqlDatabase, string>();
const portalSources = new WeakSet<SqlDatabase>();

/** Own the first snapshot, including absent future rows. The reader below
 * cannot be attached to a transaction whose snapshot was already established.
 */
export function withSupportPackageObligations<Result>(owner: PostgresDatabase, companyId: string, run: (transaction: SqlDatabase) => Promise<Result>) {
  if (!companyId) throw new Error("SUPPORT_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
  return withPackageSettlementSources(owner, companyId, [lock => lockSupportPackageObligationSources(owner, lock)], run);
}
export async function lockSupportPackageObligationSources(owner: PostgresDatabase, lock: PackageSettlementSourceLock) {
  const { database, companyId } = packageSettlementSource(owner, lock);
  await database.prepare('LOCK TABLE command_receipts,audit_events,outbox_events IN SHARE MODE').run();
  await database.prepare(`LOCK TABLE ${tables.join(",")} IN SHARE MODE`).run();
  // The enclosing settlement already holds the strong package fence. All new
  // Portal writers require that same fence, so this pre-install catalogue read
  // cannot race a normal Portal birth; ordinary pre-1065 paths remain valid.
  if(await database.prepare("SELECT to_regclass('support_ticket_portal_bindings') IS NOT NULL installed").first<boolean>('installed')){
    await database.prepare('LOCK TABLE support_ticket_portal_bindings,support_ticket_portal_events,support_ticket_portal_identities,support_ticket_portal_reads,support_ticket_portal_responses,support_ticket_portal_acknowledgements IN SHARE MODE').run();
    portalSources.add(database);
  }
  sources.set(database, companyId);
}

/** A Company-wide settlement observation, never a user's permission projection.
 * Keep both existing rows and absent future rows stable until the caller commits.
 * The owner wrapper must establish these sources before the first snapshot.
 */
export async function supportPackageObligations(owner: PostgresDatabase, transaction: SqlDatabase, input: Input) {
  owner.assertActiveTransaction(transaction);
  if (sources.get(transaction) !== input.companyId) throw new Error("SUPPORT_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
  // Field Service keeps its original handoff owner. The Portal add-on observes
  // only its own reviewed access; it never inherits ticket/SLA obligations.
  if (input.moduleId !== "support" || !["support","support-portal"].includes(input.packageId) || input.packageVersion !== 1 || !input.companyId ||
    !Number.isSafeInteger(input.now) || input.now < 0) throw new Error("SUPPORT_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
  const portal=portalSources.has(transaction)?(await transaction.prepare(`SELECT binding.id,to_jsonb(binding) binding,
    (SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM support_ticket_portal_events event WHERE event.binding_id=binding.id) events,
    (SELECT jsonb_agg(to_jsonb(identity) ORDER BY identity.binding_id) FROM support_ticket_portal_identities identity WHERE identity.binding_id=binding.id) identities,
    (SELECT jsonb_agg(to_jsonb(response) ORDER BY response.id) FROM support_ticket_portal_responses response WHERE response.binding_id=binding.id) responses,
    (SELECT jsonb_agg(to_jsonb(ack) ORDER BY ack.id) FROM support_ticket_portal_acknowledgements ack WHERE ack.binding_id=binding.id) acknowledgements,
    binding.status='draft' OR binding.status='approved' AND binding.expires_at>? pending
    FROM support_ticket_portal_bindings binding WHERE binding.company_id=? ORDER BY binding.id`).bind(input.now,input.companyId).all<{id:string;pending:boolean}>()).results:[];
  const facts = input.packageId==='support-portal'?[]:(await transaction.prepare(`
    SELECT 'ticket' kind,id,version,status state FROM support_tickets
      WHERE company_id=? AND source_kind<>'field_service' AND status NOT IN ('closed','cancelled')
    UNION ALL
    SELECT 'incident',id,version,status FROM support_incidents
      WHERE company_id=? AND status NOT IN ('closed','cancelled')
    UNION ALL
    SELECT 'session',id,version,status FROM support_sessions
      WHERE company_id=? AND status IN ('pending','approved') AND expires_at>?
    UNION ALL
    SELECT 'sla',t.id,COALESCE(a.version,0),CASE WHEN a.id IS NULL THEN 'undrained' ELSE 'unacknowledged' END
      FROM support_tickets t LEFT JOIN support_sla_alerts a ON a.ticket_id=t.id
      WHERE t.company_id=? AND a.acknowledged_by IS NULL AND (a.id IS NOT NULL
        OR (t.sla_clock_json->>'breachedAt')::bigint IS NOT NULL
        OR t.sla_clock_json->>'state'='running' AND (
          (t.sla_clock_json->>'dueAt')::bigint<? OR t.status IN ('closed','cancelled')))
    ORDER BY kind,id`).bind(input.companyId, input.companyId, input.companyId, input.now, input.companyId, input.now).all<Fact>()).results;
  // Counts/revisions leave the owner. Customer content, staff identities and
  // private evidence never enter a Commercial observation.
  const revision = createHash("sha256").update(JSON.stringify({ companyId: input.companyId,
    packageId: input.packageId, packageVersion: input.packageVersion, facts, portal })).digest("hex");
  return { companyId: input.companyId, packageId: input.packageId, moduleId: "support", observedAt: input.now,
    revision, outstanding: facts.length+portal.filter(row=>row.pending).length };
}
