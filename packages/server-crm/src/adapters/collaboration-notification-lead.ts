import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function collaborationNotificationLead(db: SqlDatabase, record_id: string, companyId: string) { return db.prepare('SELECT lead_id FROM crm_lead_activities WHERE id=? AND company_id=?').bind(record_id, companyId).first<{
    lead_id: string;
}>(); }
