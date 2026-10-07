import type { SqlDatabase } from '../../runtime/public/runtime/types';
export function collaborationTextSources(db: SqlDatabase) { return { async activeMentionAccounts(companyId: string, now: number, now2: number) { return db.prepare("SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>?").bind(companyId, now, now2).all<{
        user_id: string;
    }>(); } }; }
