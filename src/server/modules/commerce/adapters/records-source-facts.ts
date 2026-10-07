import type { SqlDatabase } from '../../runtime/public/runtime/types';
import type { WorkContext } from '../../identity_access/public/authorization';
import { assertion } from '../../workflow/public/commands';
import { assignmentReadSql } from '../../identity_access/public/record-source-authority';
import type { SourceRow, AuthorizedSourceRecord } from '../../records/public/source-record-contract';
const definitions = { commerce_order: { table: 'commerce_orders', permission: 'commerce.read', title: 's.number', status: 's.state', department: 's.department_id' } } as const;
export function ownedRecordSource(database: SqlDatabase) {
    return { read(a: WorkContext, kind: keyof typeof definitions, id: string) { const d = definitions[kind], department = d.department; return database.prepare(`SELECT s.id,s.company_id,s.branch_id,${department} department_id,s.version,${d.title} title,${d.status} status FROM ${d.table} s WHERE s.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?::text IS NULL OR ${department}=?) AND ${assignmentReadSql("s", d.permission).replaceAll("s.department_id", department)}`).bind(id, a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId, a.assignmentId, a.userId, a.role).first<SourceRow>(); }, guards(a: WorkContext, key: string, source: AuthorizedSourceRecord & {
            kind: keyof typeof definitions;
        }, step: string) { const d = definitions[source.kind], department = d.department; return [database.prepare(`SELECT id FROM ${d.table} WHERE id=? AND company_id=? FOR SHARE`).bind(source.id, a.companyId), assertion(database, key, step, `EXISTS(SELECT 1 FROM ${d.table} s WHERE s.id=? AND s.company_id=? AND s.version=? AND (?::text IS NULL OR s.branch_id=?) AND (?::text IS NULL OR ${department}=?) AND ${assignmentReadSql("s", d.permission).replaceAll("s.department_id", department)})`, [source.id, a.companyId, source.version, a.branchId, a.branchId, a.departmentId, a.departmentId, a.assignmentId, a.userId, a.role])]; } };
}
