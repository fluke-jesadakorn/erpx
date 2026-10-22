import {collaborationNotificationSnapshotSources as legalSnapshotSources} from '@erpx/server-legal/public/collaboration-notification-snapshot-sources';
const legalSources=legalSnapshotSources();
import "server-only";
/** This code-owned projection consumes the Legal owner's `added` CTE on its
 * existing transaction, so only newly created preservation notices notify. */
export function legalHoldNotificationInsert() { return `INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT 'hold-'||md5(n.hold_id||n.item_id||n.user_id),i.company_id,n.user_id,'legal',i.id,i.version,'A preservation notice requires acknowledgement',n.notified_at FROM added n ${legalSources.legal_hold_notifications_legal_items_i_join} ON i.id=n.item_id ON CONFLICT DO NOTHING`; }
