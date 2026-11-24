import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

import { packageSettlementSource, withPackageSettlementSources, type PackageSettlementSourceLock } from "@erpx/server-commercial/public/package-settlement-sources";
import { nativeSupportReadAll, SUPPORT_RD_PORTAL_BINDINGS_INSTALLED, SUPPORT_RD_SUPPORT_PACKAGE_PORTAL_BINDINGS, SUPPORT_RD_SUPPORT_PACKAGE_OBLIGATION_FACTS } from "@erpx/server-support/adapters/native-support-read";

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
  if((await nativeSupportReadAll<{installed:boolean}>(SUPPORT_RD_PORTAL_BINDINGS_INSTALLED, [], database)).results[0]?.installed){
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
  const portal=portalSources.has(transaction)?(await nativeSupportReadAll<{id:string;pending:boolean}>(SUPPORT_RD_SUPPORT_PACKAGE_PORTAL_BINDINGS, [input.now,input.companyId], transaction)).results:[];
  const facts = input.packageId==='support-portal'?[]:(await nativeSupportReadAll<Fact>(SUPPORT_RD_SUPPORT_PACKAGE_OBLIGATION_FACTS, [input.companyId, input.companyId, input.companyId, input.now, input.companyId, input.now], transaction)).results;
  // Counts/revisions leave the owner. Customer content, staff identities and
  // private evidence never enter a Commercial observation.
  const revision = createHash("sha256").update(JSON.stringify({ companyId: input.companyId,
    packageId: input.packageId, packageVersion: input.packageVersion, facts, portal })).digest("hex");
  return { companyId: input.companyId, packageId: input.packageId, moduleId: "support", observedAt: input.now,
    revision, outstanding: facts.length+portal.filter(row=>row.pending).length };
}
