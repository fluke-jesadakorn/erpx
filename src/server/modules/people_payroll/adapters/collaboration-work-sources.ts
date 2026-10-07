import { collaborationReadProjection as identity } from '../../identity_access/public/collaboration-read-projections';
import type { SqlDatabase } from '../../runtime/public/runtime/types';
import type { Row } from '../../collaboration/public/collaboration-work';
export function collaborationWorkSources(db: SqlDatabase) {
    return { async eligiblePeople(companyId: string) {
            return db
                .prepare(`SELECT DISTINCT p.user_id id,p.legal_name name FROM hr_people p ${identity.workspaceAssignment} WHERE p.company_id=? AND p.status='active' AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.write') ORDER BY p.legal_name`)
                .bind(companyId)
                .all<Row>();
        } };
}
