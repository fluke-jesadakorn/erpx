import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { nativeCrmReadFirst, CRM_RD_COLLAB_NOTIFICATION_LEAD } from './native-crm-read';
/** Owner-native read: the statement is planned by the CRM native read port
 * (`RD_COLLAB_NOTIFICATION_LEAD`) and executed on the caller's open unit; this
 * module carries no SQL for this read. */
export function collaborationNotificationLead(db: SqlDatabase, record_id: string, companyId: string) {
    return nativeCrmReadFirst<{ lead_id: string }>(CRM_RD_COLLAB_NOTIFICATION_LEAD, [record_id, companyId], db);
}
