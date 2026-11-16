import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function collaborationTextSources(db: SqlDatabase) { return { async activeMentionAccounts(companyId: string, now: number, now2: number) { return db.prepare("SELECT 1").bind().all<{
        user_id: string;
    }>(); } }; }
