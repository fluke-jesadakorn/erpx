/**
 * FIXTURE-ONLY ORACLE — retained pre-cutover inventory transit handoff notice.
 *
 * Production notice generation now belongs to the native
 * `inventory_transit_notification_statement` port (server-collaboration). This
 * copy seeds the same notification for the isolated legacy-transit harness
 * without a production TS writer in packages/server-collaboration/src.
 *
 * Tracked for removal by `retire.collaboration.residual-writers`: delete once
 * the legacy-transit harness is retired or seeds through the native port.
 */
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { inventoryReadProjection } from '@erpx/server-inventory/public/inventory-read-projections';
import { inventoryNotificationAudience } from '@erpx/server-identity_access/public/inventory-authority-projections';

export function inventoryTransitSources(db: SqlDatabase) {
  return {
    async prepareTransitNotice(key: string, id: string) {
      return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT DISTINCT ?||'-'||a.user_id,d.company_id,a.user_id,'inventory_transit',d.id,d.version,'Transport handoff updated: inspect dispatch, receipt and discrepancy evidence',d.updated_at ${inventoryReadProjection.transitNotificationSource} ${inventoryNotificationAudience.transitReaders} WHERE d.id=? ON CONFLICT DO NOTHING`)
        .bind(key, id);
    },
  };
}
