import { finalRootOwnerProjectionSources as identityAccessSources } from "../../identity_access/public/final-root-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { WorkContext } from "../../identity_access/public/authorization";
import { requirePermission } from "../../identity_access/public/iam-permissions";
import type { CapabilityPurpose } from "../../commercial/public/packages";
import { nativeSupportRuntime } from "./command-runtime";

export type FieldServiceSourceTicket = { id: string; title: string; version: number; customer_id: string; branch_id: string; department_id: string | null };

/** Status/version are projected by the original deferred Service case bridge;
 * only its immutable source and owner identity join a Service source fence. */
export const fieldServiceTicketSourceRows = "SELECT id,company_id,branch_id,department_id,customer_id,source_kind,order_id,service_agreement_id,service_order_id FROM support_tickets";
export function fieldServiceTicketSourceReadLocks(database:SqlDatabase) {
  return [database.prepare("LOCK TABLE support_tickets,support_ticket_events IN SHARE ROW EXCLUSIVE MODE")];
}
/** Service's included source capability is not the Helpdesk management UI.
 * Return only the established handoff fields under current Company/location
 * and assignment authority. Do not expose case bodies or evidence here. */
export async function fieldServiceSourceTickets(database: SqlDatabase, actor: WorkContext, purpose: CapabilityPurpose): Promise<FieldServiceSourceTicket[]> {
  requirePermission(actor, "records.read");
  const access = await nativeSupportRuntime(actor, database).purchase("support.source-ticket");
  if (!(purpose === "new-work" ? access.newWork : purpose === "settle-existing" ? access.settleExisting : access.history)) return [];
  return (await database.prepare(`SELECT t.id,t.title,t.version,t.customer_id,t.branch_id,t.department_id FROM support_tickets t
    WHERE t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?)
    AND t.source_kind='sales_order' AND t.status IN ('open','in_progress','waiting_customer')
    AND EXISTS(SELECT 1 FROM (${identityAccessSources.assignments}) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=t.company_id
      AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_role_allows(g.role,g.company_id,'records.read')
      AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=t.branch_id OR g.scope_kind='department' AND g.scope_node_id=t.department_id))
    ORDER BY t.updated_at DESC,t.id`).bind(actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId,
    actor.assignmentId, actor.userId, actor.role).all<FieldServiceSourceTicket>()).results;
}
