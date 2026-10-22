import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function StaffTasksCollaborationFacts(database: SqlDatabase) {
    return {
        readTransitionTaskFollowers(id: string) {
            return database.prepare("SELECT DISTINCT principal_id FROM social_follows WHERE target_type='post' AND target_id=?").bind(id).all<{
                principal_id: string;
            }>();
        }
    };
}
