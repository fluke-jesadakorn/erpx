import {holdingResources} from "../../runtime/public/runtime";
import {assetSourcePredicate} from "./record-purchases";
import { hasPermission } from "@/lib/identity_access/permissions";
import { hasModuleAccess } from "@/data/module-access";
import type { SqlDatabase } from '../../runtime/public/runtime/types';
import type { WorkContext } from '../../identity_access/public/authorization';
import { assertion } from '../../workflow/public/commands';
import { assignmentReadSql } from '../../identity_access/public/record-source-authority';
import type { SourceRow, AuthorizedSourceRecord } from '../../records/public/source-record-contract';
const definitions = { asset: { table: 'operational_assets', permission: 'records.read', title: 's.name', status: 's.lifecycle_status', department: 's.department_id' } } as const;
export function ownedRecordSource(database: SqlDatabase) {
    return { async read(a: WorkContext, kind: keyof typeof definitions, id: string) { const d = definitions[kind], department = d.department, purchase=await assetSourcePredicate(database,a,"s",kind); return database.prepare(`SELECT s.id,s.company_id,s.branch_id,${department} department_id,s.version,${d.title} title,${d.status} status FROM ${d.table} s WHERE s.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?::text IS NULL OR ${department}=?) AND ${assignmentReadSql("s", d.permission).replaceAll("s.department_id", department)} AND ${purchase.where}`).bind(id, a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId, a.assignmentId, a.userId, a.role,...purchase.args).first<SourceRow>(); }, async guards(a: WorkContext, key: string, source: AuthorizedSourceRecord & {
            kind: keyof typeof definitions;
        }, step: string) { const d = definitions[source.kind], department = d.department,purchase=await assetSourcePredicate(database,a,"s",source.kind); return [database.prepare(`SELECT id FROM ${d.table} WHERE id=? AND company_id=? FOR SHARE`).bind(source.id, a.companyId), assertion(database, key, step, `EXISTS(SELECT 1 FROM ${d.table} s WHERE s.id=? AND s.company_id=? AND s.version=? AND (?::text IS NULL OR s.branch_id=?) AND (?::text IS NULL OR ${department}=?) AND ${assignmentReadSql("s", d.permission).replaceAll("s.department_id", department)} AND ${purchase.where})`, [source.id, a.companyId, source.version, a.branchId, a.branchId, a.departmentId, a.departmentId, a.assignmentId, a.userId, a.role,...purchase.args])]; } };
}

/** Minimal source identifiers, with the same current authority as the record reader. */
export async function readableRecordSourceIds(a: WorkContext) {
    if (!hasPermission(a, "records.read") || !hasModuleAccess(a.modules, "assets")) return { sql: "SELECT NULL::text id WHERE FALSE", bindings: [] as unknown[] };
    const purchase=await assetSourcePredicate((await holdingResources(a.holdingId)).write,a,"s","asset");
    return { sql: `SELECT s.id FROM operational_assets s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?::text IS NULL OR s.department_id=?) AND ${assignmentReadSql("s", "records.read")} AND ${purchase.where}`,
        bindings: [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId, a.assignmentId, a.userId, a.role,...purchase.args] };
}
