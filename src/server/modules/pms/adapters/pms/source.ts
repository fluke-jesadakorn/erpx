import "server-only";
import type { SqlDatabase } from "../../../runtime/public/runtime/types";
import type { WorkContext } from "../../../identity_access/public/authorization";
import type { StaffActor } from "../../../operations/public/staff-policy";
import { pmsSourceRecipients as recipients } from "../../../identity_access/public/pms-source-recipients";
import { ApiError } from "../../../runtime/public/http";
import { pmsSourcesSql, pmsSourceReadableSql } from "./source-policy";
import { createPmsSourceApplication } from "../../application/source";
import type { PmsSource } from "../../domain/source";
export { decodePmsSource, pmsSourceLink } from "../../domain/source";
export type { PmsSource } from "../../domain/source";
function application(db: SqlDatabase) {
    return createPmsSourceApplication({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message), repository: { source: (actor, id) => db.prepare(`SELECT pr.* FROM (SELECT ?::text user_id) recipient CROSS JOIN (${pmsSourcesSql}) pr WHERE pr.id=? AND pr.company_id=?
    AND (?::text IS NULL OR pr.branch_id=?) AND ?::text IS NULL
    AND ${pmsSourceReadableSql("pr", "recipient.user_id", actor.assignmentId ? "?::text" : undefined)}`)
                .bind(actor.userId, id, actor.companyId, actor.branchId, actor.branchId, actor.departmentId, ...(actor.assignmentId ? [actor.assignmentId] : [])).first<PmsSource>(), recipients: (actor, now) => recipients(db, actor, now) } });
}
export function readablePmsSource(db: SqlDatabase, actor: StaffActor, id: string) { return application(db).readablePmsSource(actor, id); }
export function pmsSourceRecipients(db: SqlDatabase, actor: WorkContext) { return application(db).pmsSourceRecipients(actor); }
