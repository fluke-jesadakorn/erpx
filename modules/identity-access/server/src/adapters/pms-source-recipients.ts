import {identitySnapshotSources as peoplepayrollSnapshotSources} from '@erpx/people-payroll-server/public/identity-snapshot-sources';
import {pmsRecipientProjectionSql} from './pms-recipient-projection-family-r26';
const peoplepayrollSources=peoplepayrollSnapshotSources();
import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
export async function pmsSourceRecipients(db: SqlDatabase, actor: WorkContext, now: number) {
    return (await db.prepare(`${pmsRecipientProjectionSql(peoplepayrollSources.pms_source_recipients_hr_people_h_join)}
    AND h.status='active' AND h.merged_into_id IS NULL
    WHERE a.company_id=? AND a.user_id<>? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>?
    AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
    AND a.scope_kind IN ('holding','branch') AND (?::text IS NULL OR a.scope_kind='holding' OR a.scope_node_id=?)
    AND iam_role_allows(a.role,a.company_id,'pms.read') ORDER BY name,a.user_id LIMIT 100`)
        .bind(actor.companyId, actor.userId, now, now, now, now, actor.branchId, actor.branchId).all<{
        id: string;
        name: string;
    }>()).results;
}
