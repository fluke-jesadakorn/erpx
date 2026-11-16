import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function collaborationNotificationLead(db: SqlDatabase, record_id: string, companyId: string) { return db.prepare("SELECT 1").bind().first<{
    lead_id: string;
}>(); }
