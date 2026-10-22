import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { assignmentReadSql } from '@erpx/server-identity_access/public/record-source-authority';
import type { SourceRow, AuthorizedSourceRecord } from '@erpx/server-records/public/source-record-contract';
import { posPackageEnforcementMode, readPurchasedAccess } from '@erpx/server-commercial/public/purchase-command-boundary';
import { purchasedCapabilityPredicate, packageClockSql } from '@erpx/server-commercial/public/purchased-capability-sql';
const definitions = { pos_sale: { table: 'pos_sales', permission: 'pos.read', title: 's.number', status: 's.state', department: 'NULL::text' } } as const;
export function ownedRecordSource(database: SqlDatabase) {
    return { async read(a: WorkContext, kind: keyof typeof definitions, id: string) { if(!(await readPurchasedAccess(database,await posPackageEnforcementMode(),{companyId:a.companyId,capability:'pos.sales'})).history)return null;const d = definitions[kind], department = d.department; return database.prepare(`SELECT s.id,s.company_id,s.branch_id,${department} department_id,s.version,${d.title} title,${d.status} status FROM ${d.table} s WHERE s.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?::text IS NULL OR ${department}=?) AND ${assignmentReadSql("s", d.permission).replaceAll("s.department_id", department)}`).bind(id, a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId, a.assignmentId, a.userId, a.role).first<SourceRow>(); }, guards(a: WorkContext, key: string, source: AuthorizedSourceRecord & {
            kind: keyof typeof definitions;
        }, step: string) { const d = definitions[source.kind], department = d.department; return [database.prepare(`SELECT id FROM ${d.table} WHERE id=? AND company_id=? FOR SHARE`).bind(source.id, a.companyId), assertion(database, key, step, `EXISTS(SELECT 1 FROM ${d.table} s WHERE s.id=? AND s.company_id=? AND s.version=? AND (?::text IS NULL OR s.branch_id=?) AND (?::text IS NULL OR ${department}=?) AND ${assignmentReadSql("s", d.permission).replaceAll("s.department_id", department)})`, [source.id, a.companyId, source.version, a.branchId, a.branchId, a.departmentId, a.departmentId, a.assignmentId, a.userId, a.role])]; } };
}

/** Minimal source identifiers, with the same current authority as the record reader. */
export async function readableRecordSourceIds(a: WorkContext) {
    if (!hasPermission(a, "pos.read") || a.departmentId) return { sql: "SELECT NULL::text id WHERE FALSE", bindings: [] as unknown[] };
    const enforced=await posPackageEnforcementMode()==='enforced';
    const clock=enforced?`WITH purchase_clock AS MATERIALIZED (SELECT ${packageClockSql} observed_at) `:'',join=enforced?' CROSS JOIN purchase_clock':'';
    const purchase=enforced?purchasedCapabilityPredicate('s.company_id','pos.sales','history','purchase_clock.observed_at'):'TRUE';
    return { sql: `${clock}SELECT s.id FROM pos_sales s${join} WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?::text IS NULL OR NULL::text=?) AND ${purchase} AND ${assignmentReadSql("s", "pos.read").replaceAll("s.department_id", "NULL::text")}`,
        bindings: [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId, a.assignmentId, a.userId, a.role] };
}
