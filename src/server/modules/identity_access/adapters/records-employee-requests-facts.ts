import {identityVerifiedContextProjections as recordsFinalOwnerProjections} from "../../records/public/identity-access-verified-context-projections";
const recordsFinalSources=recordsFinalOwnerProjections();
import {identitySnapshotSources as supportSnapshotSources} from '../../support/public/identity-snapshot-sources';
const supportSources=supportSnapshotSources();
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { type EmployeeRequest } from '../../records/public/employee-requests-contract';
export function EmployeeRequestsIdentityAccessFacts(database: SqlDatabase) {
    return { readActOnEmployeeRequestEffect(company_id: string) {
            return database.prepare("SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND iam_role_allows(role,company_id,'company.owner') AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint)")
                .bind(company_id)
                .all<{
                user_id: string;
            }>();
        },
        readListEmployeeRequestsRows(companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null, value: 0 | 1, userId: string, userId2: string, userId3: string, userId4: string, userId5: string, userId6: string) {
            return database.prepare(`SELECT r.* ${recordsFinalSources.records_employee_requests_facts_employee_requests_r_from} WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) AND
    (?=1 OR iam_same_person(requester_user_id,?,r.company_id) OR iam_same_person(created_by,?,r.company_id) OR EXISTS(SELECT 1 ${supportSources.records_employee_requests_facts_service_team_members_m_from} WHERE m.team_id=r.team_id AND m.company_id=r.company_id AND m.user_id=?) OR EXISTS(SELECT 1 FROM erpx_json_each(r.approval_json,'$.pool') WHERE value=?) OR erpx_json_extract(r.approval_json,'$.decidedBy')=? OR EXISTS(SELECT 1 FROM jsonb_array_elements(COALESCE(r.approval_json::jsonb -> 'decisions','[]')) decision WHERE decision ->> 'userId'=?)) ORDER BY updated_at DESC,id LIMIT 150`)
                .bind(companyId, branchId, branchId2, departmentId, departmentId2, value, userId, userId2, userId3, userId4, userId5, userId6)
                .all<EmployeeRequest>();
        } };
}
