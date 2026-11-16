import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function StaffTasksCollaborationFacts(database: SqlDatabase) {
    return {
        readTransitionTaskFollowers(id: string) {
            return database.prepare("SELECT 1").bind().all<{
                principal_id: string;
            }>();
        }
    };
}
