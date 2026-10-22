import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { StaffActor } from "@erpx/server-operations/public/staff-policy";
import { pmsSourceRecipients as recipients } from "@erpx/server-identity_access/public/pms-source-recipients";
import { ApiError } from "@erpx/server-runtime/public/http";
import { pmsSourcesSql, pmsSourceReadableSql } from "@erpx/server-pms/adapters/pms/source-policy";
import { createPmsSourceApplication } from "@erpx/server-pms/application/source";
import type { PmsSource } from "@erpx/server-pms/domain/source";
import { pmsPackageEnforcementMode, readPurchasedAccess } from "@erpx/server-commercial/public/purchase-command-boundary";
import { purchasedCapabilityPredicate, packageClockSql } from "@erpx/server-commercial/public/purchased-capability-sql";
export { decodePmsSource, pmsSourceLink } from "@erpx/server-pms/domain/source";
export type { PmsSource } from "@erpx/server-pms/domain/source";
function application(db: SqlDatabase) {
    return createPmsSourceApplication({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message), repository: { source: async (actor, id) => {
      if (!(await readPurchasedAccess(db,await pmsPackageEnforcementMode(),{companyId:actor.companyId,capability:'pms.front-desk'})).history) return null;
      return db.prepare(`SELECT pr.* FROM (SELECT ?::text user_id) recipient CROSS JOIN (${pmsSourcesSql}) pr WHERE pr.id=? AND pr.company_id=?
    AND (?::text IS NULL OR pr.branch_id=?) AND ?::text IS NULL
    AND ${pmsSourceReadableSql("pr", "recipient.user_id", actor.assignmentId ? "?::text" : undefined)}`)
                .bind(actor.userId, id, actor.companyId, actor.branchId, actor.branchId, actor.departmentId, ...(actor.assignmentId ? [actor.assignmentId] : [])).first<PmsSource>(); }, recipients: async (actor, now) => {
                  if (!(await readPurchasedAccess(db,await pmsPackageEnforcementMode(),{companyId:actor.companyId,capability:'pms.front-desk'})).history) return [];
                  return recipients(db, actor, now);
                } } });
}
export function readablePmsSource(db: SqlDatabase, actor: StaffActor, id: string) { return application(db).readablePmsSource(actor, id); }
export function pmsSourceRecipients(db: SqlDatabase, actor: WorkContext) { return application(db).pmsSourceRecipients(actor); }

/** Set projection of the public source reader; never returns source bodies. */
export async function readablePmsSourceIds(actor: StaffActor) {
    if (!hasPermission(actor, "pms.read") || actor.departmentId) return { sql: "SELECT NULL::text id WHERE FALSE", bindings: [] as unknown[] };
    const can = (permission: string) => hasPermission(actor, permission), conditions: string[] = [];
    if (can("pms.housekeeping") || can("pms.inspect") || can("pms.maintenance")) conditions.push("pr.kind IN ('room','task','block')");
    if (can("pms.frontdesk") || can("pms.reservations") || can("pms.cashier")) conditions.push("pr.kind IN ('stay','reservation')");
    if (can("pms.cashier") || can("pms.reports")) conditions.push("pr.kind='guest_account'");
    if (can("pms.approve")) conditions.push("pr.kind IN ('refund','correction')");
    else if (can("pms.cashier")) conditions.push("pr.kind IN ('refund','correction') AND pr.data->>'requestedBy'=recipient.user_id");
    if (can("finance.approve")) conditions.push("pr.kind='transfer'");
    else if (can("pms.cashier")) conditions.push("pr.kind='transfer' AND pr.data->>'recordedBy'=recipient.user_id");
    if (can("pms.approve") || can("pms.audit")) conditions.push("pr.kind='shift'");
    else if (can("pms.cashier")) conditions.push("pr.kind='shift' AND pr.data->>'userId'=recipient.user_id");
    if (can("pms.audit") || can("pms.reports")) conditions.push("pr.kind='night'");
    const enforced=await pmsPackageEnforcementMode()==='enforced';
    const clock=enforced?`WITH purchase_clock AS MATERIALIZED (SELECT ${packageClockSql} observed_at) `:'',join=enforced?' CROSS JOIN purchase_clock':'';
    const purchase=enforced?purchasedCapabilityPredicate('pr.company_id','pms.front-desk','history','purchase_clock.observed_at'):'TRUE';
    return { sql: `${clock}SELECT pr.id FROM (SELECT ?::text user_id) recipient CROSS JOIN (${pmsSourcesSql}) pr${join} WHERE pr.company_id=? AND ${purchase}
      AND (?::text IS NULL OR pr.branch_id=?) AND (${conditions.length ? conditions.map(value => `(${value})`).join(" OR ") : "FALSE"})
      AND ${pmsSourceReadableSql("pr", "recipient.user_id", actor.assignmentId ? "?::text" : undefined)}`,
      bindings: [actor.userId, actor.companyId, actor.branchId, actor.branchId, ...(actor.assignmentId ? [actor.assignmentId] : [])] };
}
