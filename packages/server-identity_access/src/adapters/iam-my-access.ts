import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import 'server-only';
import { activeHoldingBindings, holdingResources, runtimeEnv, type HoldingId } from '../../../../packages/server-runtime/src/public/runtime';
import { iamDb } from '@erpx/server-identity_access/public/iam-db';
import { iamRole } from '@erpx/server-identity_access/public/iam-store';
import { createOwnIamAccess } from '@erpx/server-identity_access/application/my-access';
export async function ownIamAccess(userId: string) {
    const databases = new Map<string, ReturnType<typeof iamDb>>();
    const database = async (holdingId: string | null) => { const key = holdingId ?? 'platform'; let db = databases.get(key); if (!db) {
        db = iamDb(holdingId ? (await holdingResources(holdingId as HoldingId)).write : (await runtimeEnv()).CONTROL_DB);
        databases.set(key, db);
    } return db; };
    return createOwnIamAccess({ holdings: activeHoldingBindings, now: Date.now, assignments: async (holdingId, userId, now) => {
            const db = await database(holdingId);
            return (await db
                .prepare(`SELECT a.*,n.display_name company_name,s.display_name scope_name,m.status identity_status,m.login_policy,iam_assignment_active(a.id,?) active FROM assignments a ${organizationSources.iam_my_access_organization_nodes_n_join} ON n.id=a.company_id LEFT ${organizationSources.iam_my_access_organization_nodes_s_join} ON s.id=a.scope_node_id LEFT JOIN iam_members m ON m.user_id=a.user_id AND m.company_id=a.company_id WHERE a.user_id=? ORDER BY n.display_name,a.role`)
                .bind(now, userId)
                .all<Record<string, unknown>>()).results;
        }, platformAssignments: async (userId, now) => {
            const db = await database(null);
            return (await db
                .prepare("SELECT *,iam_assignment_active(id,?) active FROM platform_assignments WHERE user_id=?")
                .bind(now, userId)
                .all<Record<string, unknown>>()).results;
        }, role: async (holdingId, companyId, id) => iamRole(await database(holdingId), companyId, id) })(userId);
}
