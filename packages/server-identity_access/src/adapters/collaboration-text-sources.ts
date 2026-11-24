import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {distinctAssigneeSql} from './read-tail-family-r29';
export function collaborationTextSources(db: SqlDatabase) { return { async activeMentionAccounts(companyId: string, now: number, now2: number) { return db.prepare(distinctAssigneeSql("revoked_at IS NULL AND valid_from<=? AND valid_until>?")).bind(companyId, now, now2).all<{
        user_id: string;
    }>(); } }; }
